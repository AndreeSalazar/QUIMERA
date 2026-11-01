//! Descompresión de QUIMERA, sin dependencias: **DEFLATE** (RFC 1951) y **zlib** (RFC 1950).
//!
//! La usan las imágenes PNG, las fuentes WOFF y, más adelante, las respuestas `gzip` de la red.
//! Está en el nivel 0 para que cualquier crate de arriba pueda usarla.
#![no_std]

extern crate alloc;

mod inflar;

pub use inflar::{inflar, zlib};

/// Por qué no se pudo descomprimir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Los datos están rotos o cortados.
    Datos(&'static str),
    /// Una variante que todavía no está (zlib con diccionario).
    NoSoportado(&'static str),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Datos(m) => write!(f, "datos comprimidos no válidos: {m}"),
            Error::NoSoportado(m) => write!(f, "compresión no soportada: {m}"),
        }
    }
}
