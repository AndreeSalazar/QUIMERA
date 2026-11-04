//! Filtro de antepasados (un *bloom filter*, como el de Blink y Stylo): descarta sin recorrer el
//! árbol las reglas cuyo selector pide antepasados que el elemento no tiene.
//!
//! - De cada elemento: 256 bits con la etiqueta, el id y las clases de todos sus antepasados.
//! - De cada regla: los bits de lo que piden las partes de su selector que tienen que ser
//!   antepasados (las unidas a su derecha por un espacio o por `>`).
//!
//! Si la regla pide un bit que el elemento no tiene, no puede casar. Al revés no vale nada (dos
//! nombres pueden dar el mismo bit), así que lo que pasa el filtro se comprueba de verdad.

use quimera_css::{Combinador, Compuesto, Selector};
use quimera_html::{Documento, NodoId};

/// 256 bits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Filtro([u64; 4]);

impl Filtro {
    fn poner(&mut self, tipo: u8, nombre: &str) {
        // FNV-1a de 32 bits, con el tipo delante (`#id`, `.clase`, etiqueta).
        let mut h: u32 = 0x811c_9dc5;
        for b in core::iter::once(tipo).chain(nombre.bytes()) {
            h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
        }
        // Dos bits por nombre.
        for bit in [h & 255, (h >> 8) & 255] {
            self.0[(bit / 64) as usize] |= 1 << (bit % 64);
        }
    }

    fn poner_compuesto(&mut self, c: &Compuesto) {
        if let Some(t) = &c.etiqueta {
            self.poner(b't', t);
        }
        for id in &c.ids {
            self.poner(b'#', id);
        }
        for k in &c.clases {
            self.poner(b'.', k);
        }
    }

    /// Si tiene todos los bits de `otro`.
    pub(crate) fn contiene(&self, otro: &Filtro) -> bool {
        self.0.iter().zip(&otro.0).all(|(a, b)| a & b == *b)
    }

    /// Lo que pide el selector a los antepasados del elemento.
    pub(crate) fn de_selector(s: &Selector) -> Filtro {
        let mut f = Filtro::default();
        for (i, c) in s.partes.iter().enumerate().take(s.partes.len().saturating_sub(1)) {
            // Unido a la parte de su derecha por un espacio o por `>`: es antepasado de esa, y por
            // tanto del elemento (la de su derecha es el elemento, un antepasado o el hermano de
            // uno de los dos).
            if matches!(s.combinadores.get(i), Some(Combinador::Descendiente | Combinador::Hijo)) {
                f.poner_compuesto(c);
            }
        }
        f
    }

    /// Los antepasados de `id` (etiqueta, id y clases de cada uno).
    pub(crate) fn de_elemento(doc: &Documento, id: NodoId) -> Filtro {
        Filtro::desde(doc, doc.nodo(id).padre)
    }

    /// `id` y sus antepasados: los antepasados de su `::before` o `::after`.
    pub(crate) fn de_elemento_y_antepasados(doc: &Documento, id: NodoId) -> Filtro {
        Filtro::desde(doc, Some(id))
    }

    fn desde(doc: &Documento, primero: Option<NodoId>) -> Filtro {
        let mut f = Filtro::default();
        let mut actual = primero;
        while let Some(p) = actual {
            if let Some(e) = doc.elemento(p) {
                f.poner(b't', &e.nombre);
                if let Some(i) = e.atributo("id") {
                    f.poner(b'#', i);
                }
                if let Some(cl) = e.atributo("class") {
                    for k in cl.split_ascii_whitespace() {
                        f.poner(b'.', k);
                    }
                }
            }
            actual = doc.nodo(p).padre;
        }
        f
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn selector(s: &str) -> Selector {
        let hoja = quimera_css::analizar(&alloc::format!("{s} {{}}"));
        hoja.reglas
            .into_iter()
            .next()
            .and_then(|r| r.selectores.into_iter().next())
            .unwrap_or(Selector { partes: alloc::vec![], combinadores: alloc::vec![] })
    }

    #[test]
    fn descarta_lo_que_no_puede_casar() {
        let doc = quimera_html::analizar("<div id=a class='x y'><section><p id=p>t</p></section></div>");
        let p = doc.descendientes(Documento::RAIZ).find(|&i| doc.elemento(i).is_some_and(|e| e.nombre == "p"));
        let f = Filtro::de_elemento(&doc, p.unwrap_or(Documento::RAIZ));
        assert!(f.contiene(&Filtro::de_selector(&selector(".x p"))));
        assert!(f.contiene(&Filtro::de_selector(&selector("div#a > section > p"))));
        assert!(
            f.contiene(&Filtro::de_selector(&selector(".y section + p"))),
            "la de antes de un hermano es antepasado"
        );
        assert!(!f.contiene(&Filtro::de_selector(&selector(".z p"))));
        assert!(!f.contiene(&Filtro::de_selector(&selector("article p"))));
        // Lo que va unido por `+` o `~` no tiene por qué ser antepasado: no se pide.
        assert_eq!(Filtro::de_selector(&selector(".z + p")), Filtro::default());
    }
}
