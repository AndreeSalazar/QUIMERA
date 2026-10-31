//! Formas (*shapes*, *hidden classes*): qué propiedades tiene un objeto y en qué hueco está cada una.
//!
//! Dos objetos creados igual (`{x: 1, y: 2}`) comparten forma, así que leer `o.x` se reduce a
//! "¿es la forma de siempre? entonces el hueco 0". Es la base de las cachés en línea de
//! SpiderMonkey y V8. Las formas viven fuera del GC y no se liberan (como en un motor real,
//! son pocas comparadas con los objetos).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Identificador de una forma.
pub type IdForma = u32;

/// La forma vacía, con la que nace todo objeto.
pub const FORMA_VACIA: IdForma = 0;

#[derive(Debug, Clone, Default)]
struct Forma {
    /// Clave (id de cadena) → hueco.
    huecos: BTreeMap<u32, u32>,
    /// Clave añadida → forma siguiente.
    transiciones: BTreeMap<u32, IdForma>,
    /// Claves en orden de creación (el orden de `for-in` y de `Object.keys`).
    orden: Vec<u32>,
}

/// La tabla de formas.
#[derive(Debug, Clone)]
pub struct Formas {
    formas: Vec<Forma>,
}

impl Default for Formas {
    fn default() -> Self {
        Formas { formas: alloc::vec![Forma::default()] }
    }
}

impl Formas {
    /// El hueco de `clave` en `forma`, si la tiene.
    pub fn buscar(&self, forma: IdForma, clave: u32) -> Option<u32> {
        self.formas.get(forma as usize)?.huecos.get(&clave).copied()
    }

    /// Cuántos huecos usa la forma.
    pub fn huecos(&self, forma: IdForma) -> u32 {
        self.formas.get(forma as usize).map_or(0, |f| f.orden.len() as u32)
    }

    /// Las claves en orden de creación.
    pub fn claves(&self, forma: IdForma) -> &[u32] {
        self.formas.get(forma as usize).map_or(&[], |f| &f.orden)
    }

    /// La forma que resulta de añadir `clave`. La misma secuencia de claves da siempre la misma forma.
    pub fn anadir(&mut self, forma: IdForma, clave: u32) -> IdForma {
        if let Some(&siguiente) = self.formas.get(forma as usize).and_then(|f| f.transiciones.get(&clave)) {
            return siguiente;
        }
        let mut nueva = self.formas.get(forma as usize).cloned().unwrap_or_default();
        nueva.transiciones.clear();
        let hueco = nueva.orden.len() as u32;
        nueva.huecos.insert(clave, hueco);
        nueva.orden.push(clave);
        let id = self.formas.len() as IdForma;
        self.formas.push(nueva);
        if let Some(f) = self.formas.get_mut(forma as usize) {
            f.transiciones.insert(clave, id);
        }
        id
    }

    /// Cuántas formas existen.
    pub fn total(&self) -> usize {
        self.formas.len()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_mismo_orden_da_la_misma_forma() {
        let mut f = Formas::default();
        let a = f.anadir(FORMA_VACIA, 10);
        let ab = f.anadir(a, 20);
        let otra_a = f.anadir(FORMA_VACIA, 10);
        assert_eq!(f.anadir(otra_a, 20), ab, "transición reutilizada");
        let b = f.anadir(FORMA_VACIA, 20);
        let ba = f.anadir(b, 10);
        assert_ne!(ab, ba, "otro orden, otra forma");
        assert_eq!((f.buscar(ab, 10), f.buscar(ab, 20), f.buscar(ab, 30)), (Some(0), Some(1), None));
        assert_eq!(f.claves(ab), [10, 20]);
        assert_eq!(f.total(), 5);
    }
}
