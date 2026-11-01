//! Un elemento del árbol de `quimera-html` visto por los selectores de `quimera-css`.

use quimera_css::ElementoCss;
use quimera_html::{Dato, Documento, NodoId};

/// Un elemento (documento + número), barato de copiar.
#[derive(Debug, Clone, Copy)]
pub struct Nodo<'a> {
    /// El documento.
    pub doc: &'a Documento,
    /// El nodo (siempre un elemento).
    pub id: NodoId,
}

impl<'a> Nodo<'a> {
    fn hermano(&self, paso: isize) -> Option<Nodo<'a>> {
        let padre = self.doc.nodo(self.id).padre?;
        let hijos = &self.doc.nodo(padre).hijos;
        let mut i = hijos.iter().position(|&h| h == self.id)? as isize;
        loop {
            i += paso;
            let h = *hijos.get(usize::try_from(i).ok()?)?;
            if self.doc.elemento(h).is_some() {
                return Some(Nodo { doc: self.doc, id: h });
            }
        }
    }
}

impl ElementoCss for Nodo<'_> {
    fn nombre(&self) -> &str {
        self.doc.elemento(self.id).map_or("", |e| e.nombre.as_str())
    }

    fn atributo(&self, nombre: &str) -> Option<&str> {
        self.doc.elemento(self.id)?.atributo(nombre)
    }

    fn padre(&self) -> Option<Self> {
        let p = self.doc.nodo(self.id).padre?;
        self.doc.elemento(p).map(|_| Nodo { doc: self.doc, id: p })
    }

    fn anterior(&self) -> Option<Self> {
        self.hermano(-1)
    }

    fn siguiente(&self) -> Option<Self> {
        self.hermano(1)
    }

    fn vacio(&self) -> bool {
        self.doc.nodo(self.id).hijos.iter().all(|&h| matches!(self.doc.nodo(h).dato, Dato::Comentario(_)))
    }

    fn tiene_clase(&self, clase: &str) -> bool {
        self.doc.elemento(self.id).is_some_and(|e| e.tiene_clase(clase))
    }
}
