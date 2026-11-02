//! Formas (*shapes*, *hidden classes*): qué propiedades tiene un objeto y en qué hueco está cada una.
//!
//! Dos objetos creados igual (`{x: 1, y: 2}`) comparten forma, así que leer `o.x` se reduce a
//! "¿es la forma de siempre? entonces el hueco 0". Es la base de las cachés en línea de
//! SpiderMonkey y V8. Las formas viven fuera del GC y no se liberan (como en un motor real,
//! son pocas comparadas con los objetos).
//!
//! **Modo diccionario** (como en V8): cada forma nueva copia las claves de la anterior, así que
//! un objeto de N claves dejaría N formas con N²/2 claves en total. Los diccionarios de
//! traducciones de Steam (miles de claves) se comían 1,7 GB y segundos de CPU. Pasadas
//! [`LIMITE_DICCIONARIO`] claves, el objeto recibe una forma propia que crece en su sitio, sin
//! copias ni transiciones. Las cachés en línea no guardan nada de esas formas (pueden ganar o
//! perder claves sin cambiar de número), así que acceder a esos objetos va por el camino lento.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Identificador de una forma.
pub type IdForma = u32;

/// La forma vacía, con la que nace todo objeto.
pub const FORMA_VACIA: IdForma = 0;

/// Claves a partir de las que un objeto pasa a modo diccionario. Alto para que los prototipos
/// normales (`jQuery.fn` tiene unas 200) sigan compartiendo formas y cachés.
pub const LIMITE_DICCIONARIO: usize = 1024;

#[derive(Debug, Clone, Default)]
struct Forma {
    /// Clave (id de cadena) → hueco.
    huecos: BTreeMap<u32, u32>,
    /// Clave añadida → forma siguiente.
    transiciones: BTreeMap<u32, IdForma>,
    /// Claves en orden de creación (el orden de `for-in` y de `Object.keys`).
    orden: Vec<u32>,
    /// Forma propia de un objeto en modo diccionario: crece y encoge en su sitio.
    diccionario: bool,
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
        if let Some(f) = self.formas.get_mut(forma as usize).filter(|f| f.diccionario) {
            if !f.huecos.contains_key(&clave) {
                f.huecos.insert(clave, f.orden.len() as u32);
                f.orden.push(clave);
            }
            return forma;
        }
        if let Some(&siguiente) = self.formas.get(forma as usize).and_then(|f| f.transiciones.get(&clave)) {
            return siguiente;
        }
        let mut nueva = self.formas.get(forma as usize).cloned().unwrap_or_default();
        nueva.transiciones.clear();
        if nueva.orden.len() >= LIMITE_DICCIONARIO {
            // La forma propia de este objeto: sin transición desde la anterior (nadie más la usa).
            nueva.diccionario = true;
            nueva.huecos.insert(clave, nueva.orden.len() as u32);
            nueva.orden.push(clave);
            self.formas.push(nueva);
            return (self.formas.len() - 1) as IdForma;
        }
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

    /// Si `forma` es la forma propia de un objeto en modo diccionario.
    #[inline]
    pub fn es_diccionario(&self, forma: IdForma) -> bool {
        self.formas.get(forma as usize).is_some_and(|f| f.diccionario)
    }

    /// Quita `clave` de una forma diccionario, en su sitio (los huecos de detrás se corren uno).
    pub fn quitar(&mut self, forma: IdForma, clave: u32) {
        let Some(f) = self.formas.get_mut(forma as usize).filter(|f| f.diccionario) else { return };
        let Some(hueco) = f.huecos.remove(&clave) else { return };
        f.orden.remove(hueco as usize);
        for (i, k) in f.orden.iter().enumerate().skip(hueco as usize) {
            f.huecos.insert(*k, i as u32);
        }
    }

    /// Cambia `clave` por `nueva` en una forma diccionario, en el mismo hueco.
    pub fn renombrar(&mut self, forma: IdForma, clave: u32, nueva: u32) {
        let Some(f) = self.formas.get_mut(forma as usize).filter(|f| f.diccionario) else { return };
        let Some(hueco) = f.huecos.remove(&clave) else { return };
        f.huecos.insert(nueva, hueco);
        f.orden[hueco as usize] = nueva;
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

    #[test]
    fn modo_diccionario_sin_copias() {
        let mut f = Formas::default();
        let mut forma = FORMA_VACIA;
        for k in 0..LIMITE_DICCIONARIO as u32 + 5000 {
            forma = f.anadir(forma, k);
        }
        // Una forma por clave hasta el límite, y una sola (propia) para todo lo demás.
        assert_eq!(f.total(), LIMITE_DICCIONARIO + 2);
        assert!(f.es_diccionario(forma) && !f.es_diccionario(FORMA_VACIA));
        assert_eq!((f.buscar(forma, 7), f.buscar(forma, 3000)), (Some(7), Some(3000)));
        f.quitar(forma, 7);
        assert_eq!((f.buscar(forma, 7), f.buscar(forma, 8), f.claves(forma)[7]), (None, Some(7), 8));
        f.renombrar(forma, 8, 99_999);
        assert_eq!((f.buscar(forma, 8), f.buscar(forma, 99_999)), (None, Some(7)));
    }
}
