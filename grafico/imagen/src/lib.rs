//! Decodificadores de imágenes de QUIMERA, propios, `no_std` y sin dependencias.
//!
//! - **PNG** (`png.rs`), con su propio DEFLATE/zlib (`inflar.rs`): todas las profundidades y
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
mod inflar;
mod jpeg;
mod jpeg_bloques;
mod png;

use alloc::vec::Vec;

pub use inflar::{inflar, zlib};

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
    /// SVG (todavía no se dibuja)
    Svg,
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

/// Decodifica una imagen en cualquiera de los formatos que se saben leer.
pub fn decodificar(d: &[u8]) -> Result<Imagen, Error> {
    match formato(d) {
        Some(Formato::Png) => png::decodificar(d),
        Some(Formato::Jpeg) => jpeg::decodificar(d),
        Some(Formato::Gif) => gif::decodificar(d),
        Some(Formato::Webp) => Err(Error::NoSoportado("WebP")),
        Some(Formato::Svg) => Err(Error::NoSoportado("SVG")),
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
        assert!(matches!(decodificar(b"<svg/>"), Err(Error::NoSoportado(_))));
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
