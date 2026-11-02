//! Léxico y parser de JavaScript de QUIMERA: texto → árbol de sintaxis ([`ast`]).
//!
//! 1. [`lexico`]: texto → tokens (las plantillas `` `a${x}` `` ya salen como una suma).
//! 2. [`parser`]: tokens → árbol, con la sintaxis moderna (clases, desestructuración,
//!    spread, `?.`, `async`...) convertida en la que entiende el compilador, como Babel.
//!
//! El compilador a bytecode está en `quimera-frontend`, un nivel por encima.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod ast;
pub mod fuente;
pub mod lexico;
pub mod parser;

pub use fuente::{Error, Pos, Resultado};
pub use parser::{analizar, analizar_modulo};
