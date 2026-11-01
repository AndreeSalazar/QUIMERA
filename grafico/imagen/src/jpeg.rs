//! JPEG: secuencial (baseline y extendido con Huffman) y **progresivo**, con submuestreo de color,
//! intervalos de reinicio, gris, YCbCr, RGB y CMYK/YCCK de Adobe.
//!
//! Los coeficientes de cada bloque se guardan (los escaneos progresivos los van completando) y al
//! final se cuantizan, se pasa la IDCT, se remuestrea el color y se convierte a RGB.

use crate::jpeg_bloques::{
    Bits, Escaneo, Huffman, ZIGZAG, bloque_ac_primero, bloque_ac_refinar, bloque_dc, bloque_secuencial, idct,
    remuestrear,
};
use crate::{Error, Imagen};
use alloc::vec::Vec;

struct Componente {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    /// Bloques de ancho y alto (rellenos hasta completar las MCU).
    bw: usize,
    bh: usize,
    /// Muestras reales de ancho y alto.
    ancho: usize,
    alto: usize,
    bloques: Vec<[i16; 64]>,
    pred: i32,
    td: usize,
    ta: usize,
}

struct Estado {
    cuant: [[u16; 64]; 4],
    dc: [Huffman; 4],
    ac: [Huffman; 4],
    comps: Vec<Componente>,
    ancho: usize,
    alto: usize,
    progresivo: bool,
    reinicio: usize,
    /// Transformación de color de Adobe (APP14), si la hay.
    adobe: Option<u8>,
    hmax: usize,
    vmax: usize,
    mcux: usize,
    mcuy: usize,
}

fn u16_be(d: &[u8], i: usize) -> usize {
    usize::from(u16::from_be_bytes([d[i], d[i + 1]]))
}

/// Decodifica un JPEG.
pub fn decodificar(d: &[u8]) -> Result<Imagen, Error> {
    if !d.starts_with(&[0xFF, 0xD8]) {
        return Err(Error::Datos("no es JPEG"));
    }
    let mut e = Estado {
        cuant: [[1; 64]; 4],
        dc: Default::default(),
        ac: Default::default(),
        comps: Vec::new(),
        ancho: 0,
        alto: 0,
        progresivo: false,
        reinicio: 0,
        adobe: None,
        hmax: 1,
        vmax: 1,
        mcux: 0,
        mcuy: 0,
    };
    let mut i = 2;
    loop {
        // Buscar el siguiente marcador (se toleran bytes de relleno 0xFF).
        while i < d.len() && d[i] != 0xFF {
            i += 1;
        }
        while i < d.len() && d[i] == 0xFF {
            i += 1;
        }
        let Some(&m) = d.get(i) else { break };
        i += 1;
        if m == 0xD9 {
            break;
        }
        if matches!(m, 0x00 | 0x01 | 0xD0..=0xD7) {
            continue;
        }
        let largo = d.get(i..i + 2).map(|_| u16_be(d, i)).ok_or(Error::Datos("marcador JPEG cortado"))?;
        let seg = d.get(i + 2..i + largo).ok_or(Error::Datos("segmento JPEG cortado"))?;
        match m {
            0xDB => cuantizacion(&mut e, seg)?,
            0xC4 => huffman(&mut e, seg)?,
            0xC0..=0xC2 => cuadro(&mut e, seg, m == 0xC2)?,
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                return Err(Error::NoSoportado("JPEG sin pérdida o aritmético"));
            }
            0xDD if seg.len() >= 2 => e.reinicio = u16_be(seg, 0),
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => e.adobe = Some(seg[11]),
            0xDA => {
                i = escaneo(&mut e, seg, d, i + largo)?;
                continue;
            }
            _ => {}
        }
        i += largo;
    }
    componer(&e)
}

fn cuantizacion(e: &mut Estado, mut s: &[u8]) -> Result<(), Error> {
    while let Some(&pq) = s.first() {
        let (precision, t) = (pq >> 4, usize::from(pq & 3));
        let tam = if precision == 0 { 64 } else { 128 };
        let datos = s.get(1..1 + tam).ok_or(Error::Datos("tabla de cuantización cortada"))?;
        for k in 0..64 {
            let v =
                if precision == 0 { u16::from(datos[k]) } else { u16::from_be_bytes([datos[k * 2], datos[k * 2 + 1]]) };
            e.cuant[t][ZIGZAG[k]] = v;
        }
        s = &s[1 + tam..];
    }
    Ok(())
}

fn huffman(e: &mut Estado, mut s: &[u8]) -> Result<(), Error> {
    while s.len() >= 17 {
        let (clase, t) = (s[0] >> 4, usize::from(s[0] & 3));
        let cuentas: [u8; 16] = core::array::from_fn(|k| s[1 + k]);
        let total: usize = cuentas.iter().map(|&c| usize::from(c)).sum();
        let valores = s.get(17..17 + total).ok_or(Error::Datos("tabla de Huffman cortada"))?;
        let h = Huffman::nuevo(&cuentas, valores);
        if clase == 0 {
            e.dc[t] = h
        } else {
            e.ac[t] = h
        }
        s = &s[17 + total..];
    }
    Ok(())
}

fn cuadro(e: &mut Estado, s: &[u8], progresivo: bool) -> Result<(), Error> {
    if s.len() < 6 || s[0] != 8 {
        return Err(Error::NoSoportado("JPEG de 12 bits"));
    }
    let (alto, ancho, n) = (u16_be(s, 1), u16_be(s, 3), usize::from(s[5]));
    if alto == 0 || ancho == 0 || !matches!(n, 1 | 3 | 4) || s.len() < 6 + n * 3 {
        return Err(Error::Datos("cuadro JPEG no válido"));
    }
    crate::comprobar_tamano(ancho as u32, alto as u32)?;
    (e.ancho, e.alto, e.progresivo) = (ancho, alto, progresivo);
    let leidos: Vec<(u8, usize, usize, usize)> = (0..n)
        .map(|k| {
            (
                s[6 + k * 3],
                usize::from(s[7 + k * 3] >> 4).max(1),
                usize::from(s[7 + k * 3] & 15).max(1),
                usize::from(s[8 + k * 3] & 3),
            )
        })
        .collect();
    e.hmax = leidos.iter().map(|c| c.1).max().unwrap_or(1);
    e.vmax = leidos.iter().map(|c| c.2).max().unwrap_or(1);
    e.mcux = ancho.div_ceil(8 * e.hmax);
    e.mcuy = alto.div_ceil(8 * e.vmax);
    e.comps = leidos
        .into_iter()
        .map(|(id, h, v, tq)| {
            let (bw, bh) = (e.mcux * h, e.mcuy * v);
            Componente {
                id,
                h,
                v,
                tq,
                bw,
                bh,
                ancho: (ancho * h).div_ceil(e.hmax),
                alto: (alto * v).div_ceil(e.vmax),
                bloques: alloc::vec![[0i16; 64]; bw * bh],
                pred: 0,
                td: 0,
                ta: 0,
            }
        })
        .collect();
    Ok(())
}

/// Un escaneo: cabecera (`seg`) y datos desde `inicio`. Devuelve dónde siguen los marcadores.
fn escaneo(e: &mut Estado, seg: &[u8], d: &[u8], inicio: usize) -> Result<usize, Error> {
    let ns = usize::from(*seg.first().ok_or(Error::Datos("escaneo vacío"))?);
    let mut en_escaneo = Vec::with_capacity(ns);
    for k in 0..ns {
        let (id, tablas) = (seg.get(1 + k * 2).copied(), seg.get(2 + k * 2).copied());
        let c = e.comps.iter().position(|c| Some(c.id) == id).ok_or(Error::Datos("componente desconocido"))?;
        let t = tablas.unwrap_or(0);
        e.comps[c].td = usize::from(t >> 4) & 3;
        e.comps[c].ta = usize::from(t & 15) & 3;
        en_escaneo.push(c);
    }
    let p = 1 + ns * 2;
    let (ss, se, a) =
        (seg.get(p).copied().unwrap_or(0), seg.get(p + 1).copied().unwrap_or(63), seg.get(p + 2).copied().unwrap_or(0));
    let mut esc = Escaneo { inicio: usize::from(ss), fin: usize::from(se).min(63), alto: a >> 4, bajo: a & 15, eob: 0 };
    if !e.progresivo {
        (esc.inicio, esc.fin) = (0, 63);
    }
    for c in &mut e.comps {
        c.pred = 0;
    }
    let mut b = Bits::nuevo(d, inicio);
    let unidades: Vec<(usize, usize, usize)> = unidades(e, &en_escaneo);
    let por_mcu = if en_escaneo.len() == 1 { 1 } else { en_escaneo.iter().map(|&c| e.comps[c].h * e.comps[c].v).sum() };
    for (n, &(c, bx, by)) in unidades.iter().enumerate() {
        let mcu = n / por_mcu;
        if e.reinicio > 0 && n % por_mcu == 0 && mcu > 0 && mcu % e.reinicio == 0 {
            b.reiniciar();
            esc.eob = 0;
            for comp in &mut e.comps {
                comp.pred = 0;
            }
        }
        bloque(e, &mut b, &mut esc, c, by * e.comps[c].bw + bx)?;
    }
    Ok(b.i)
}

/// Los bloques de un escaneo en orden: (componente, x, y). Con un componente, recorre solo sus
/// bloques reales; con varios, MCU a MCU.
fn unidades(e: &Estado, en_escaneo: &[usize]) -> Vec<(usize, usize, usize)> {
    let mut v = Vec::new();
    if let [c] = *en_escaneo {
        let comp = &e.comps[c];
        for by in 0..comp.alto.div_ceil(8) {
            for bx in 0..comp.ancho.div_ceil(8) {
                v.push((c, bx, by));
            }
        }
        return v;
    }
    for my in 0..e.mcuy {
        for mx in 0..e.mcux {
            for &c in en_escaneo {
                let comp = &e.comps[c];
                for y in 0..comp.v {
                    for x in 0..comp.h {
                        v.push((c, mx * comp.h + x, my * comp.v + y));
                    }
                }
            }
        }
    }
    v
}

fn bloque(e: &mut Estado, b: &mut Bits<'_>, esc: &mut Escaneo, c: usize, k: usize) -> Result<(), Error> {
    let (td, ta, progresivo) = (e.comps[c].td, e.comps[c].ta, e.progresivo);
    let (dc, ac) = (&e.dc[td], &e.ac[ta]);
    let comp = &mut e.comps[c];
    let Some(bloque) = comp.bloques.get_mut(k) else { return Ok(()) };
    if !progresivo {
        return bloque_secuencial(b, bloque, dc, ac, &mut comp.pred);
    }
    if esc.inicio == 0 {
        return bloque_dc(b, bloque, dc, &mut comp.pred, esc);
    }
    if esc.alto == 0 { bloque_ac_primero(b, bloque, ac, esc) } else { bloque_ac_refinar(b, bloque, ac, esc) }
}

/// IDCT de todos los bloques, remuestreo y conversión a RGBA.
fn componer(e: &Estado) -> Result<Imagen, Error> {
    if e.comps.is_empty() {
        return Err(Error::Datos("JPEG sin cuadro"));
    }
    let planos: Vec<Vec<u8>> = e
        .comps
        .iter()
        .map(|c| {
            let paso = c.bw * 8;
            let mut plano = alloc::vec![0u8; paso * c.bh * 8];
            for (k, bl) in c.bloques.iter().enumerate() {
                let (bx, by) = (k % c.bw, k / c.bw);
                idct(bl, &e.cuant[c.tq], &mut plano[by * 8 * paso + bx * 8..], paso);
            }
            let (fx, fy) = (e.hmax / c.h, e.vmax / c.v);
            if fx == 1 && fy == 1 {
                plano
            } else {
                // Tras remuestrear, el plano mide (ancho real · fx) de paso.
                remuestrear(&plano, (c.ancho, c.alto), paso, (fx, fy))
            }
        })
        .collect();
    let pasos: Vec<usize> = (0..e.comps.len())
        .map(|k| {
            let c = &e.comps[k];
            if e.hmax / c.h == 1 && e.vmax / c.v == 1 { c.bw * 8 } else { c.ancho * (e.hmax / c.h) }
        })
        .collect();
    let mut img = Imagen::vacia(e.ancho as u32, e.alto as u32);
    let rgb = e.adobe == Some(0) && e.comps.len() == 3 || e.comps.iter().map(|c| c.id).eq(*b"RGB");
    let w = e.ancho;
    for (y, salida) in img.rgba.chunks_exact_mut(w * 4).enumerate() {
        let fila = |k: usize| &planos[k][y * pasos[k]..y * pasos[k] + w];
        match e.comps.len() {
            1 => {
                for (p, &g) in salida.chunks_exact_mut(4).zip(fila(0)) {
                    p.copy_from_slice(&[g, g, g, 255]);
                }
            }
            3 => {
                for (x, p) in salida.chunks_exact_mut(4).enumerate() {
                    let (a, b, c) = (fila(0)[x], fila(1)[x], fila(2)[x]);
                    let [r, g, bl] = if rgb { [a, b, c] } else { ycbcr(a, b, c) };
                    p.copy_from_slice(&[r, g, bl, 255]);
                }
            }
            _ => {
                for (x, p) in salida.chunks_exact_mut(4).enumerate() {
                    let [r, g, b] = cmyk(e.adobe, [fila(0)[x], fila(1)[x], fila(2)[x], fila(3)[x]]);
                    p.copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
    Ok(img)
}

/// YCbCr (BT.601, rango completo como JFIF) a RGB, en coma fija de 20 bits (como stb_image y
/// jpeg-decoder: los mismos números).
#[inline]
fn ycbcr(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    const UNO: i32 = 1 << 20;
    const R_CR: i32 = (1.402 * UNO as f32 + 0.5) as i32;
    const G_CB: i32 = (0.34414 * UNO as f32 + 0.5) as i32;
    const G_CR: i32 = (0.71414 * UNO as f32 + 0.5) as i32;
    const B_CB: i32 = (1.772 * UNO as f32 + 0.5) as i32;
    let (y, cb, cr) = (i32::from(y) * UNO + UNO / 2, i32::from(cb) - 128, i32::from(cr) - 128);
    let a = |v: i32| (v >> 20).clamp(0, 255) as u8;
    [a(y + R_CR * cr), a(y - G_CB * cb - G_CR * cr), a(y + B_CB * cb)]
}

/// CMYK de Adobe (guardado invertido) o YCCK, a RGB.
fn cmyk(adobe: Option<u8>, [a, b, c, k]: [u8; 4]) -> [u8; 3] {
    let [c, m, y] = if adobe == Some(2) { ycbcr(a, b, c) } else { [a, b, c] };
    let mezclar = |v: u8| (u16::from(v) * u16::from(k) / 255) as u8;
    [mezclar(c), mezclar(m), mezclar(y)]
}
