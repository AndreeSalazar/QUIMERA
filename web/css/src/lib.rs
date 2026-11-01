//! Analizador de CSS de QUIMERA, `no_std` y sin dependencias.
//!
//! - [`analizar`]: una hoja de estilo → reglas (selectores + declaraciones) y reglas `@media`.
//! - [`declaraciones`]: el atributo `style="..."`.
//! - [`Selector::casa`]: si un selector casa con un elemento de cualquier árbol que implemente
//!   [`ElementoCss`]; [`Selector::especificidad`] para la cascada.
//! - [`color`], [`longitud`], [`piezas`]: leer valores.
//!
//! ```
//! let hoja = quimera_css::analizar("p.nota { color: #c00; margin: 0 auto }");
//! let d = &hoja.reglas[0].declaraciones[0];
//! assert_eq!(quimera_css::color(&d.valor), Some(quimera_css::Color::rgb(204, 0, 0)));
//! ```
//!
//! Inspiración: Stylo y cssparser (Servo) y CSS Syntax Level 3. Lo que falta está en
//! `docs/HTML-CSS.md`.
#![no_std]

extern crate alloc;

mod colores;
mod hoja;
mod selectores;
mod tokenizador;
mod valores;

pub use hoja::{Declaracion, Hoja, Regla, ReglaMedio, analizar, declaraciones, texto};
pub use selectores::{Atributo, Combinador, Compuesto, ElementoCss, Pseudo, Selector};
pub use tokenizador::{Token, tokens};
pub use valores::{Color, Longitud, color, longitud, piezas};

#[cfg(test)]
mod pruebas {
    #[test]
    fn la_tabla_de_colores_esta_ordenada() {
        assert!(super::colores::NOMBRES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(super::colores::NOMBRES.len(), 148);
    }
}
