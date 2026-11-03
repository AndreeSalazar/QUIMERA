//! El registro de cambios del árbol, para `MutationObserver`: mientras está encendido, cada
//! cambio de hijos, de atributo o de texto deja una nota. Apagado (lo normal) no cuesta nada.
//!
//! No se anotan las copias que se hacen dentro de nodos recién creados (`innerHTML`, `cloneNode`):
//! nadie puede estar observándolos todavía.

use crate::arbol::{Documento, NodoId};
use alloc::string::String;
use alloc::vec::Vec;

/// Un cambio del árbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cambio {
    /// Hijos que entraron o salieron de `padre`, con los hermanos que quedaron a sus lados.
    Hijos {
        /// El nodo que cambió.
        padre: NodoId,
        /// Los que entraron.
        anadidos: Vec<NodoId>,
        /// Los que salieron.
        quitados: Vec<NodoId>,
        /// El hermano de antes.
        anterior: Option<NodoId>,
        /// El hermano de después.
        siguiente: Option<NodoId>,
    },
    /// Un atributo que cambió (o se creó, o se quitó).
    Atributo {
        /// El elemento.
        nodo: NodoId,
        /// El atributo, en minúsculas.
        nombre: String,
        /// Su valor de antes (`None` si no lo tenía).
        viejo: Option<String>,
    },
    /// El texto de un nodo de texto o comentario.
    Texto {
        /// El nodo.
        nodo: NodoId,
        /// Su texto de antes.
        viejo: String,
    },
}

impl Documento {
    /// Enciende (o apaga) el registro de cambios.
    pub fn anotar_cambios(&mut self, si: bool) {
        match (si, self.cambios.is_some()) {
            (true, false) => self.cambios = Some(Vec::new()),
            (false, true) => self.cambios = None,
            _ => {}
        }
    }

    /// Los cambios anotados desde la última vez (el registro sigue encendido).
    pub fn tomar_cambios(&mut self) -> Vec<Cambio> {
        self.cambios.as_mut().map(core::mem::take).unwrap_or_default()
    }

    /// Hace `f` sin anotar nada (para copiar dentro de nodos recién creados).
    pub fn sin_anotar<R>(&mut self, f: impl FnOnce(&mut Documento) -> R) -> R {
        let guardado = self.cambios.take();
        let r = f(self);
        self.cambios = guardado;
        r
    }

    pub(crate) fn anotar(&mut self, c: Cambio) {
        if let Some(v) = &mut self.cambios {
            v.push(c);
        }
    }

    /// Los hermanos de `id` en su padre (el de antes y el de después).
    pub(crate) fn hermanos(&self, id: NodoId) -> (Option<NodoId>, Option<NodoId>) {
        let Some(p) = self.nodo(id).padre else { return (None, None) };
        let hijos = &self.nodo(p).hijos;
        let Some(i) = hijos.iter().position(|&h| h == id) else { return (None, None) };
        (i.checked_sub(1).map(|j| hijos[j]), hijos.get(i + 1).copied())
    }
}
