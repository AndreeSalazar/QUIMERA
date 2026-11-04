//! Cambiar el árbol después de analizarlo (lo que hace JavaScript a través del DOM): crear
//! nodos, moverlos, quitarlos, cambiar atributos y texto, y `innerHTML` en los dos sentidos.
//!
//! Un nodo quitado sigue en el arreglo, suelto (sin padre): JavaScript puede volver a meterlo.

use crate::arbol::{Dato, Documento, Elemento, Nodo, NodoId};
use crate::registro::Cambio;
use crate::toques::Toque;
use alloc::string::String;
use alloc::vec::Vec;

/// El nombre de un `DocumentFragment`: un elemento que nunca entra en el árbol (al meterlo, entran
/// sus hijos). El analizador no puede crear un elemento con este nombre.
pub const FRAGMENTO: &str = "#document-fragment";

/// Elementos sin cierre (`<br>`): al escribir HTML no llevan `</br>`.
const VACIOS: &[&str] =
    &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

impl Documento {
    /// Un nodo nuevo, suelto.
    pub fn crear(&mut self, dato: Dato) -> NodoId {
        let id = NodoId(self.nodos.len() as u32);
        self.nodos.push(Nodo { padre: None, hijos: Vec::new(), dato });
        id
    }

    /// Un elemento nuevo, suelto.
    pub fn crear_elemento(&mut self, nombre: &str) -> NodoId {
        self.crear(Dato::Elemento(Elemento { nombre: nombre.to_ascii_lowercase(), atributos: Vec::new() }))
    }

    /// Si existe un nodo con este número.
    pub fn existe(&self, id: NodoId) -> bool {
        (id.0 as usize) < self.nodos.len()
    }

    /// Si `a` es `b` o uno de sus antepasados.
    pub fn contiene(&self, a: NodoId, b: NodoId) -> bool {
        let mut actual = Some(b);
        while let Some(x) = actual {
            if x == a {
                return true;
            }
            actual = self.nodo(x).padre;
        }
        false
    }

    /// Saca un nodo de su padre (queda suelto).
    pub fn quitar(&mut self, id: NodoId) {
        if self.cambios.is_some() && self.nodo(id).padre.is_some() {
            let (anterior, siguiente) = self.hermanos(id);
            let padre = self.nodo(id).padre.unwrap_or(Self::RAIZ);
            self.anotar(Cambio::Hijos { padre, anadidos: Vec::new(), quitados: alloc::vec![id], anterior, siguiente });
        }
        if let Some(p) = self.nodos[id.0 as usize].padre.take() {
            self.nodos[p.0 as usize].hijos.retain(|&h| h != id);
            self.tocar(Toque::Hijos(p));
        }
    }

    /// Mete `hijo` en `padre`, antes de `antes` (o al final). Si estaba en otro sitio, se mueve.
    /// `false` si no se puede: meter un nodo dentro de sí mismo, o `antes` no es hijo de `padre`.
    pub fn insertar(&mut self, padre: NodoId, hijo: NodoId, antes: Option<NodoId>) -> bool {
        if self.contiene(hijo, padre) || hijo == Self::RAIZ {
            return false;
        }
        if self.elemento(hijo).is_some_and(|e| e.nombre == FRAGMENTO) {
            // Un fragmento no entra: entran sus hijos, en orden, y él queda vacío.
            if antes.is_some_and(|a| self.nodo(a).padre != Some(padre)) {
                return false;
            }
            for h in self.nodo(hijo).hijos.clone() {
                self.insertar(padre, h, antes);
            }
            return true;
        }
        if antes.is_some_and(|a| self.nodo(a).padre != Some(padre) || a == hijo) {
            return false;
        }
        self.quitar(hijo);
        let hijos = &mut self.nodos[padre.0 as usize].hijos;
        let i = antes.and_then(|a| hijos.iter().position(|&h| h == a)).unwrap_or(hijos.len());
        hijos.insert(i, hijo);
        self.nodos[hijo.0 as usize].padre = Some(padre);
        self.tocar(Toque::Hijos(padre));
        if self.cambios.is_some() {
            let (anterior, siguiente) = self.hermanos(hijo);
            self.anotar(Cambio::Hijos {
                padre,
                anadidos: alloc::vec![hijo],
                quitados: Vec::new(),
                anterior,
                siguiente,
            });
        }
        true
    }

    /// Cambia (o crea) un atributo.
    pub fn poner_atributo(&mut self, id: NodoId, nombre: &str, valor: &str) {
        let mut nota = None;
        if let Dato::Elemento(e) = &mut self.nodos[id.0 as usize].dato {
            let nombre = nombre.to_ascii_lowercase();
            let viejo = match e.atributos.iter_mut().find(|(n, _)| *n == nombre) {
                Some((_, v)) => Some(core::mem::replace(v, String::from(valor))),
                None => {
                    e.atributos.push((nombre.clone(), String::from(valor)));
                    None
                }
            };
            nota = Some(Cambio::Atributo { nodo: id, nombre, viejo });
        }
        if nota.is_some() {
            self.tocar(Toque::Atributo(id, crate::toques::huella_atributo(nombre)));
        }
        if let Some(c) = nota.filter(|_| self.cambios.is_some()) {
            self.anotar(c);
        }
    }

    /// Quita un atributo.
    pub fn quitar_atributo(&mut self, id: NodoId, nombre: &str) {
        let mut nota = None;
        if let Dato::Elemento(e) = &mut self.nodos[id.0 as usize].dato {
            let nombre = nombre.to_ascii_lowercase();
            if let Some(i) = e.atributos.iter().position(|(n, _)| *n == nombre) {
                let (nombre, viejo) = e.atributos.remove(i);
                nota = Some(Cambio::Atributo { nodo: id, nombre, viejo: Some(viejo) });
            }
        }
        if nota.is_some() {
            self.tocar(Toque::Atributo(id, crate::toques::huella_atributo(nombre)));
        }
        if let Some(c) = nota.filter(|_| self.cambios.is_some()) {
            self.anotar(c);
        }
    }

    /// `textContent = texto`: en un texto o comentario cambia su contenido; en un elemento
    /// sustituye todos sus hijos por un solo texto (o ninguno si está vacío).
    pub fn poner_texto(&mut self, id: NodoId, texto: &str) {
        match &mut self.nodos[id.0 as usize].dato {
            Dato::Texto(t) | Dato::Comentario(t) => {
                let viejo = core::mem::replace(t, String::from(texto));
                self.anotar(Cambio::Texto { nodo: id, viejo });
                self.tocar(Toque::Texto(id));
            }
            Dato::Elemento(_) | Dato::Documento => {
                self.vaciar(id);
                if !texto.is_empty() {
                    let t = self.crear(Dato::Texto(String::from(texto)));
                    self.insertar(id, t, None);
                }
            }
            Dato::Doctype(_) => {}
        }
    }

    /// Quita todos los hijos de un nodo.
    pub fn vaciar(&mut self, id: NodoId) {
        let quitados = core::mem::take(&mut self.nodos[id.0 as usize].hijos);
        for &h in &quitados {
            self.nodos[h.0 as usize].padre = None;
        }
        if !quitados.is_empty() {
            self.tocar(Toque::Hijos(id));
        }
        if self.cambios.is_some() && !quitados.is_empty() {
            self.anotar(Cambio::Hijos { padre: id, anadidos: Vec::new(), quitados, anterior: None, siguiente: None });
        }
    }

    /// `innerHTML = html`: analiza el trozo (como dentro de `<body>`) y lo pone de hijos.
    pub fn poner_html(&mut self, id: NodoId, html: &str) {
        self.vaciar(id);
        let otro = crate::analizar(&alloc::format!("<body>{html}"));
        let Some(body) = otro.buscar("body") else { return };
        for &h in &otro.nodo(body).hijos {
            let copia = self.importar(&otro, h);
            self.insertar(id, copia, None);
        }
    }

    /// Copia el nodo `id` de `otro` (con sus descendientes) en este documento, suelto.
    pub fn importar(&mut self, otro: &Documento, id: NodoId) -> NodoId {
        self.sin_anotar(|d| d.importar_sin_anotar(otro, id))
    }

    fn importar_sin_anotar(&mut self, otro: &Documento, id: NodoId) -> NodoId {
        let nuevo = self.crear(otro.nodo(id).dato.clone());
        for &h in &otro.nodo(id).hijos {
            let c = self.importar_sin_anotar(otro, h);
            self.insertar(nuevo, c, None);
        }
        nuevo
    }

    /// `innerHTML`: los hijos de un nodo escritos como HTML.
    pub fn html_de(&self, id: NodoId) -> String {
        let mut s = String::new();
        for &h in &self.nodo(id).hijos {
            self.escribir_html(h, &mut s);
        }
        s
    }

    /// `outerHTML`: el nodo y sus hijos escritos como HTML.
    pub fn escribir_html(&self, id: NodoId, s: &mut String) {
        let escapar = |t: &str, atributo: bool| {
            let t = t.replace('&', "&amp;").replace('\u{a0}', "&nbsp;");
            if atributo { t.replace('"', "&quot;") } else { t.replace('<', "&lt;").replace('>', "&gt;") }
        };
        match &self.nodo(id).dato {
            Dato::Texto(t) => {
                let crudo = self.nodo(id).padre.and_then(|p| self.elemento(p)).is_some_and(|e| {
                    matches!(e.nombre.as_str(), "script" | "style" | "noscript" | "xmp" | "iframe" | "plaintext")
                });
                s.push_str(&if crudo { t.clone() } else { escapar(t, false) });
            }
            Dato::Comentario(c) => s.push_str(&alloc::format!("<!--{c}-->")),
            Dato::Doctype(n) => s.push_str(&alloc::format!("<!DOCTYPE {n}>")),
            Dato::Documento => s.push_str(&self.html_de(id)),
            Dato::Elemento(e) => {
                s.push('<');
                s.push_str(&e.nombre);
                for (n, v) in &e.atributos {
                    s.push_str(&alloc::format!(" {n}=\"{}\"", escapar(v, true)));
                }
                s.push('>');
                if !VACIOS.contains(&e.nombre.as_str()) {
                    s.push_str(&self.html_de(id));
                    s.push_str(&alloc::format!("</{}>", e.nombre));
                }
            }
        }
    }
}

#[cfg(test)]
mod pruebas {
    use crate::{Dato, Documento, analizar};
    use alloc::string::String;

    #[test]
    fn crear_mover_y_escribir() {
        let mut d = analizar("<div id=a><p>uno</p><p>dos</p></div>");
        let div = d.buscar("div").expect("div");
        let p2 = d.nodo(div).hijos[1];
        let span = d.crear_elemento("SPAN");
        d.poner_atributo(span, "Class", "x\"y");
        d.poner_texto(span, "<hola> & adiós");
        assert!(d.insertar(div, span, Some(p2)));
        assert_eq!(d.html_de(div), "<p>uno</p><span class=\"x&quot;y\">&lt;hola&gt; &amp; adiós</span><p>dos</p>");
        assert!(!d.insertar(span, div, None), "un nodo no entra en su propio hijo");
        let body = d.buscar("body").expect("body");
        assert!(d.insertar(body, p2, None), "mover");
        d.quitar(span);
        assert_eq!(d.html_de(div), "<p>uno</p>");
        d.poner_html(div, "<b>negrita</b> y <br>texto<!--c-->");
        assert_eq!(d.html_de(div), "<b>negrita</b> y <br>texto<!--c-->");
        assert_eq!(d.texto(div), "negrita y texto");
        d.quitar_atributo(div, "ID");
        assert_eq!(d.elemento(div).map(|e| e.atributos.len()), Some(0));
        let t = d.crear(Dato::Texto(String::from("a")));
        assert!(d.existe(t) && d.nodo(t).padre.is_none());
        assert!(!d.insertar(Documento::RAIZ, Documento::RAIZ, None));
    }
}
