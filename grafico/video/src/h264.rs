//! H.264 de MP4 (`avcC` y NAL con su longitud delante) a Annex B (códigos de inicio
//! `00 00 00 01`), que es lo que esperan los decodificadores.

use alloc::vec::Vec;

const INICIO: [u8; 4] = [0, 0, 0, 1];

/// La configuración `avcC`: cuántos bytes mide la longitud de cada NAL, y los SPS y PPS ya en
/// Annex B (van delante de cada fotograma clave).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Bytes de la longitud de cada NAL (1, 2 o 4).
    pub largo_nal: usize,
    /// SPS y PPS con sus códigos de inicio.
    pub cabecera: Vec<u8>,
}

/// Lee una caja `avcC` (ISO/IEC 14496-15).
pub fn config(avcc: &[u8]) -> Option<Config> {
    let largo_nal = usize::from(*avcc.get(4)? & 3) + 1;
    let mut cabecera = Vec::new();
    let mut i = 5;
    // Primero los SPS (5 bits de cuenta) y luego los PPS (8 bits).
    for mascara in [0x1f, 0xff] {
        let n = usize::from(*avcc.get(i)? & mascara);
        i += 1;
        for _ in 0..n {
            let largo = usize::from(u16::from_be_bytes([*avcc.get(i)?, *avcc.get(i + 1)?]));
            cabecera.extend_from_slice(&INICIO);
            cabecera.extend_from_slice(avcc.get(i + 2..i + 2 + largo)?);
            i += 2 + largo;
        }
    }
    Some(Config { largo_nal, cabecera })
}

/// Una muestra en Annex B; si es clave, con los SPS y PPS delante.
pub fn annex_b(c: &Config, muestra: &[u8], clave: bool) -> Vec<u8> {
    let mut v = Vec::with_capacity(muestra.len() + c.cabecera.len() + 16);
    if clave {
        v.extend_from_slice(&c.cabecera);
    }
    let mut i = 0;
    while i + c.largo_nal <= muestra.len() {
        let largo = muestra[i..i + c.largo_nal].iter().fold(0usize, |a, &b| (a << 8) | usize::from(b));
        i += c.largo_nal;
        let Some(nal) = muestra.get(i..i + largo) else { break };
        v.extend_from_slice(&INICIO);
        v.extend_from_slice(nal);
        i += largo;
    }
    v
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn de_avcc_a_annex_b() {
        // avcC con longitudes de 4 bytes, un SPS [0x67, 0xaa] y un PPS [0x68].
        let avcc = [1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 2, 0x67, 0xaa, 1, 0, 1, 0x68];
        let c = config(&avcc).unwrap_or(Config { largo_nal: 0, cabecera: Vec::new() });
        assert_eq!(c.largo_nal, 4);
        assert_eq!(c.cabecera, [0, 0, 0, 1, 0x67, 0xaa, 0, 0, 0, 1, 0x68]);
        let muestra = [0, 0, 0, 2, 0x65, 9, 0, 0, 0, 1, 0x06];
        assert_eq!(annex_b(&c, &muestra, false), [0, 0, 0, 1, 0x65, 9, 0, 0, 0, 1, 0x06]);
        assert_eq!(annex_b(&c, &muestra, true).len(), 11 + 11);
    }
}
