//! Frontend de QUIMERA: JavaScript → bytecode de registros.
//!
//! Tres pasos, cada uno en su módulo:
//! 1. [`lexico`]: texto → tokens.
//! 2. [`parser`]: tokens → árbol de sintaxis ([`ast`]).
//! 3. [`compilador`]: árbol → [`bytecode::Stencil`].
//!
//! La idea del *Stencil* viene de SpiderMonkey (`frontend/Stencil.cpp`): el resultado de
//! compilar no contiene objetos del GC, solo números, índices y cadenas. Se puede guardar,
//! compartir entre hilos o compilar en paralelo, y el motor lo convierte en funciones vivas
//! al ejecutarlo. El bytecode es de **registros**, no de pila: el perfil de Boa midió que
//! mover valores entre la pila y los registros se llevaba el 8-13 % del tiempo.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod ambitos;
pub mod ast;
pub mod bloques;
pub mod bytecode;
pub mod compilador;
pub mod fuente;
pub mod lexico;
pub mod parser;

pub use bytecode::Stencil;
pub use fuente::{Error, Pos, Resultado};
pub use parser::analizar;

/// Analiza y compila un programa: texto → Stencil.
pub fn compilar(fuente: &str) -> Resultado<Stencil> {
    let mut programa = parser::analizar(fuente)?;
    bloques::renombrar(&mut programa);
    compilador::compilar(&programa)
}
