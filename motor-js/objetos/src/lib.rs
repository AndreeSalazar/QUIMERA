//! Los objetos de QUIMERA, construidos sobre `quimera-gc`.
//!
//! Cada objeto es una celda del GC con una cabecera fija y una referencia a otra celda con
//! sus propiedades (los *slots* de SpiderMonkey). Así un objeto puede crecer sin moverse:
//! si se llena, se crea una celda de propiedades más grande.
//!
//! | Campo | Objeto | Arreglo | Función | Nativa |
//! |---:|---|---|---|---|
//! | 0 | tipo | tipo | tipo | tipo |
//! | 1 | forma | forma | forma | forma |
//! | 2 | prototipo | prototipo | prototipo | prototipo |
//! | 3 | propiedades | propiedades | propiedades | propiedades |
//! | 4 | | elementos | índice de función | id de nativa |
//! | 5 | | longitud | entorno | |
//! | 6 | | | `this` léxico (flechas) | |
//!
//! Un **entorno** es más simple: `[tipo, padre, hueco0, hueco1...]`.
//!
//! Regla de oro: toda función que puede asignar memoria puede mover objetos. Por eso estas
//! funciones meten sus argumentos en la pila de raíces antes de asignar y los vuelven a leer
//! después; quien las llama no debe guardar un [`Valor`] de objeto en una variable de Rust a
//! través de una asignación sin leerlo de nuevo de sus registros.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod cadenas;
pub mod formas;
mod mundo;

pub use cadenas::Cadenas;
pub use formas::{FORMA_VACIA, Formas, IdForma};
pub use mundo::{Clase, Mundo};
pub use quimera_gc::{Config, Estadisticas, Monton};
pub use quimera_valor::{Ref, Tipo, Valor};
