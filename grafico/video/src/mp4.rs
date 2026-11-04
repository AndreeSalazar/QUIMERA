//! MP4 (ISO BMFF, ISO/IEC 14496-12): las pistas y sus muestras.
//!
//! Sirve para un archivo entero (las muestras en la tabla `stbl`) y para el MP4 fragmentado que
//! llega por MediaSource: un segmento de inicio (`ftyp` + `moov`) y después segmentos con
//! `moof` + `mdat`, que se van empujando a un [`Demuxer`].

use alloc::vec::Vec;

/// Qué hay en una pista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codec {
    /// H.264: la configuración `avcC` (con los SPS y PPS) y el tamaño de la imagen.
    Avc {
        /// El contenido de la caja `avcC`.
        avcc: Vec<u8>,
        /// Ancho en píxeles.
        ancho: u16,
        /// Alto en píxeles.
        alto: u16,
    },
    /// AAC: su `AudioSpecificConfig`.
    Aac {
        /// La configuración del decodificador (de `esds`).
        config: Vec<u8>,
        /// Canales.
        canales: u16,
        /// Muestras por segundo.
        frecuencia: u32,
    },
    /// Otro formato (`vp09`, `av01`, `Opus`...), con su código de cuatro letras.
    Otro([u8; 4]),
}

/// Una pista del archivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pista {
    /// Su número (`track_ID`).
    pub id: u32,
    /// Unidades de tiempo por segundo.
    pub escala: u32,
    /// Su formato.
    pub codec: Codec,
    /// Lo que valen las muestras de un fragmento que no lo dicen (`trex`): duración, tamaño, banderas.
    defecto: (u32, u32, u32),
}

/// Una muestra: un fotograma de vídeo o un trozo de audio comprimido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Muestra {
    /// La pista a la que pertenece.
    pub pista: u32,
    /// Cuándo se decodifica, en unidades de su pista.
    pub dts: u64,
    /// Cuándo se muestra (el `dts` más el desfase de composición).
    pub pts: i64,
    /// Cuánto dura.
    pub duracion: u32,
    /// Si se puede empezar a decodificar desde ella (fotograma clave).
    pub clave: bool,
    /// Sus bytes.
    pub datos: Vec<u8>,
}

/// Lo que puede fallar al leer un MP4.
pub type Error = &'static str;

/// Una caja: su tipo y dónde empieza y acaba su contenido dentro de los datos.
#[derive(Clone, Copy)]
struct Caja {
    tipo: [u8; 4],
    inicio: usize,
    cuerpo: usize,
    fin: usize,
}

/// Las cajas seguidas de `datos[desde..hasta]`. La última puede estar incompleta: no se devuelve.
fn cajas(datos: &[u8], desde: usize, hasta: usize) -> Vec<Caja> {
    let mut v = Vec::new();
    let mut i = desde;
    while i + 8 <= hasta {
        let tam = u32_en(datos, i) as usize;
        let tipo = [datos[i + 4], datos[i + 5], datos[i + 6], datos[i + 7]];
        let (tam, cabeza) = match tam {
            1 if i + 16 <= hasta => (u64_en(datos, i + 8) as usize, 16),
            1 => break,
            0 => (hasta - i, 8),
            t => (t, 8),
        };
        if tam < cabeza || i + tam > hasta {
            break;
        }
        v.push(Caja { tipo, inicio: i, cuerpo: i + cabeza, fin: i + tam });
        i += tam;
    }
    v
}

fn hija(datos: &[u8], c: Caja, tipo: &[u8; 4]) -> Option<Caja> {
    cajas(datos, c.cuerpo, c.fin).into_iter().find(|h| &h.tipo == tipo)
}

fn u16_en(d: &[u8], i: usize) -> u16 {
    d.get(i..i + 2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
}

fn u32_en(d: &[u8], i: usize) -> u32 {
    d.get(i..i + 4).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn u64_en(d: &[u8], i: usize) -> u64 {
    (u64::from(u32_en(d, i)) << 32) | u64::from(u32_en(d, i + 4))
}

/// Las pistas de un `moov`, con los valores por defecto de sus fragmentos (`mvex/trex`).
fn leer_moov(d: &[u8], moov: Caja) -> Vec<Pista> {
    let mut pistas: Vec<Pista> = cajas(d, moov.cuerpo, moov.fin)
        .into_iter()
        .filter(|c| &c.tipo == b"trak")
        .filter_map(|t| leer_trak(d, t))
        .collect();
    if let Some(mvex) = hija(d, moov, b"mvex") {
        for trex in cajas(d, mvex.cuerpo, mvex.fin).into_iter().filter(|c| &c.tipo == b"trex") {
            let b = trex.cuerpo;
            let id = u32_en(d, b + 4);
            if let Some(p) = pistas.iter_mut().find(|p| p.id == id) {
                p.defecto = (u32_en(d, b + 12), u32_en(d, b + 16), u32_en(d, b + 20));
            }
        }
    }
    pistas
}

fn leer_trak(d: &[u8], trak: Caja) -> Option<Pista> {
    let tkhd = hija(d, trak, b"tkhd")?;
    let id = u32_en(d, tkhd.cuerpo + if d[tkhd.cuerpo] == 1 { 20 } else { 12 });
    let mdia = hija(d, trak, b"mdia")?;
    let mdhd = hija(d, mdia, b"mdhd")?;
    let escala = u32_en(d, mdhd.cuerpo + if d[mdhd.cuerpo] == 1 { 20 } else { 12 });
    let stbl = hija(d, hija(d, mdia, b"minf")?, b"stbl")?;
    let stsd = hija(d, stbl, b"stsd")?;
    let entrada = *cajas(d, stsd.cuerpo + 8, stsd.fin).first()?;
    Some(Pista { id, escala, codec: leer_entrada(d, entrada), defecto: (0, 0, 0) })
}

/// La descripción de las muestras (`avc1`, `mp4a`...).
fn leer_entrada(d: &[u8], e: Caja) -> Codec {
    match &e.tipo {
        b"avc1" | b"avc3" => {
            let (ancho, alto) = (u16_en(d, e.cuerpo + 24), u16_en(d, e.cuerpo + 26));
            let avcc = cajas(d, e.cuerpo + 78, e.fin).into_iter().find(|c| &c.tipo == b"avcC");
            match avcc {
                Some(c) => Codec::Avc { avcc: d[c.cuerpo..c.fin].to_vec(), ancho, alto },
                None => Codec::Otro(e.tipo),
            }
        }
        b"mp4a" => {
            let canales = u16_en(d, e.cuerpo + 16);
            let frecuencia = u32_en(d, e.cuerpo + 24) >> 16;
            let esds = cajas(d, e.cuerpo + 28, e.fin).into_iter().find(|c| &c.tipo == b"esds");
            let config = esds.and_then(|c| config_aac(&d[c.cuerpo + 4..c.fin])).unwrap_or_default();
            Codec::Aac { config, canales, frecuencia }
        }
        otro => Codec::Otro(*otro),
    }
}

/// El `AudioSpecificConfig` de los descriptores de `esds` (el de etiqueta 5).
fn config_aac(mut d: &[u8]) -> Option<Vec<u8>> {
    // Cada descriptor: etiqueta, longitud en bytes de 7 bits y contenido. Los de ES (3) y de
    // configuración del decodificador (4) llevan una cabecera fija antes de sus hijos.
    while let Some((&etiqueta, resto)) = d.split_first() {
        let (mut largo, mut i) = (0usize, 0);
        while i < 4 {
            let b = *resto.get(i)?;
            largo = (largo << 7) | usize::from(b & 0x7f);
            i += 1;
            if b & 0x80 == 0 {
                break;
            }
        }
        let cuerpo = resto.get(i..i + largo.min(resto.len() - i))?;
        match etiqueta {
            3 => {
                let banderas = *cuerpo.get(2)?;
                let mut salto = 3;
                if banderas & 0x80 != 0 {
                    salto += 2;
                }
                if banderas & 0x40 != 0 {
                    salto += 1 + usize::from(*cuerpo.get(salto)?);
                }
                if banderas & 0x20 != 0 {
                    salto += 2;
                }
                d = cuerpo.get(salto..)?;
            }
            4 => d = cuerpo.get(13..)?,
            5 => return Some(cuerpo.to_vec()),
            _ => d = resto.get(i + largo..)?,
        }
    }
    None
}

/// Las muestras de la tabla `stbl` de una pista (un MP4 entero; `d` es el archivo completo).
fn muestras_de_tabla(d: &[u8], trak: Caja, id: u32) -> Option<Vec<Muestra>> {
    let mdia = hija(d, trak, b"mdia")?;
    let stbl = hija(d, hija(d, mdia, b"minf")?, b"stbl")?;
    let tabla = |t: &[u8; 4]| hija(d, stbl, t);
    // Tamaños.
    let stsz = tabla(b"stsz")?;
    let (fijo, n) = (u32_en(d, stsz.cuerpo + 4), u32_en(d, stsz.cuerpo + 8) as usize);
    let tam = |i: usize| if fijo != 0 { fijo } else { u32_en(d, stsz.cuerpo + 12 + 4 * i) };
    // Desplazamiento de cada trozo.
    let trozos: Vec<u64> = if let Some(c) = tabla(b"stco") {
        (0..u32_en(d, c.cuerpo + 4) as usize).map(|i| u64::from(u32_en(d, c.cuerpo + 8 + 4 * i))).collect()
    } else {
        let c = tabla(b"co64")?;
        (0..u32_en(d, c.cuerpo + 4) as usize).map(|i| u64_en(d, c.cuerpo + 8 + 8 * i)).collect()
    };
    // Muestras por trozo (`stsc`: desde qué trozo, cuántas).
    let stsc = tabla(b"stsc")?;
    let reglas: Vec<(usize, u32)> = (0..u32_en(d, stsc.cuerpo + 4) as usize)
        .map(|i| (u32_en(d, stsc.cuerpo + 8 + 12 * i) as usize, u32_en(d, stsc.cuerpo + 12 + 12 * i)))
        .collect();
    let mut sitio = Vec::with_capacity(n);
    for (t, &inicio) in trozos.iter().enumerate() {
        let por_trozo = reglas.iter().rev().find(|r| r.0 <= t + 1).map_or(0, |r| r.1);
        let mut o = inicio as usize;
        for _ in 0..por_trozo {
            if sitio.len() == n {
                break;
            }
            let s = tam(sitio.len()) as usize;
            sitio.push((o, s));
            o += s;
        }
    }
    let duraciones = corridas(d, tabla(b"stts"), n);
    let desfases = corridas(d, tabla(b"ctts"), n);
    let claves: Option<Vec<u32>> = tabla(b"stss")
        .map(|c| (0..u32_en(d, c.cuerpo + 4) as usize).map(|i| u32_en(d, c.cuerpo + 8 + 4 * i)).collect());
    let mut dts = 0u64;
    let mut v = Vec::with_capacity(sitio.len());
    for (i, &(o, s)) in sitio.iter().enumerate() {
        let datos = d.get(o..o + s).ok_or("una muestra se sale del archivo").ok()?.to_vec();
        let clave = claves.as_ref().is_none_or(|c| c.contains(&(i as u32 + 1)));
        let pts = dts as i64 + i64::from(desfases[i] as i32);
        v.push(Muestra { pista: id, dts, pts, duracion: duraciones[i], clave, datos });
        dts += u64::from(duraciones[i]);
    }
    Some(v)
}

/// Una tabla de corridas (`stts`, `ctts`: cuántas muestras, valor) desplegada en `n` valores.
fn corridas(d: &[u8], c: Option<Caja>, n: usize) -> Vec<u32> {
    let mut v = Vec::with_capacity(n);
    if let Some(c) = c {
        for i in 0..u32_en(d, c.cuerpo + 4) as usize {
            let (k, valor) = (u32_en(d, c.cuerpo + 8 + 8 * i), u32_en(d, c.cuerpo + 12 + 8 * i));
            v.extend(core::iter::repeat_n(valor, (k as usize).min(n - v.len())));
        }
    }
    v.resize(n, 0);
    v
}

/// Las muestras de un fragmento: `d[moof.inicio..]` contiene el `moof` y su `mdat` detrás.
fn muestras_de_fragmento(d: &[u8], moof: Caja, pistas: &[Pista]) -> Vec<Muestra> {
    let mut v = Vec::new();
    for traf in cajas(d, moof.cuerpo, moof.fin).into_iter().filter(|c| &c.tipo == b"traf") {
        let Some(tfhd) = hija(d, traf, b"tfhd") else { continue };
        let banderas = u32_en(d, tfhd.cuerpo) & 0x00ff_ffff;
        let id = u32_en(d, tfhd.cuerpo + 4);
        let pista = pistas.iter().find(|p| p.id == id);
        let (mut dur, mut tam, mut band) = pista.map_or((0, 0, 0), |p| p.defecto);
        let mut i = tfhd.cuerpo + 8;
        let mut base = moof.inicio;
        if banderas & 0x1 != 0 {
            base = u64_en(d, i) as usize;
            i += 8;
        }
        for (bit, campo) in [(0x2, None), (0x8, Some(0)), (0x10, Some(1)), (0x20, Some(2))] {
            if banderas & bit != 0 {
                match campo {
                    Some(0) => dur = u32_en(d, i),
                    Some(1) => tam = u32_en(d, i),
                    Some(_) => band = u32_en(d, i),
                    None => {}
                }
                i += 4;
            }
        }
        let mut dts = hija(d, traf, b"tfdt")
            .map_or(0, |t| if d[t.cuerpo] == 1 { u64_en(d, t.cuerpo + 4) } else { u64::from(u32_en(d, t.cuerpo + 4)) });
        let mut sitio = base;
        for trun in cajas(d, traf.cuerpo, traf.fin).into_iter().filter(|c| &c.tipo == b"trun") {
            let b = u32_en(d, trun.cuerpo);
            let (version, fl) = (b >> 24, b & 0x00ff_ffff);
            let n = u32_en(d, trun.cuerpo + 4);
            let mut j = trun.cuerpo + 8;
            if fl & 0x1 != 0 {
                sitio = (base as i64 + i64::from(u32_en(d, j) as i32)) as usize;
                j += 4;
            }
            let mut primera = None;
            if fl & 0x4 != 0 {
                primera = Some(u32_en(d, j));
                j += 4;
            }
            for k in 0..n {
                let mut leer = |bit: u32, defecto: u32| {
                    if fl & bit != 0 {
                        let x = u32_en(d, j);
                        j += 4;
                        x
                    } else {
                        defecto
                    }
                };
                let duracion = leer(0x100, dur);
                let s = leer(0x200, tam) as usize;
                let f = leer(0x400, if k == 0 { primera.unwrap_or(band) } else { band });
                let cto = leer(0x800, 0);
                let desfase = if version == 0 { i64::from(cto) } else { i64::from(cto as i32) };
                let Some(datos) = d.get(sitio..sitio + s) else { break };
                // Fotograma clave: ni «depende de otros» ni «no es de sincronía».
                let clave = (f >> 24) & 3 != 1 && (f >> 16) & 1 == 0;
                v.push(Muestra { pista: id, dts, pts: dts as i64 + desfase, duracion, clave, datos: datos.to_vec() });
                dts += u64::from(duracion);
                sitio += s;
            }
        }
    }
    v
}

/// Las pistas y todas las muestras de un MP4 entero (normal o fragmentado).
pub fn leer_archivo(d: &[u8]) -> Result<(Vec<Pista>, Vec<Muestra>), Error> {
    let arriba = cajas(d, 0, d.len());
    let moov = arriba.iter().copied().find(|c| &c.tipo == b"moov").ok_or("no hay moov")?;
    let pistas = leer_moov(d, moov);
    let mut muestras = Vec::new();
    for trak in cajas(d, moov.cuerpo, moov.fin).into_iter().filter(|c| &c.tipo == b"trak") {
        let Some(id) = hija(d, trak, b"tkhd").map(|t| u32_en(d, t.cuerpo + if d[t.cuerpo] == 1 { 20 } else { 12 }))
        else {
            continue;
        };
        muestras.extend(muestras_de_tabla(d, trak, id).unwrap_or_default());
    }
    for moof in arriba.iter().filter(|c| &c.tipo == b"moof") {
        muestras.extend(muestras_de_fragmento(d, *moof, &pistas));
    }
    Ok((pistas, muestras))
}

/// Lee un MP4 fragmentado que llega a trozos (lo que se mete en un `SourceBuffer`).
#[derive(Debug, Default)]
pub struct Demuxer {
    /// Las pistas del último segmento de inicio.
    pub pistas: Vec<Pista>,
    pendiente: Vec<u8>,
}

impl Demuxer {
    /// Un lector vacío.
    pub fn nuevo() -> Demuxer {
        Demuxer::default()
    }

    /// Añade bytes y devuelve las muestras de los fragmentos que ya están completos (un `moof`
    /// con su `mdat`). Lo que queda a medias espera al siguiente trozo.
    pub fn empujar(&mut self, bytes: &[u8]) -> Vec<Muestra> {
        self.pendiente.extend_from_slice(bytes);
        let d = &self.pendiente;
        let mut v = Vec::new();
        let mut usado = 0;
        let lista = cajas(d, 0, d.len());
        let mut k = 0;
        while k < lista.len() {
            let c = lista[k];
            match &c.tipo {
                b"moov" => self.pistas = leer_moov(d, c),
                b"moof" => {
                    // Hace falta su `mdat`, que va detrás.
                    let Some(m) = lista[k + 1..].iter().position(|x| &x.tipo == b"mdat") else { break };
                    v.extend(muestras_de_fragmento(d, c, &self.pistas));
                    k += m + 1;
                }
                _ => {}
            }
            usado = lista[k].fin;
            k += 1;
        }
        self.pendiente.drain(..usado);
        v
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn caja(tipo: &[u8; 4], cuerpo: &[u8]) -> Vec<u8> {
        let mut v = ((cuerpo.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(tipo);
        v.extend_from_slice(cuerpo);
        v
    }

    fn cat(partes: &[Vec<u8>]) -> Vec<u8> {
        partes.concat()
    }

    /// Un segmento de inicio con una pista H.264 (id 1, 90 000 por segundo, 640 × 360).
    fn inicio() -> Vec<u8> {
        let mut visual = alloc::vec![0u8; 78];
        visual[24..26].copy_from_slice(&640u16.to_be_bytes());
        visual[26..28].copy_from_slice(&360u16.to_be_bytes());
        visual.extend(caja(b"avcC", &[1, 0x64, 0, 0x1f]));
        let stsd = cat(&[alloc::vec![0, 0, 0, 0, 0, 0, 0, 1], caja(b"avc1", &visual)]);
        let stbl = caja(b"stbl", &caja(b"stsd", &stsd));
        let mut mdhd = alloc::vec![0u8; 20];
        mdhd[12..16].copy_from_slice(&90_000u32.to_be_bytes());
        let mdia = caja(b"mdia", &cat(&[caja(b"mdhd", &mdhd), caja(b"minf", &stbl)]));
        let mut tkhd = alloc::vec![0u8; 20];
        tkhd[12..16].copy_from_slice(&1u32.to_be_bytes());
        let trak = caja(b"trak", &cat(&[caja(b"tkhd", &tkhd), mdia]));
        let trex = caja(b"trex", &[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0x0b, 0xb8, 0, 0, 0, 0, 0, 1, 0, 0]);
        cat(&[caja(b"ftyp", b"iso5"), caja(b"moov", &cat(&[trak, caja(b"mvex", &trex)]))])
    }

    /// Un fragmento con dos muestras: una clave de 3 bytes y otra de 2.
    fn fragmento(tiempo: u32) -> Vec<u8> {
        let tfhd = caja(b"tfhd", &[0, 0x02, 0, 0, 0, 0, 0, 1]);
        let mut tfdt = alloc::vec![0, 0, 0, 0];
        tfdt.extend_from_slice(&tiempo.to_be_bytes());
        // trun: data_offset y tamaño por muestra; la primera con sus banderas propias (clave).
        let mut trun = alloc::vec![0, 0, 0x02, 0x05, 0, 0, 0, 2, 0, 0, 0, 0, 0x02, 0, 0, 0];
        trun.extend_from_slice(&3u32.to_be_bytes());
        trun.extend_from_slice(&2u32.to_be_bytes());
        let traf = caja(b"traf", &cat(&[tfhd, caja(b"tfdt", &tfdt), caja(b"trun", &trun)]));
        let mut moof = caja(b"moof", &cat(&[caja(b"mfhd", &[0; 8]), traf]));
        let desfase = (moof.len() + 8) as u32;
        let i = moof.windows(4).position(|w| w == b"trun").unwrap_or(0) + 4 + 8;
        moof[i..i + 4].copy_from_slice(&desfase.to_be_bytes());
        cat(&[moof, caja(b"mdat", &[1, 2, 3, 4, 5])])
    }

    #[test]
    fn segmento_de_inicio_y_fragmentos_a_trozos() {
        let mut d = Demuxer::nuevo();
        assert!(d.empujar(&inicio()).is_empty());
        assert_eq!(d.pistas.len(), 1);
        assert_eq!(d.pistas[0].escala, 90_000);
        assert!(matches!(&d.pistas[0].codec, Codec::Avc { ancho: 640, alto: 360, avcc } if avcc[0] == 1));
        let f = fragmento(9000);
        // A medias no sale nada; con el resto, las dos muestras.
        assert!(d.empujar(&f[..20]).is_empty());
        let m = d.empujar(&f[20..]);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].dts, m[0].duracion, m[0].clave, m[0].datos.as_slice()), (9000, 3000, true, &[1u8, 2, 3][..]));
        assert_eq!((m[1].dts, m[1].clave, m[1].datos.as_slice()), (12_000, false, &[4u8, 5][..]));
        assert_eq!(d.empujar(&fragmento(15_000))[0].dts, 15_000);
    }
}
