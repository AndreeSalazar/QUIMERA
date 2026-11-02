//! Constructor del árbol: tokens → documento, con la recuperación de errores de los navegadores.
//!
//! Una versión compacta de los modos de inserción del estándar. Crea `<html>`, `<head>` y
//! `<body>` aunque falten; lleva a `<head>` lo que va ahí (`title`, `meta`, `style`...); cierra
//! solos los `<p>`, `<li>`, `<dt>`/`<dd>`, `<option>` y celdas de tabla; crea `<tbody>`/`<tr>`
//! implícitos; y deja caer los cierres que no casan con nada. Falta, sobre todo, el
//! «algoritmo de adopción» que reabre `<b>`/`<i>` mal anidados.

use crate::arbol::{Dato, Documento, Elemento, NodoId};
use crate::extranjero;
use crate::tokenizador::{Token, Tokenizador};
use alloc::string::String;
use alloc::vec::Vec;

/// Elementos sin contenido ni cierre.
const VACIOS: &[&str] =
    &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];
/// Los que van en `<head>` si llegan antes del cuerpo.
const DE_CABECERA: &[&str] = &["base", "link", "meta", "noscript", "script", "style", "template", "title"];
/// Los que cierran un `<p>` abierto.
const CIERRAN_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "li",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "ul",
    "dd",
    "dt",
    "listing",
    "plaintext",
];
/// Límites que un cierre no cruza al buscar su apertura (el «ámbito» del estándar).
const LIMITES: &[&str] = &["html", "table", "td", "th", "caption", "marquee", "object", "applet", "template"];
const TITULOS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
/// Elementos de HTML que, dentro de SVG o MathML, cierran el contenido extranjero.
const ROMPEN_EXTRANJERO: &[&str] = &[
    "b",
    "big",
    "blockquote",
    "body",
    "br",
    "center",
    "code",
    "dd",
    "div",
    "dl",
    "dt",
    "em",
    "embed",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "hr",
    "i",
    "img",
    "li",
    "listing",
    "menu",
    "meta",
    "nobr",
    "ol",
    "p",
    "pre",
    "ruby",
    "s",
    "small",
    "span",
    "strong",
    "strike",
    "sub",
    "sup",
    "table",
    "tt",
    "u",
    "ul",
    "var",
];

struct Constructor {
    doc: Documento,
    /// Elementos abiertos, de fuera hacia dentro.
    pila: Vec<NodoId>,
    html: Option<NodoId>,
    head: Option<NodoId>,
    body: Option<NodoId>,
    /// Tras `<pre>` o `<textarea>` se quita un salto de línea inicial.
    quitar_salto: bool,
}

/// Analiza un documento HTML entero.
pub fn analizar(html: &str) -> Documento {
    let texto = html.replace("\r\n", "\n").replace('\r', "\n");
    let mut c = Constructor {
        doc: Documento::default(),
        pila: Vec::new(),
        html: None,
        head: None,
        body: None,
        quitar_salto: false,
    };
    let mut t = Tokenizador::nuevo(&texto);
    while let Some(token) = t.siguiente() {
        c.token(token);
    }
    c.asegurar_body();
    c.doc
}

impl Constructor {
    fn actual(&self) -> NodoId {
        self.pila.last().copied().unwrap_or(Documento::RAIZ)
    }

    fn nombre(&self, id: NodoId) -> &str {
        self.doc.elemento(id).map_or("", |e| e.nombre.as_str())
    }

    fn nombre_actual(&self) -> &str {
        self.nombre(self.actual())
    }

    fn crear(&mut self, padre: NodoId, nombre: &str, atributos: Vec<(String, String)>) -> NodoId {
        self.doc.anadir(padre, Dato::Elemento(Elemento { nombre: String::from(nombre), atributos }))
    }

    fn asegurar_html(&mut self) -> NodoId {
        if let Some(h) = self.html {
            return h;
        }
        let h = self.crear(Documento::RAIZ, "html", Vec::new());
        self.html = Some(h);
        self.pila = alloc::vec![h];
        h
    }

    /// Crea `<head>` (abierto: lo que llegue hasta `</head>` va dentro) si aún no existe.
    fn asegurar_head(&mut self) -> NodoId {
        if let Some(h) = self.head {
            return h;
        }
        let html = self.asegurar_html();
        let h = self.crear(html, "head", Vec::new());
        self.head = Some(h);
        self.pila = alloc::vec![html, h];
        h
    }

    /// Si `<head>` sigue abierto (está en la pila).
    /// Dentro de un `<template>` abierto: su contenido se queda en él (aunque esté en `<head>`).
    fn en_plantilla(&self) -> bool {
        self.pila.iter().any(|&id| self.nombre(id) == "template")
    }

    fn head_abierto(&self) -> bool {
        self.head.is_some_and(|h| self.pila.contains(&h))
    }

    fn asegurar_body(&mut self) -> NodoId {
        if let Some(b) = self.body {
            return b;
        }
        self.asegurar_head();
        let html = self.asegurar_html();
        let b = self.crear(html, "body", Vec::new());
        self.body = Some(b);
        self.pila = alloc::vec![html, b];
        b
    }

    fn token(&mut self, t: Token) {
        match t {
            Token::Doctype(n) => {
                if self.html.is_none() {
                    self.doc.anadir(Documento::RAIZ, Dato::Doctype(n));
                }
            }
            Token::Comentario(c) => {
                let donde = if self.html.is_none() { Documento::RAIZ } else { self.actual() };
                self.doc.anadir(donde, Dato::Comentario(c));
            }
            Token::Texto(s) => self.texto(s),
            Token::Inicio { nombre, atributos, autocierre } => self.apertura(&nombre, atributos, autocierre),
            Token::Fin(nombre) => self.cierre(&nombre),
        }
    }

    fn texto(&mut self, mut s: String) {
        if core::mem::take(&mut self.quitar_salto) && s.starts_with('\n') {
            s.remove(0);
        }
        if s.is_empty() {
            return;
        }
        let dentro_de_cabecera =
            (DE_CABECERA.contains(&self.nombre_actual()) || self.en_plantilla()) && self.body.is_none();
        if self.body.is_none() && !dentro_de_cabecera {
            // Antes del cuerpo: los blancos van a `<head>` (o a `<html>` tras `</head>`), o se
            // ignoran si aún no hay `<head>`; el primer carácter que no es blanco abre el cuerpo.
            let blancos = s.len() - s.trim_start_matches(|c: char| c.is_ascii_whitespace()).len();
            if blancos > 0 && self.head.is_some() {
                let donde = self.actual();
                self.doc.anadir(donde, Dato::Texto(String::from(&s[..blancos])));
            }
            if blancos == s.len() {
                return;
            }
            s = String::from(&s[blancos..]);
            self.asegurar_body();
        }
        let donde = self.actual();
        self.doc.anadir(donde, Dato::Texto(s));
    }

    fn apertura(&mut self, nombre: &str, atributos: Vec<(String, String)>, autocierre: bool) {
        match nombre {
            // Los atributos de `<html>`, `<head>` y `<body>` van al elemento (exista ya o no).
            "html" => {
                let h = self.asegurar_html();
                self.doc.anadir_atributos(h, atributos);
                return;
            }
            "head" if self.body.is_none() => {
                let h = self.asegurar_head();
                self.doc.anadir_atributos(h, atributos);
                return;
            }
            "body" => {
                let b = self.asegurar_body();
                self.doc.anadir_atributos(b, atributos);
                return;
            }
            n if self.body.is_none() && self.en_plantilla() => {
                let padre = self.actual();
                let id = self.crear(padre, n, atributos);
                if !VACIOS.contains(&n) {
                    self.pila.push(id);
                }
                return;
            }
            n if DE_CABECERA.contains(&n) && self.body.is_none() => {
                let head = self.asegurar_head();
                // Tras `</head>`, el estándar lo reabre un momento para meter el elemento.
                let padre = if self.head_abierto() { self.actual() } else { head };
                let id = self.crear(padre, n, atributos);
                if !VACIOS.contains(&n) && self.head_abierto() {
                    self.pila.push(id);
                }
                return;
            }
            _ => {}
        }
        self.asegurar_body();
        if let Some(svg) = self.extranjero(nombre) {
            return self.apertura_extranjera(nombre, atributos, autocierre, svg);
        }
        self.cierres_implicitos(nombre);
        let padre = self.padre_para(nombre);
        let id = self.crear(padre, nombre, atributos);
        // En HTML, `<div/>` no se cierra solo: solo los vacíos no tienen contenido.
        if !VACIOS.contains(&nombre) {
            self.pila.push(id);
        }
        self.quitar_salto = matches!(nombre, "pre" | "listing" | "textarea");
    }

    /// Si `nombre` va como SVG (`Some(true)`) o MathML (`Some(false)`) en vez de HTML. Un
    /// elemento de HTML «de los que rompen» (`<p>`, `<div>`...) cierra el SVG que lo rodea.
    fn extranjero(&mut self, nombre: &str) -> Option<bool> {
        let contexto = self.pila.iter().rev().find_map(|&id| match self.nombre(id) {
            "svg" => Some(Some(true)),
            "math" => Some(Some(false)),
            "foreignObject" | "desc" | "annotation-xml" => Some(None),
            _ => None,
        });
        match (contexto.flatten(), nombre) {
            (None, "svg") => Some(true),
            (None, "math") => Some(false),
            (None, _) => None,
            (Some(_), n) if ROMPEN_EXTRANJERO.contains(&n) => {
                // Sale del SVG/MathML: se cierra todo hasta el primer elemento de HTML.
                while let Some(&id) = self.pila.last() {
                    if self.extranjero_abierto(id) {
                        self.pila.pop();
                    } else {
                        break;
                    }
                }
                None
            }
            (Some(svg), _) => Some(svg),
        }
    }

    /// Si el elemento abierto `id` es SVG/MathML (está dentro de un `<svg>`/`<math>` sin
    /// punto de vuelta a HTML de por medio).
    fn extranjero_abierto(&self, id: NodoId) -> bool {
        let Some(i) = self.pila.iter().position(|&x| x == id) else { return false };
        self.pila[..=i].iter().rev().find_map(|&x| match self.nombre(x) {
            "svg" | "math" => Some(true),
            "foreignObject" | "desc" | "annotation-xml" => Some(x != id),
            _ => None,
        }) == Some(true)
    }

    /// Un elemento SVG o MathML: nombres con sus mayúsculas, y `<path/>` sí se cierra solo.
    fn apertura_extranjera(&mut self, nombre: &str, atributos: Vec<(String, String)>, autocierre: bool, svg: bool) {
        let nombre = if svg { extranjero::elemento_svg(nombre) } else { nombre };
        let atributos = atributos.into_iter().map(|(n, v)| (String::from(extranjero::atributo(&n, svg)), v)).collect();
        let padre = self.actual();
        let id = self.crear(padre, nombre, atributos);
        if !autocierre {
            self.pila.push(id);
        }
    }

    /// Lo que una apertura cierra antes de abrirse (`<p>` dentro de `<p>`, `<li>` tras `<li>`...).
    fn cierres_implicitos(&mut self, nombre: &str) {
        if CIERRAN_P.contains(&nombre) && self.en_ambito("p") {
            self.cerrar_hasta("p");
        }
        match nombre {
            "li" => self.cerrar_hermano(&["li"], &["ul", "ol", "table", "td", "th", "html"]),
            "dt" | "dd" => self.cerrar_hermano(&["dt", "dd"], &["dl", "table", "td", "th", "html"]),
            "option" => self.cerrar_hermano(&["option"], &["select", "datalist", "html"]),
            // En tablas, una fila nueva cierra la anterior con su celda dentro.
            "td" | "th" => self.cerrar_hermano(&["td", "th"], &["tr", "table", "html"]),
            "tr" => self.cerrar_hermano(&["tr"], &["tbody", "thead", "tfoot", "table", "html"]),
            "tbody" | "thead" | "tfoot" => self.cerrar_hermano(&["tbody", "thead", "tfoot"], &["table", "html"]),
            "a" if self.en_ambito("a") => self.cerrar_hasta("a"),
            n if TITULOS.contains(&n) && TITULOS.contains(&self.nombre_actual()) => {
                self.pila.pop();
            }
            _ => {}
        }
    }

    /// Cierra el elemento abierto más cercano de `cuales` (con lo que tenga dentro) si está
    /// antes que cualquiera de `limites`.
    fn cerrar_hermano(&mut self, cuales: &[&str], limites: &[&str]) {
        for i in (0..self.pila.len()).rev() {
            let n = self.nombre(self.pila[i]);
            if cuales.contains(&n) {
                self.pila.truncate(i);
                return;
            }
            if limites.contains(&n) {
                return;
            }
        }
    }

    /// Dónde va un elemento de tabla: crea los `<tbody>`/`<tr>` que falten.
    fn padre_para(&mut self, nombre: &str) -> NodoId {
        let actual = self.actual();
        let necesita = match (self.nombre(actual), nombre) {
            ("table", "tr") => Some("tbody"),
            ("table" | "tbody" | "thead" | "tfoot", "td" | "th") => {
                Some(if self.nombre(actual) == "table" { "tbody" } else { "tr" })
            }
            _ => None,
        };
        let Some(intermedio) = necesita else { return actual };
        let id = self.crear(actual, intermedio, Vec::new());
        self.pila.push(id);
        self.padre_para(nombre)
    }

    /// Si hay un `nombre` abierto sin cruzar un límite.
    fn en_ambito(&self, nombre: &str) -> bool {
        for &id in self.pila.iter().rev() {
            let n = self.nombre(id);
            // Sin distinguir mayúsculas: `</lineargradient>` cierra `<linearGradient>`.
            if n.eq_ignore_ascii_case(nombre) {
                return true;
            }
            if LIMITES.contains(&n) || (nombre == "p" && n == "button") {
                return false;
            }
        }
        false
    }

    /// Cierra el `nombre` abierto más cercano y todo lo que tenga dentro.
    fn cerrar_hasta(&mut self, nombre: &str) {
        if let Some(i) = self.pila.iter().rposition(|&id| self.nombre(id).eq_ignore_ascii_case(nombre)) {
            self.pila.truncate(i);
        }
    }

    fn cierre(&mut self, nombre: &str) {
        match nombre {
            "head" => {
                self.asegurar_head();
                if self.body.is_none() && self.head_abierto() {
                    let html = self.asegurar_html();
                    self.pila = alloc::vec![html];
                }
            }
            // Lo que venga después de `</body>` o `</html>` sigue yendo al cuerpo.
            "body" | "html" => {
                self.asegurar_body();
            }
            "p" if !self.en_ambito("p") => {
                // Dentro del cuerpo, `</p>` sin `<p>` crea un párrafo vacío; antes, se ignora.
                if self.body.is_none() {
                    return;
                }
                let actual = self.actual();
                self.crear(actual, "p", Vec::new());
            }
            "br" => self.apertura("br", Vec::new(), false),
            n if TITULOS.contains(&n) => {
                if let Some(i) = self.pila.iter().rposition(|&id| TITULOS.contains(&self.nombre(id))) {
                    self.pila.truncate(i);
                }
            }
            n if self.en_ambito(n) || DE_CABECERA.contains(&n) => self.cerrar_hasta(n),
            _ => {} // un cierre que no casa con nada se ignora
        }
    }
}
