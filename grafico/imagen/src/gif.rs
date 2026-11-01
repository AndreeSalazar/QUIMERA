//! GIF (87a y 89a): el primer fotograma, con su tabla de colores global o local, el color
//! transparente de la extensión de control gráfico, entrelazado y descompresión LZW.
//! (Las animaciones se verán quietas en su primer fotograma por ahora.)

use crate::{Error, Imagen};
use alloc::vec::Vec;

/// Decodifica el primer fotograma de un GIF sobre un lienzo transparente del tamaño lógico.
pub fn decodificar(d: &[u8]) -> Result<Imagen, Error> {
    if !(d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a")) || d.len() < 13 {
        return Err(Error::Datos("no es GIF"));
    }
    let ancho = u32::from(u16::from_le_bytes([d[6], d[7]]));
    let alto = u32::from(u16::from_le_bytes([d[8], d[9]]));
    crate::comprobar_tamano(ancho.max(1), alto.max(1))?;
    let campos = d[10];
    let mut i = 13;
    let mut global: &[u8] = &[];
    if campos & 0x80 != 0 {
        let tam = 3 << ((campos & 7) + 1);
        global = d.get(i..i + tam).ok_or(Error::Datos("tabla global cortada"))?;
        i += tam;
    }
    let mut transparente: Option<u8> = None;
    loop {
        match d.get(i) {
            Some(0x21) => {
                // Extensión: la de control gráfico (0xF9) trae el color transparente.
                let etiqueta = *d.get(i + 1).ok_or(Error::Datos("extensión cortada"))?;
                if etiqueta == 0xF9 && d.get(i + 2) == Some(&4) {
                    let banderas = d[i + 3];
                    transparente = (banderas & 1 != 0).then(|| d[i + 6]);
                }
                i = saltar_subbloques(d, i + 2)?;
            }
            Some(0x2C) => return fotograma(d, i, global, transparente, (ancho, alto)),
            _ => return Err(Error::Datos("GIF sin imágenes")),
        }
    }
}

/// Salta una secuencia de subbloques (tamaño, datos...) terminada en 0; devuelve dónde sigue.
fn saltar_subbloques(d: &[u8], mut i: usize) -> Result<usize, Error> {
    loop {
        let n = *d.get(i).ok_or(Error::Datos("subbloque cortado"))? as usize;
        i += 1 + n;
        if n == 0 {
            return Ok(i);
        }
    }
}

fn fotograma(
    d: &[u8],
    mut i: usize,
    global: &[u8],
    transparente: Option<u8>,
    (ancho, alto): (u32, u32),
) -> Result<Imagen, Error> {
    let cab = d.get(i..i + 10).ok_or(Error::Datos("descriptor cortado"))?;
    let le = |k: usize| u32::from(u16::from_le_bytes([cab[k], cab[k + 1]]));
    let (x0, y0, w, h) = (le(1), le(3), le(5), le(7));
    let campos = cab[9];
    i += 10;
    let mut tabla = global;
    if campos & 0x80 != 0 {
        let tam = 3 << ((campos & 7) + 1);
        tabla = d.get(i..i + tam).ok_or(Error::Datos("tabla local cortada"))?;
        i += tam;
    }
    let minimo = *d.get(i).ok_or(Error::Datos("LZW sin tamaño"))?;
    let mut datos = Vec::new();
    let mut j = i + 1;
    loop {
        let n = *d.get(j).ok_or(Error::Datos("datos LZW cortados"))? as usize;
        if n == 0 {
            break;
        }
        datos.extend_from_slice(d.get(j + 1..j + 1 + n).ok_or(Error::Datos("datos LZW cortados"))?);
        j += 1 + n;
    }
    let indices = lzw(&datos, minimo, (w * h) as usize)?;
    let mut img = Imagen::vacia(ancho.max(1), alto.max(1));
    let filas = orden_filas(h, campos & 0x40 != 0);
    for (k, &fila) in filas.iter().enumerate() {
        for x in 0..w {
            let Some(&ind) = indices.get(k * w as usize + x as usize) else { break };
            let (px, py) = (x0 + x, y0 + fila);
            if Some(ind) == transparente || px >= img.ancho || py >= img.alto {
                continue;
            }
            let c = tabla.get(ind as usize * 3..ind as usize * 3 + 3).unwrap_or(&[0, 0, 0]);
            let p = (py as usize * img.ancho as usize + px as usize) * 4;
            img.rgba[p..p + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Ok(img)
}

/// En qué fila va cada línea decodificada (entrelazado: pasadas de 8, 8, 4 y 2).
fn orden_filas(h: u32, entrelazado: bool) -> Vec<u32> {
    if !entrelazado {
        return (0..h).collect();
    }
    let mut v = Vec::with_capacity(h as usize);
    for (inicio, paso) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
        v.extend((inicio..h).step_by(paso));
    }
    v
}

/// LZW de GIF: códigos de 3 a 12 bits (bit menos significativo primero), con código de
/// limpieza y de fin.
fn lzw(datos: &[u8], minimo: u8, esperados: usize) -> Result<Vec<u8>, Error> {
    if !(2..=11).contains(&minimo) {
        return Err(Error::Datos("tamaño LZW no válido"));
    }
    let limpiar = 1u16 << minimo;
    let fin = limpiar + 1;
    // Cada código: (código previo, último byte, largo).
    let mut prefijo: Vec<u16> = alloc::vec![0; 4096];
    let mut sufijo: Vec<u8> = alloc::vec![0; 4096];
    let mut largo: Vec<u16> = alloc::vec![0; 4096];
    for c in 0..limpiar {
        sufijo[c as usize] = c as u8;
        largo[c as usize] = 1;
    }
    let (mut ancho, mut siguiente, mut previo): (u32, u16, Option<u16>) = (u32::from(minimo) + 1, fin + 1, None);
    let (mut buf, mut nbits, mut i) = (0u32, 0u32, 0usize);
    let mut salida = Vec::with_capacity(esperados);
    let mut tmp = Vec::new();
    while salida.len() < esperados {
        while nbits < ancho {
            let Some(&b) = datos.get(i) else { return Ok(rellenar(salida, esperados)) };
            buf |= u32::from(b) << nbits;
            nbits += 8;
            i += 1;
        }
        let codigo = (buf & ((1 << ancho) - 1)) as u16;
        buf >>= ancho;
        nbits -= ancho;
        if codigo == limpiar {
            (ancho, siguiente, previo) = (u32::from(minimo) + 1, fin + 1, None);
            continue;
        }
        if codigo == fin {
            break;
        }
        // El código nuevo (aún no en la tabla) es el previo más su primer byte.
        let conocido = codigo < siguiente;
        let base = if conocido { codigo } else { previo.ok_or(Error::Datos("código LZW no válido"))? };
        tmp.clear();
        let mut c = base;
        loop {
            tmp.push(sufijo[c as usize]);
            if largo[c as usize] <= 1 {
                break;
            }
            c = prefijo[c as usize];
        }
        tmp.reverse();
        let primero = tmp[0];
        salida.extend_from_slice(&tmp);
        if !conocido {
            salida.push(primero);
        }
        if let Some(p) = previo
            && siguiente < 4096
        {
            prefijo[siguiente as usize] = p;
            sufijo[siguiente as usize] = primero;
            largo[siguiente as usize] = largo[p as usize] + 1;
            siguiente += 1;
            if u32::from(siguiente) == 1 << ancho && ancho < 12 {
                ancho += 1;
            }
        }
        previo = Some(codigo);
    }
    Ok(rellenar(salida, esperados))
}

fn rellenar(mut v: Vec<u8>, n: usize) -> Vec<u8> {
    v.resize(n, 0);
    v
}
