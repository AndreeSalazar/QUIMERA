//! Rellenar polígonos con antialias sobre un lienzo RGBA.
//!
//! Cada fila de píxeles se mira en [`SUB`] líneas: en cada una se buscan los cruces con los
//! bordes y su sentido, y los tramos de dentro (regla `nonzero` o `evenodd`) suman su parte a
//! cada píxel, con la fracción exacta en los extremos. Luego se mezcla el color con esa cobertura
//! (`source-over`). Sobra para iconos y logotipos; no es tan fino como un rasterizador analítico.

use crate::mates::{suelo, techo};
use crate::ruta::Punto;
use alloc::vec::Vec;

/// Sub-líneas por fila de píxeles.
const SUB: usize = 5;

/// Un color RGBA sin premultiplicar, con alfa de 0 a 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Rojo.
    pub r: u8,
    /// Verde.
    pub g: u8,
    /// Azul.
    pub b: u8,
    /// Opacidad, de 0 a 1.
    pub a: f32,
}

/// Una imagen RGBA (sin premultiplicar), fila a fila.
pub struct Lienzo {
    /// Ancho en píxeles.
    pub ancho: u32,
    /// Alto en píxeles.
    pub alto: u32,
    /// Cuatro bytes por píxel.
    pub rgba: Vec<u8>,
}

impl Lienzo {
    /// Un lienzo transparente.
    pub fn nuevo(ancho: u32, alto: u32) -> Lienzo {
        Lienzo { ancho, alto, rgba: alloc::vec![0; (ancho * alto * 4) as usize] }
    }

    /// Rellena los polígonos (juntos: un agujero de uno resta de otro) con `color`.
    pub fn rellenar(&mut self, poligonos: &[Vec<Punto>], par_impar: bool, color: Rgba) {
        if color.a <= 0.0 || poligonos.is_empty() {
            return;
        }
        let bordes: Vec<(Punto, Punto)> = poligonos
            .iter()
            .flat_map(|p| p.iter().zip(p.iter().cycle().skip(1)).take(p.len()))
            .map(|(a, b)| (*a, *b))
            .filter(|(a, b)| a.1 != b.1)
            .collect();
        let arriba = bordes.iter().map(|(a, b)| a.1.min(b.1)).fold(f32::INFINITY, f32::min);
        let abajo = bordes.iter().map(|(a, b)| a.1.max(b.1)).fold(f32::NEG_INFINITY, f32::max);
        if !arriba.is_finite() {
            return;
        }
        let y0 = (suelo(arriba).max(0.0)) as u32;
        let y1 = (techo(abajo) as i64).clamp(0, i64::from(self.alto)) as u32;
        let mut cobertura = alloc::vec![0.0f32; self.ancho as usize + 1];
        let mut cruces: Vec<(f32, i32)> = Vec::new();
        for fila in y0..y1 {
            cobertura.iter_mut().for_each(|c| *c = 0.0);
            for k in 0..SUB {
                let y = fila as f32 + (k as f32 + 0.5) / SUB as f32;
                cruces.clear();
                for &(a, b) in &bordes {
                    let (lo, hi) = if a.1 < b.1 { (a, b) } else { (b, a) };
                    if y >= lo.1 && y < hi.1 {
                        let x = lo.0 + (y - lo.1) * (hi.0 - lo.0) / (hi.1 - lo.1);
                        cruces.push((x, if a.1 < b.1 { 1 } else { -1 }));
                    }
                }
                cruces.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
                self.tramos(&cruces, par_impar, &mut cobertura);
            }
            self.mezclar_fila(fila, &cobertura, color);
        }
    }

    /// Suma a `cobertura` los tramos de dentro de una sub-línea.
    fn tramos(&self, cruces: &[(f32, i32)], par_impar: bool, cobertura: &mut [f32]) {
        let mut vueltas = 0;
        for w in cruces.windows(2) {
            vueltas += w[0].1;
            let dentro = if par_impar { vueltas % 2 != 0 } else { vueltas != 0 };
            if dentro {
                sumar_tramo(cobertura, w[0].0, w[1].0, self.ancho as f32);
            }
        }
    }

    fn mezclar_fila(&mut self, fila: u32, cobertura: &[f32], color: Rgba) {
        let ancho = self.ancho as usize;
        let inicio = fila as usize * ancho * 4;
        let pixeles = self.rgba[inicio..inicio + ancho * 4].chunks_exact_mut(4);
        for (p, cob) in pixeles.zip(cobertura) {
            let c = (cob / SUB as f32).min(1.0) * color.a;
            if c <= 0.0 {
                continue;
            }
            let fondo = f32::from(p[3]) / 255.0;
            let a = c + fondo * (1.0 - c);
            let mezcla = |s: u8, d: u8| {
                ((f32::from(s) * c + f32::from(d) * fondo * (1.0 - c)) / a + 0.5).clamp(0.0, 255.0) as u8
            };
            (p[0], p[1], p[2]) = (mezcla(color.r, p[0]), mezcla(color.g, p[1]), mezcla(color.b, p[2]));
            p[3] = (a * 255.0 + 0.5) as u8;
        }
    }
}

/// Suma un tramo [x0, x1) a la fila: 1 en los píxeles enteros y la fracción en los extremos.
fn sumar_tramo(cobertura: &mut [f32], x0: f32, x1: f32, ancho: f32) {
    let (x0, x1) = (x0.clamp(0.0, ancho), x1.clamp(0.0, ancho));
    if x1 <= x0 {
        return;
    }
    let (i0, i1) = (suelo(x0) as usize, suelo(x1) as usize);
    if i0 == i1 {
        if let Some(c) = cobertura.get_mut(i0) {
            *c += x1 - x0;
        }
        return;
    }
    cobertura[i0] += (i0 + 1) as f32 - x0;
    for c in &mut cobertura[i0 + 1..i1] {
        *c += 1.0;
    }
    if let Some(c) = cobertura.get_mut(i1) {
        *c += x1 - i1 as f32;
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const NEGRO: Rgba = Rgba { r: 0, g: 0, b: 0, a: 1.0 };

    fn alfa(l: &Lienzo, x: u32, y: u32) -> u8 {
        l.rgba[((y * l.ancho + x) * 4 + 3) as usize]
    }

    #[test]
    fn un_cuadrado_con_media_columna() {
        let mut l = Lienzo::nuevo(10, 10);
        l.rellenar(&[alloc::vec![(2.0, 2.0), (6.5, 2.0), (6.5, 6.0), (2.0, 6.0)]], false, NEGRO);
        assert_eq!(alfa(&l, 3, 3), 255);
        assert_eq!(alfa(&l, 6, 3), 128);
        assert_eq!(alfa(&l, 7, 3), 0);
        assert_eq!(alfa(&l, 3, 6), 0);
    }

    #[test]
    fn agujero_con_las_dos_reglas() {
        let fuera = alloc::vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        // Mismo sentido: con nonzero el de dentro no hace agujero; con evenodd sí.
        let dentro = alloc::vec![(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)];
        let mut l = Lienzo::nuevo(10, 10);
        l.rellenar(&[fuera.clone(), dentro.clone()], false, NEGRO);
        assert_eq!(alfa(&l, 5, 5), 255);
        let mut l = Lienzo::nuevo(10, 10);
        l.rellenar(&[fuera, dentro], true, NEGRO);
        assert_eq!(alfa(&l, 5, 5), 0);
        assert_eq!(alfa(&l, 1, 1), 255);
    }
}
