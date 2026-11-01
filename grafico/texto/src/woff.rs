//! WOFF 1.0 → TrueType/OpenType (*sfnt*), con el DEFLATE propio de `quimera-comprimir`.
//!
//! Un WOFF es una fuente TrueType u OpenType con cada tabla comprimida aparte con zlib (o sin
//! comprimir si no ganaba nada). Se descomprime cada tabla y se vuelve a armar el archivo tal
//! como era: la cabecera *sfnt*, el directorio de tablas y cada tabla alineada a 4 bytes.
//! WOFF2 (Brotli y tablas transformadas) es otro formato y todavía no está.

use alloc::vec::Vec;

/// Más que esto no es una fuente razonable (y protege de archivos que piden memoria sin fin).
const MAX_FUENTE: usize = 64 << 20;

fn u16_en(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(i..i + 2)?.try_into().ok()?))
}

fn u32_en(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

/// Si los bytes son un WOFF 1.0.
pub fn es_woff(d: &[u8]) -> bool {
    d.starts_with(b"wOFF")
}

/// Los bytes TrueType/OpenType de un WOFF 1.0; `None` si está roto.
pub fn a_sfnt(d: &[u8]) -> Option<Vec<u8>> {
    if !es_woff(d) {
        return None;
    }
    let sabor = u32_en(d, 4)?;
    let n = usize::from(u16_en(d, 12)?);
    if n == 0 || n > 512 {
        return None;
    }
    // (etiqueta, suma de control, desde, comprimido, original) de cada tabla.
    let mut tablas = Vec::with_capacity(n);
    for t in 0..n {
        let e = 44 + t * 20;
        let etiqueta = u32_en(d, e)?;
        let (desde, comprimido, original, suma) =
            (u32_en(d, e + 4)?, u32_en(d, e + 8)?, u32_en(d, e + 12)?, u32_en(d, e + 16)?);
        tablas.push((etiqueta, suma, desde as usize, comprimido as usize, original as usize));
    }
    let total: usize = 12 + 16 * n + tablas.iter().map(|t| t.4.div_ceil(4) * 4).sum::<usize>();
    if total > MAX_FUENTE {
        return None;
    }
    let mut salida = Vec::with_capacity(total);
    // Cabecera sfnt: sabor, número de tablas y los tres campos para la búsqueda binaria.
    let potencia = 1usize << (usize::BITS - 1 - n.leading_zeros());
    let rango = potencia * 16;
    let selector = potencia.trailing_zeros() as u16;
    salida.extend_from_slice(&sabor.to_be_bytes());
    for x in [n as u16, rango as u16, selector, (n * 16 - rango) as u16] {
        salida.extend_from_slice(&x.to_be_bytes());
    }
    let mut posicion = 12 + 16 * n;
    for &(etiqueta, suma, _, _, original) in &tablas {
        for x in [etiqueta, suma, posicion as u32, original as u32] {
            salida.extend_from_slice(&x.to_be_bytes());
        }
        posicion += original.div_ceil(4) * 4;
    }
    for &(_, _, desde, comprimido, original) in &tablas {
        let datos = d.get(desde..desde.checked_add(comprimido)?)?;
        if comprimido < original {
            let tabla = quimera_comprimir::zlib(datos, original).ok()?;
            if tabla.len() != original {
                return None;
            }
            salida.extend_from_slice(&tabla);
        } else {
            salida.extend_from_slice(datos.get(..original)?);
        }
        salida.resize(salida.len().div_ceil(4) * 4, 0);
    }
    Some(salida)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Un WOFF hecho a mano con dos tablas: una sin comprimir y otra comprimida con zlib.
    #[test]
    fn rearma_las_tablas() {
        let crudo = *b"abcdefghij"; // 10 bytes, se rellena hasta 12
        // zlib de 40 letras «a» (12 bytes: comprime de verdad, como en un WOFF real).
        let z = [0x78u8, 0x9C, 0x4B, 0x4C, 0x24, 0x0E, 0x00, 0x00, 0x36, 0xEB, 0x0F, 0x29];
        let mut w = Vec::new();
        w.extend_from_slice(b"wOFF");
        w.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sabor TrueType
        w.extend_from_slice(&0u32.to_be_bytes()); // largo (no se mira)
        w.extend_from_slice(&2u16.to_be_bytes());
        w.resize(44, 0);
        let datos = 44 + 2 * 20;
        let tabla = |w: &mut Vec<u8>, etiqueta: &[u8; 4], desde: usize, comp: usize, orig: usize| {
            w.extend_from_slice(etiqueta);
            for x in [desde as u32, comp as u32, orig as u32, 7] {
                w.extend_from_slice(&x.to_be_bytes());
            }
        };
        tabla(&mut w, b"aaaa", datos, crudo.len(), crudo.len());
        tabla(&mut w, b"bbbb", datos + crudo.len(), z.len(), 40);
        w.extend_from_slice(&crudo);
        w.extend_from_slice(&z);
        let s = a_sfnt(&w).expect("un WOFF válido");
        assert_eq!(&s[..4], &[0, 1, 0, 0]);
        assert_eq!(u16_en(&s, 4), Some(2));
        assert_eq!(u32_en(&s, 12 + 8), Some(12 + 32), "la primera tabla, tras la cabecera y el directorio");
        assert_eq!(&s[44..54], b"abcdefghij");
        assert_eq!(&s[56..96], &[b'a'; 40], "la segunda, descomprimida y alineada a 4");
        assert!(a_sfnt(&w[..50]).is_none(), "cortado");
    }
}
