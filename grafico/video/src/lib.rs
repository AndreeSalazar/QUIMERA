//! Vídeo de QUIMERA: MP4 propio (normal y fragmentado, el de MediaSource), H.264 a Annex B
//! propio, color NV12 → RGBA propio y la decodificación de H.264 con Media Foundation en Windows
//! (el decodificador del sistema, con la GPU cuando puede).

#![no_std]

extern crate alloc;

pub mod color;
pub mod h264;
#[cfg(windows)]
mod mf;
pub mod mp4;

#[cfg(windows)]
pub use mf::DecodificadorH264;

use alloc::string::String;
use alloc::vec::Vec;

/// Lo que puede fallar al decodificar.
pub type Error = String;

/// Un fotograma decodificado.
#[derive(Debug, Clone, PartialEq)]
pub struct Fotograma {
    /// Ancho visible en píxeles.
    pub ancho: u32,
    /// Alto visible en píxeles.
    pub alto: u32,
    /// Píxeles RGBA, fila a fila.
    pub rgba: Vec<u8>,
    /// Cuándo se muestra, en segundos.
    pub tiempo: f64,
}
