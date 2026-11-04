//! Los toques: qué nodos han cambiado desde la última vez, para que el estilo rehaga solo lo
//! tocado (y no el documento entero en cada cambio de JavaScript).
//!
//! Siempre encendido y barato: cada cambio apunta un número de nodo y su clase. Es distinto del
//! registro de `registro.rs`, que guarda el detalle para `MutationObserver` y solo se enciende
//! si alguien observa. Si se juntan demasiados, se apunta «todo» y se olvida la lista: quien
//! los lea lo rehará entero. No cuenta para la igualdad de documentos.

use crate::arbol::{Documento, NodoId};
use alloc::vec::Vec;

/// Qué se tocó.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toque {
    /// Un atributo del elemento (incluidos `class`, `id` y `style`).
    Atributo(NodoId),
    /// Los hijos del nodo: entró, salió o se movió alguno.
    Hijos(NodoId),
    /// El contenido de un nodo de texto.
    Texto(NodoId),
}

/// Los toques juntados desde la última vez que se tomaron.
#[derive(Debug, Clone, Default)]
pub struct Toques {
    /// En el orden en que pasaron (sin repetir el último).
    pub lista: Vec<Toque>,
    /// Hubo demasiados: hay que rehacerlo todo.
    pub todo: bool,
}

/// Cuántos toques se guardan como mucho antes de pasar a «todo».
pub const MAX_TOQUES: usize = 20_000;

impl PartialEq for Toques {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Toques {}

impl Toques {
    pub(crate) fn tocar(&mut self, t: Toque) {
        if self.todo || self.lista.last() == Some(&t) {
            return;
        }
        if self.lista.len() >= MAX_TOQUES {
            self.todo = true;
            self.lista = Vec::new();
            return;
        }
        self.lista.push(t);
    }

    /// No hubo ningún cambio.
    pub fn vacios(&self) -> bool {
        !self.todo && self.lista.is_empty()
    }
}

impl Documento {
    /// Los toques desde la última vez (y los olvida).
    pub fn tomar_toques(&mut self) -> Toques {
        core::mem::take(&mut self.toques)
    }

    pub(crate) fn tocar(&mut self, t: Toque) {
        self.toques.tocar(t);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::analizar;

    #[test]
    fn cada_cambio_deja_su_toque() {
        let mut d = analizar("<div id=a><p>uno</p></div>");
        d.tomar_toques();
        let div = d.buscar("div").unwrap();
        let p = d.buscar("p").unwrap();
        d.poner_atributo(div, "class", "x");
        let nuevo = d.crear_elemento("span");
        d.insertar(div, nuevo, None);
        let texto = d.nodo(p).hijos[0];
        d.poner_texto(texto, "dos");
        d.quitar(p);
        let t = d.tomar_toques();
        assert_eq!(t.lista, [Toque::Atributo(div), Toque::Hijos(div), Toque::Texto(texto), Toque::Hijos(div)]);
        assert!(d.tomar_toques().vacios(), "tomarlos los olvida");
        assert_eq!(analizar("<p>"), analizar("<p>"), "no cuentan para la igualdad");
    }

    #[test]
    fn demasiados_pasan_a_todo() {
        let mut t = Toques::default();
        for i in 0..=MAX_TOQUES as u32 {
            t.tocar(Toque::Atributo(NodoId(i)));
        }
        assert!(t.todo && t.lista.is_empty() && !t.vacios());
    }
}
