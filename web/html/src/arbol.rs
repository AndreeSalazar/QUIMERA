//! El árbol del documento: nodos en un arreglo, enlazados por número (sin `Rc` ni punteros).
//!
//! Es el árbol que produce el analizador. `quimera-dom` lo envuelve para JavaScript; el
//! estilo y la maquetación lo recorren directamente.

use alloc::string::String;
use alloc::vec::Vec;

/// Número de un nodo dentro de su documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodoId(pub u32);

/// Un elemento: `<a href="...">`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Elemento {
    /// Nombre en minúsculas (`div`, `p`...).
    pub nombre: String,
    /// Atributos en orden, con el nombre en minúsculas (el primero gana si se repite).
    pub atributos: Vec<(String, String)>,
}

impl Elemento {
    /// El valor de un atributo.
    pub fn atributo(&self, nombre: &str) -> Option<&str> {
        self.atributos.iter().find(|(n, _)| n == nombre).map(|(_, v)| v.as_str())
    }

    /// Si `class` contiene esta clase.
    pub fn tiene_clase(&self, clase: &str) -> bool {
        self.atributo("class").is_some_and(|c| c.split_ascii_whitespace().any(|x| x == clase))
    }
}

/// Qué es un nodo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dato {
    /// La raíz.
    Documento,
    /// `<!DOCTYPE html>` (con su nombre).
    Doctype(String),
    /// Un elemento.
    Elemento(Elemento),
    /// Texto.
    Texto(String),
    /// `<!-- ... -->`.
    Comentario(String),
}

/// Un nodo con su lugar en el árbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nodo {
    /// El padre (ninguno para la raíz).
    pub padre: Option<NodoId>,
    /// Los hijos en orden.
    pub hijos: Vec<NodoId>,
    /// Qué es.
    pub dato: Dato,
}

/// Un documento HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Documento {
    pub(crate) nodos: Vec<Nodo>,
}

impl Default for Documento {
    fn default() -> Self {
        Documento { nodos: alloc::vec![Nodo { padre: None, hijos: Vec::new(), dato: Dato::Documento }] }
    }
}

impl Documento {
    /// La raíz (el nodo documento).
    pub const RAIZ: NodoId = NodoId(0);

    /// Un nodo.
    pub fn nodo(&self, id: NodoId) -> &Nodo {
        &self.nodos[id.0 as usize]
    }

    /// Cuántos nodos tiene.
    pub fn total(&self) -> usize {
        self.nodos.len()
    }

    /// El elemento de un nodo, si lo es.
    pub fn elemento(&self, id: NodoId) -> Option<&Elemento> {
        match &self.nodo(id).dato {
            Dato::Elemento(e) => Some(e),
            _ => None,
        }
    }

    /// Añade `dato` como último hijo de `padre`. Dos textos seguidos se unen en uno.
    pub fn anadir(&mut self, padre: NodoId, dato: Dato) -> NodoId {
        if let Dato::Texto(t) = &dato
            && let Some(&ultimo) = self.nodo(padre).hijos.last()
            && let Dato::Texto(previo) = &mut self.nodos[ultimo.0 as usize].dato
        {
            previo.push_str(t);
            return ultimo;
        }
        let id = NodoId(self.nodos.len() as u32);
        self.nodos.push(Nodo { padre: Some(padre), hijos: Vec::new(), dato });
        self.nodos[padre.0 as usize].hijos.push(id);
        id
    }

    /// Añade a un elemento los atributos que aún no tiene (un segundo `<html lang>` o `<body class>`).
    pub fn anadir_atributos(&mut self, id: NodoId, atributos: Vec<(String, String)>) {
        if let Dato::Elemento(e) = &mut self.nodos[id.0 as usize].dato {
            for (n, v) in atributos {
                if !e.atributos.iter().any(|(x, _)| *x == n) {
                    e.atributos.push((n, v));
                }
            }
        }
    }

    /// El primer elemento con este nombre, en orden de documento.
    pub fn buscar(&self, nombre: &str) -> Option<NodoId> {
        self.descendientes(Self::RAIZ).find(|&id| self.elemento(id).is_some_and(|e| e.nombre == nombre))
    }

    /// Todos los nodos bajo `id` (sin él), en orden de documento.
    pub fn descendientes(&self, id: NodoId) -> impl Iterator<Item = NodoId> + '_ {
        let mut pila: Vec<NodoId> = self.nodo(id).hijos.iter().rev().copied().collect();
        core::iter::from_fn(move || {
            let n = pila.pop()?;
            pila.extend(self.nodo(n).hijos.iter().rev());
            Some(n)
        })
    }

    /// El texto de un nodo y sus descendientes, junto (`textContent`).
    pub fn texto(&self, id: NodoId) -> String {
        let mut s = String::new();
        if let Dato::Texto(t) = &self.nodo(id).dato {
            s.push_str(t);
        }
        for d in self.descendientes(id) {
            if let Dato::Texto(t) = &self.nodo(d).dato {
                s.push_str(t);
            }
        }
        s
    }

    /// El hermano anterior que es un elemento (para selectores como `:first-child` o `+`).
    pub fn elemento_anterior(&self, id: NodoId) -> Option<NodoId> {
        let padre = self.nodo(id).padre?;
        let hijos = &self.nodo(padre).hijos;
        let i = hijos.iter().position(|&h| h == id)?;
        hijos[..i].iter().rev().copied().find(|&h| self.elemento(h).is_some())
    }

    /// El árbol como texto, un nodo por línea y sangrado (el formato de las pruebas html5lib).
    pub fn volcar(&self) -> String {
        let mut s = String::new();
        for id in self.descendientes(Self::RAIZ) {
            let mut nivel = 0;
            let mut p = self.nodo(id).padre;
            while let Some(x) = p.filter(|x| *x != Self::RAIZ) {
                nivel += 1;
                p = self.nodo(x).padre;
            }
            for _ in 0..nivel {
                s.push_str("  ");
            }
            match &self.nodo(id).dato {
                Dato::Documento => {}
                Dato::Doctype(n) => s.push_str(&alloc::format!("<!DOCTYPE {n}>")),
                Dato::Elemento(e) => {
                    s.push('<');
                    s.push_str(&e.nombre);
                    for (n, v) in &e.atributos {
                        s.push_str(&alloc::format!(" {n}=\"{v}\""));
                    }
                    s.push('>');
                }
                Dato::Texto(t) => s.push_str(&alloc::format!("\"{t}\"")),
                Dato::Comentario(c) => s.push_str(&alloc::format!("<!-- {c} -->")),
            }
            s.push('\n');
        }
        s
    }
}
