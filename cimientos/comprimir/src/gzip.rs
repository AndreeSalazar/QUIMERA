//! **gzip** (RFC 1952): una cabecera, datos DEFLATE y un pie con su CRC-32 y su tamaño. Lo usan
//! las respuestas HTTP con `Content-Encoding: gzip`.

use crate::{Error, inflar};
use alloc::vec::Vec;

const FHCRC: u8 = 2;
const FEXTRA: u8 = 4;
const FNAME: u8 = 8;
const FCOMMENT: u8 = 16;

/// Descomprime un archivo gzip (uno o varios miembros seguidos) y comprueba el CRC-32 de cada uno.
pub fn gzip(datos: &[u8]) -> Result<Vec<u8>, Error> {
    let mut salida = Vec::with_capacity(datos.len() * 4);
    let mut resto = datos;
    loop {
        let inicio = salida.len();
        let usados = miembro(resto, &mut salida)?;
        let pie = resto.get(usados..usados + 8).ok_or(Error::Datos("gzip sin su pie"))?;
        let crc = u32::from_le_bytes([pie[0], pie[1], pie[2], pie[3]]);
        let largo = u32::from_le_bytes([pie[4], pie[5], pie[6], pie[7]]);
        let hecho = &salida[inicio..];
        if crc32(hecho) != crc || hecho.len() as u32 != largo {
            return Err(Error::Datos("gzip con el CRC o el tamaño equivocados"));
        }
        resto = &resto[usados + 8..];
        // Otro miembro detrás (algunos servidores concatenan), o relleno de ceros que se ignora.
        if resto.len() < 18 || resto[..2] != [0x1f, 0x8b] {
            return Ok(salida);
        }
    }
}

/// Un miembro: lee su cabecera y sus datos DEFLATE. Devuelve los bytes que ocupan (sin el pie).
fn miembro(d: &[u8], salida: &mut Vec<u8>) -> Result<usize, Error> {
    if d.len() < 18 || d[0] != 0x1f || d[1] != 0x8b {
        return Err(Error::Datos("no es gzip"));
    }
    if d[2] != 8 {
        return Err(Error::NoSoportado("gzip con un método distinto de DEFLATE"));
    }
    let banderas = d[3];
    let mut i = 10;
    if banderas & FEXTRA != 0 {
        let n = usize::from(u16::from_le_bytes([*d.get(i).unwrap_or(&0), *d.get(i + 1).unwrap_or(&0)]));
        i += 2 + n;
    }
    for bandera in [FNAME, FCOMMENT] {
        if banderas & bandera != 0 {
            let fin = d.get(i..).and_then(|r| r.iter().position(|&b| b == 0)).ok_or(Error::Datos("gzip cortado"))?;
            i += fin + 1;
        }
    }
    if banderas & FHCRC != 0 {
        i += 2;
    }
    let cuerpo = d.get(i..).ok_or(Error::Datos("gzip cortado"))?;
    let usados = inflar::inflar_contando(cuerpo, salida)?;
    Ok(i + usados)
}

/// CRC-32 (el de gzip y PNG), bit a bit con una tabla de 16 entradas.
fn crc32(datos: &[u8]) -> u32 {
    const TABLA: [u32; 16] = [
        0x0000_0000,
        0x1db7_1064,
        0x3b6e_20c8,
        0x26d9_30ac,
        0x76dc_4190,
        0x6b6b_51f4,
        0x4db2_6158,
        0x5005_713c,
        0xedb8_8320,
        0xf00f_9344,
        0xd6d6_a3e8,
        0xcb61_b38c,
        0x9b64_c2b0,
        0x86d3_d2d4,
        0xa00a_e278,
        0xbdbd_f21c,
    ];
    let mut c = !0u32;
    for &b in datos {
        c = TABLA[((c ^ u32::from(b)) & 15) as usize] ^ (c >> 4);
        c = TABLA[((c ^ (u32::from(b) >> 4)) & 15) as usize] ^ (c >> 4);
    }
    !c
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// `gzip.GzipFile(filename='hola.txt')` de Python con 20 veces «hola QUIMERA, ñandú. ».
    const HOLA: [u8; 58] = [
        31, 139, 8, 8, 0, 0, 0, 0, 2, 255, 104, 111, 108, 97, 46, 116, 120, 116, 0, 203, 200, 207, 73, 84, 8, 12, 245,
        244, 117, 13, 114, 212, 81, 56, 188, 49, 49, 47, 229, 240, 46, 61, 133, 140, 81, 225, 161, 39, 12, 0, 123, 147,
        68, 108, 204, 1, 0, 0,
    ];

    #[test]
    fn descomprime_y_comprueba() {
        let esperado = "hola QUIMERA, ñandú. ".repeat(20);
        assert_eq!(gzip(&HOLA).as_deref(), Ok(esperado.as_bytes()));
        // Dos miembros seguidos se unen.
        let doble = [HOLA, HOLA].concat();
        assert_eq!(gzip(&doble).map(|v| v.len()), Ok(esperado.len() * 2));
        // Un byte cambiado: el CRC lo nota.
        let mut roto = HOLA;
        roto[30] ^= 1;
        assert!(gzip(&roto).is_err());
        assert!(gzip(b"<html>").is_err());
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
