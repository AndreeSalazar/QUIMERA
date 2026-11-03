//! Decodificadores de imágenes de QUIMERA, propios, `no_std` y sin dependencias.
//!
//! - **PNG** (`png.rs`), con DEFLATE/zlib propio (`quimera-comprimir`): todas las profundidades y
//!   tipos de color, paleta, transparencia (`tRNS`) y entrelazado Adam7.
//! - **JPEG** (`jpeg.rs`, `jpeg_bloques.rs`): secuencial y progresivo, submuestreo, reinicios,
//!   gris, YCbCr, RGB y CMYK de Adobe; la misma IDCT entera que stb_image.
//! - **GIF** (`gif.rs`): el primer fotograma, con transparencia y entrelazado.
//!
//! [`decodificar`] mira los primeros bytes para saber el formato (no la extensión ni el tipo
//! que diga el servidor, que a menudo mienten).
#![no_std]

extern crate alloc;

mod gif;
mod jpeg;
mod jpeg_bloques;
mod png;

use alloc::vec::Vec;

pub use quimera_comprimir::{inflar, zlib};

/// Algo no se pudo decodificar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Los datos están mal o cortados.
    Datos(&'static str),
    /// Una variante del formato que todavía no se sabe leer.
    NoSoportado(&'static str),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Datos(m) => write!(f, "imagen no válida: {m}"),
            Error::NoSoportado(m) => write!(f, "formato no soportado: {m}"),
        }
    }
}

/// Una imagen en RGBA de 8 bits (sin premultiplicar), fila a fila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imagen {
    /// Ancho en píxeles.
    pub ancho: u32,
    /// Alto en píxeles.
    pub alto: u32,
    /// `ancho × alto × 4` bytes.
    pub rgba: Vec<u8>,
}

impl Imagen {
    /// Una imagen transparente.
    pub fn vacia(ancho: u32, alto: u32) -> Imagen {
        Imagen { ancho, alto, rgba: alloc::vec![0; ancho as usize * alto as usize * 4] }
    }
}

/// Un formato reconocido por sus primeros bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formato {
    /// PNG
    Png,
    /// JPEG
    Jpeg,
    /// GIF
    Gif,
    /// WebP (todavía no se decodifica)
    Webp,
    /// SVG (formas y rellenos, con quimera-vector)
    Svg,
}

impl From<quimera_comprimir::Error> for Error {
    fn from(e: quimera_comprimir::Error) -> Self {
        match e {
            quimera_comprimir::Error::Datos(m) => Error::Datos(m),
            quimera_comprimir::Error::NoSoportado(m) => Error::NoSoportado(m),
        }
    }
}

/// El formato por su firma.
pub fn formato(d: &[u8]) -> Option<Formato> {
    if d.starts_with(b"\x89PNG") {
        Some(Formato::Png)
    } else if d.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Formato::Jpeg)
    } else if d.starts_with(b"GIF8") {
        Some(Formato::Gif)
    } else if d.len() >= 12 && &d[..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        Some(Formato::Webp)
    } else if d.iter().take(512).copied().collect::<Vec<u8>>().windows(4).any(|w| w == b"<svg") {
        Some(Formato::Svg)
    } else {
        None
    }
}

/// El tamaño (ancho, alto) de una imagen leído de su cabecera, sin decodificarla. Basta para
/// maquetar: la ventana solo decodifica las imágenes que están cerca de la vista. `None` si el
/// formato no se sabe leer o la cabecera está rota.
pub fn dimensiones(d: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| Some(u32::from(u16::from_be_bytes([*d.get(i)?, *d.get(i + 1)?])));
    let r = match formato(d)? {
        // IHDR: ancho y alto en big-endian tras la firma y la cabecera del trozo.
        Formato::Png => {
            let n = |i: usize| Some(u32::from_be_bytes([*d.get(i)?, *d.get(i + 1)?, *d.get(i + 2)?, *d.get(i + 3)?]));
            (n(16)?, n(20)?)
        }
        // La pantalla lógica: little-endian en los bytes 6 a 9.
        Formato::Gif => (
            u32::from(u16::from_le_bytes([*d.get(6)?, *d.get(7)?])),
            u32::from(u16::from_le_bytes([*d.get(8)?, *d.get(9)?])),
        ),
        // El primer marcador SOF (C0–CF salvo C4, C8 y CC): alto y ancho.
        Formato::Jpeg => {
            let mut i = 2;
            loop {
                while *d.get(i)? != 0xFF {
                    i += 1;
                }
                while *d.get(i)? == 0xFF {
                    i += 1;
                }
                let m = *d.get(i)?;
                if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
                    break (be16(i + 6)?, be16(i + 4)?);
                }
                if (0xD0..=0xD9).contains(&m) || m == 0x01 {
                    i += 1; // marcadores sin longitud
                    continue;
                }
                i += 1 + be16(i + 1)? as usize;
            }
        }
        Formato::Svg => quimera_vector::tam_propio(d)?,
        Formato::Webp => return None,
    };
    (r.0 > 0 && r.1 > 0 && comprobar_tamano(r.0, r.1).is_ok()).then_some(r)
}

/// Decodifica una imagen en cualquiera de los formatos que se saben leer.
pub fn decodificar(d: &[u8]) -> Result<Imagen, Error> {
    match formato(d) {
        Some(Formato::Png) => png::decodificar(d),
        Some(Formato::Jpeg) => jpeg::decodificar(d),
        Some(Formato::Gif) => gif::decodificar(d),
        Some(Formato::Webp) => Err(Error::NoSoportado("WebP")),
        Some(Formato::Svg) => quimera_vector::rasterizar(d)
            .map(|l| Imagen { ancho: l.ancho, alto: l.alto, rgba: l.rgba })
            .ok_or(Error::Datos("SVG ilegible")),
        None => Err(Error::Datos("formato desconocido")),
    }
}

/// Las imágenes muy grandes se rechazan antes de pedir memoria (100 megapíxeles).
pub(crate) fn comprobar_tamano(ancho: u32, alto: u32) -> Result<(), Error> {
    if u64::from(ancho) * u64::from(alto) > 100_000_000 {
        return Err(Error::NoSoportado("imagen de más de 100 megapíxeles"));
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use alloc::vec;

    fn crc(d: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in d {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    fn trozo(v: &mut Vec<u8>, tipo: &[u8], datos: &[u8]) {
        v.extend_from_slice(&(datos.len() as u32).to_be_bytes());
        let inicio = v.len();
        v.extend_from_slice(tipo);
        v.extend_from_slice(datos);
        let c = crc(&v[inicio..]);
        v.extend_from_slice(&c.to_be_bytes());
    }

    /// Un PNG RGBA de 2×1 hecho a mano (zlib con un bloque almacenado).
    fn png_2x1() -> Vec<u8> {
        let crudo = [0u8, 255, 0, 0, 255, 0, 0, 255, 128];
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &crudo {
            a = (a + u32::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        let mut z = vec![0x78, 0x01, 0x01, crudo.len() as u8, 0, !(crudo.len() as u8), 0xFF];
        z.extend_from_slice(&crudo);
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        trozo(&mut v, b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        trozo(&mut v, b"IDAT", &z);
        trozo(&mut v, b"IEND", &[]);
        v
    }

    #[test]
    fn reconoce_los_formatos() {
        assert_eq!(formato(b"\x89PNG\r\n"), Some(Formato::Png));
        assert_eq!(formato(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Formato::Jpeg));
        assert_eq!(formato(b"GIF89a"), Some(Formato::Gif));
        assert_eq!(formato(b"RIFF\0\0\0\0WEBPVP8 "), Some(Formato::Webp));
        assert_eq!(formato(b"<?xml version='1.0'?><svg xmlns="), Some(Formato::Svg));
        assert_eq!(formato(b"hola"), None);
        assert!(matches!(decodificar(b"<svg width='2' height='1'/>"), Ok(Imagen { ancho: 2, alto: 1, .. })));
    }

    #[test]
    fn tamano_sin_decodificar() {
        assert_eq!(dimensiones(&png_2x1()), Some((2, 1)));
        let gif = b"GIF89a\x03\x00\x02\x00\x80\x00\x00";
        assert_eq!(dimensiones(gif), Some((3, 2)));
        // JPEG: SOI, un APP0 que se salta, y SOF0 con alto 480 y ancho 640.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xE0, 0x02, 0x80, 0x03,
        ];
        assert_eq!(dimensiones(&jpeg), Some((640, 480)));
        assert_eq!(dimensiones(b"<svg viewBox='0 0 24 12'/>"), Some((24, 12)));
        assert_eq!(dimensiones(&[0xFF, 0xD8, 0xFF]), None, "cortado");
    }

    #[test]
    fn png_hecho_a_mano() {
        let i = decodificar(&png_2x1()).unwrap();
        assert_eq!((i.ancho, i.alto, i.rgba), (2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]));
    }

    #[test]
    fn gif_de_un_pixel() {
        let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\x00\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
        let i = decodificar(gif).unwrap();
        assert_eq!((i.ancho, i.alto, i.rgba), (1, 1, vec![255, 0, 0, 255]));
    }

    #[test]
    fn datos_rotos_dan_error_sin_romperse() {
        // Cada trozo inicial del PNG, y el PNG con cada byte cambiado: error o imagen, nunca pánico.
        let png = png_2x1();
        for n in 0..png.len() {
            let _ = decodificar(&png[..n]);
            let mut roto = png.clone();
            roto[n] ^= 0x5A;
            let _ = decodificar(&roto);
        }
        assert!(decodificar(&[0xFF, 0xD8, 0xFF, 0xC0, 0, 2]).is_err());
        assert!(decodificar(b"GIF89a\xff\xff").is_err());
    }
}
