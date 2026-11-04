//! Analizador de HTML5 de QUIMERA, `no_std` y sin dependencias.
//!
//! `analizar(texto)` devuelve el árbol del documento, recuperándose de los errores como un
//! navegador: cualquier texto da un árbol con `<html>`, `<head>` y `<body>`.
//!
//! ```
//! let doc = quimera_html::analizar("<title>Hola</title><p>uno<p>dos");
//! assert_eq!(doc.texto(doc.buscar("title").unwrap()), "Hola");
//! ```
//!
//! Inspiración: html5ever (Servo) y el estándar de HTML. Lo que falta está en
//! `docs/HTML-CSS.md`.
#![no_std]

extern crate alloc;

mod arbol;
mod cambios;
mod constructor;
mod entidades;
mod extranjero;
mod registro;
mod tokenizador;
mod toques;

pub use arbol::{Dato, Documento, Elemento, Nodo, NodoId};
pub use cambios::FRAGMENTO;
pub use constructor::analizar;
pub use registro::Cambio;
pub use toques::{MAX_TOQUES, Toque, Toques};
