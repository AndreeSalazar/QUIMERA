//! PNG: trozos (`IHDR`, `PLTE`, `tRNS`, `IDAT`), descompresión zlib, los cinco filtros por línea,
//! entrelazado Adam7, todas las profundidades (1 a 16 bits) y tipos de color, a RGBA de 8 bits.

use crate::{Error, Imagen};
use alloc::vec::Vec;
use quimera_comprimir::zlib;

struct Cabecera {
    ancho: u32,
    alto: u32,
    bits: u8,
    color: u8,
    entrelazado: bool,
}

impl Cabecera {
    /// Muestras por píxel.
    fn canales(&self) -> usize {
        match self.color {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }

    /// Bytes de una línea de `ancho` píxeles (sin el byte del filtro).
    fn bytes_linea(&self, ancho: u32) -> usize {
        (ancho as usize * self.canales() * self.bits as usize).div_ceil(8)
    }

    /// Bytes por píxel para los filtros (al menos 1).
    fn bpp(&self) -> usize {
        (self.canales() * self.bits as usize).div_ceil(8).max(1)
    }
}

fn u32_be(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Decodifica un PNG.
pub fn decodificar(d: &[u8]) -> Result<Imagen, Error> {
    if !d.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::Datos("no es PNG"));
    }
    let (mut cab, mut paleta, mut trns, mut idat) = (None, Vec::new(), Vec::new(), Vec::new());
    let mut i = 8;
    while i + 8 <= d.len() {
        let largo = u32_be(&d[i..]) as usize;
        let tipo = &d[i + 4..i + 8];
        let datos = d.get(i + 8..i + 8 + largo).ok_or(Error::Datos("trozo PNG cortado"))?;
        match tipo {
            b"IHDR" if datos.len() >= 13 => {
                cab = Some(Cabecera {
                    ancho: u32_be(datos),
                    alto: u32_be(&datos[4..]),
                    bits: datos[8],
                    color: datos[9],
                    entrelazado: datos[12] == 1,
                })
            }
            b"PLTE" => paleta = datos.to_vec(),
            b"tRNS" => trns = datos.to_vec(),
            b"IDAT" => idat.extend_from_slice(datos),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + largo;
    }
    let cab = cab.ok_or(Error::Datos("PNG sin IHDR"))?;
    // Como libpng (Chrome, Firefox): un tRNS con más entradas que la paleta no vale y se ignora.
    if cab.color == 3 && trns.len() > paleta.len() / 3 {
        trns.clear();
    }
    let valido = matches!((cab.color, cab.bits), (0, 1 | 2 | 4 | 8 | 16) | (3, 1 | 2 | 4 | 8) | (2 | 4 | 6, 8 | 16));
    if !valido || cab.ancho == 0 || cab.alto == 0 {
        return Err(Error::Datos("cabecera PNG no válida"));
    }
    crate::comprobar_tamano(cab.ancho, cab.alto)?;
    let crudo = zlib(&idat, (cab.bytes_linea(cab.ancho) + 1) * cab.alto as usize)?;
    let mut img = Imagen::vacia(cab.ancho, cab.alto);
    if cab.entrelazado {
        adam7(&cab, &crudo, &paleta, &trns, &mut img)?;
    } else {
        let lineas = desfiltrar(&cab, &crudo, cab.ancho, cab.alto)?;
        volcar(&cab, &lineas, (cab.ancho, cab.alto), &paleta, &trns, &mut img, |x, y| (x, y));
    }
    Ok(img)
}

/// Las siete pasadas de Adam7: (x0, y0, paso x, paso y).
const PASADAS: [(u32, u32, u32, u32); 7] =
    [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];

fn adam7(cab: &Cabecera, crudo: &[u8], paleta: &[u8], trns: &[u8], img: &mut Imagen) -> Result<(), Error> {
    let mut resto = crudo;
    for (x0, y0, dx, dy) in PASADAS {
        let ancho = (cab.ancho.saturating_sub(x0)).div_ceil(dx);
        let alto = (cab.alto.saturating_sub(y0)).div_ceil(dy);
        if ancho == 0 || alto == 0 {
            continue;
        }
        let tam = (cab.bytes_linea(ancho) + 1) * alto as usize;
        let trozo = resto.get(..tam).ok_or(Error::Datos("PNG entrelazado cortado"))?;
        let lineas = desfiltrar(cab, trozo, ancho, alto)?;
        volcar(cab, &lineas, (ancho, alto), paleta, trns, img, |x, y| (x0 + x * dx, y0 + y * dy));
        resto = &resto[tam..];
    }
    Ok(())
}

/// Quita los filtros (ninguno, sub, up, average, Paeth) de cada línea.
fn desfiltrar(cab: &Cabecera, crudo: &[u8], ancho: u32, alto: u32) -> Result<Vec<u8>, Error> {
    let linea = cab.bytes_linea(ancho);
    let bpp = cab.bpp();
    let mut salida = alloc::vec![0u8; linea * alto as usize];
    for y in 0..alto as usize {
        let entrada = crudo.get(y * (linea + 1)..(y + 1) * (linea + 1)).ok_or(Error::Datos("PNG cortado"))?;
        let (filtro, datos) = (entrada[0], &entrada[1..]);
        let (antes, actual) = salida.split_at_mut(y * linea);
        let arriba = if y > 0 { &antes[(y - 1) * linea..] } else { &[][..] };
        let actual = &mut actual[..linea];
        if filtro > 4 {
            return Err(Error::Datos("filtro PNG no válido"));
        }
        filtrar_linea(filtro, &datos[..linea], arriba, actual, bpp);
    }
    Ok(salida)
}

/// Una línea sin su filtro. El tipo de filtro se decide una vez por línea, no por byte.
fn filtrar_linea(filtro: u8, datos: &[u8], arriba: &[u8], actual: &mut [u8], bpp: usize) {
    let n = actual.len();
    let primera = arriba.is_empty();
    match filtro {
        0 => actual.copy_from_slice(datos),
        1 => {
            actual[..bpp.min(n)].copy_from_slice(&datos[..bpp.min(n)]);
            for x in bpp..n {
                actual[x] = datos[x].wrapping_add(actual[x - bpp]);
            }
        }
        2 if primera => actual.copy_from_slice(datos),
        2 => {
            for x in 0..n {
                actual[x] = datos[x].wrapping_add(arriba[x]);
            }
        }
        3 => {
            for x in 0..n {
                let a = if x >= bpp { u16::from(actual[x - bpp]) } else { 0 };
                let b = if primera { 0 } else { u16::from(arriba[x]) };
                actual[x] = datos[x].wrapping_add(((a + b) / 2) as u8);
            }
        }
        _ => {
            for x in 0..n {
                let a = if x >= bpp { actual[x - bpp] } else { 0 };
                let (b, c) = if primera { (0, 0) } else { (arriba[x], if x >= bpp { arriba[x - bpp] } else { 0 }) };
                actual[x] = datos[x].wrapping_add(paeth(a, b, c));
            }
        }
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Pasa las líneas ya sin filtro a RGBA en la imagen (`pos` lleva cada píxel a su sitio).
fn volcar(
    cab: &Cabecera,
    lineas: &[u8],
    (ancho, alto): (u32, u32),
    paleta: &[u8],
    trns: &[u8],
    img: &mut Imagen,
    pos: impl Fn(u32, u32) -> (u32, u32),
) {
    let linea = cab.bytes_linea(ancho);
    let canales = cab.canales();
    // Lo más común (8 bits sin entrelazar): fila a fila, sin pasar por cada muestra.
    if cab.bits == 8 && !cab.entrelazado && (cab.color != 0 && cab.color != 2 || trns.is_empty()) {
        for (fila, salida) in lineas.chunks_exact(linea).zip(img.rgba.chunks_exact_mut(ancho as usize * 4)) {
            rapido(cab.color, fila, salida, paleta, trns);
        }
        return;
    }
    for y in 0..alto {
        let fila = &lineas[y as usize * linea..(y as usize + 1) * linea];
        for x in 0..ancho {
            let muestra = |c: usize| leer_muestra(fila, x as usize * canales + c, cab.bits);
            let rgba = pixel(cab, &muestra, paleta, trns);
            let (px, py) = pos(x, y);
            let i = (py as usize * img.ancho as usize + px as usize) * 4;
            img.rgba[i..i + 4].copy_from_slice(&rgba);
        }
    }
}

/// Una línea de 8 bits a RGBA.
fn rapido(color: u8, fila: &[u8], salida: &mut [u8], paleta: &[u8], trns: &[u8]) {
    match color {
        6 => salida.copy_from_slice(fila),
        2 => {
            for (p, s) in salida.chunks_exact_mut(4).zip(fila.chunks_exact(3)) {
                p.copy_from_slice(&[s[0], s[1], s[2], 255]);
            }
        }
        0 => {
            for (p, &g) in salida.chunks_exact_mut(4).zip(fila) {
                p.copy_from_slice(&[g, g, g, 255]);
            }
        }
        4 => {
            for (p, s) in salida.chunks_exact_mut(4).zip(fila.chunks_exact(2)) {
                p.copy_from_slice(&[s[0], s[0], s[0], s[1]]);
            }
        }
        _ => {
            for (p, &i) in salida.chunks_exact_mut(4).zip(fila) {
                let i = usize::from(i);
                let c = paleta.get(i * 3..i * 3 + 3).unwrap_or(&[0, 0, 0]);
                p.copy_from_slice(&[c[0], c[1], c[2], trns.get(i).copied().unwrap_or(255)]);
            }
        }
    }
}

/// La muestra número `k` de una línea, en su profundidad original (sin escalar).
fn leer_muestra(fila: &[u8], k: usize, bits: u8) -> u16 {
    match bits {
        8 => u16::from(fila[k]),
        16 => u16::from_be_bytes([fila[k * 2], fila[k * 2 + 1]]),
        b => {
            let bit = k * b as usize;
            let byte = fila[bit / 8];
            let desplazar = 8 - b as usize - bit % 8;
            u16::from((byte >> desplazar) & ((1 << b) - 1))
        }
    }
}

fn pixel(cab: &Cabecera, muestra: &dyn Fn(usize) -> u16, paleta: &[u8], trns: &[u8]) -> [u8; 4] {
    let a8 = |v: u16| -> u8 {
        match cab.bits {
            16 => (v >> 8) as u8,
            8 => v as u8,
            b => (u32::from(v) * 255 / ((1 << b) - 1)) as u8,
        }
    };
    // tRNS en gris o color: un valor exacto (en la profundidad original) es transparente.
    let trns16 = |k: usize| trns.get(k * 2..k * 2 + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    match cab.color {
        0 => {
            let g = muestra(0);
            let a = if trns16(0) == Some(g) { 0 } else { 255 };
            [a8(g), a8(g), a8(g), a]
        }
        2 => {
            let (r, g, b) = (muestra(0), muestra(1), muestra(2));
            let transparente = trns16(0) == Some(r) && trns16(1) == Some(g) && trns16(2) == Some(b);
            [a8(r), a8(g), a8(b), if transparente { 0 } else { 255 }]
        }
        3 => {
            let i = muestra(0) as usize;
            let c = paleta.get(i * 3..i * 3 + 3).unwrap_or(&[0, 0, 0]);
            [c[0], c[1], c[2], trns.get(i).copied().unwrap_or(255)]
        }
        4 => {
            let g = a8(muestra(0));
            [g, g, g, a8(muestra(1))]
        }
        _ => [a8(muestra(0)), a8(muestra(1)), a8(muestra(2)), a8(muestra(3))],
    }
}
