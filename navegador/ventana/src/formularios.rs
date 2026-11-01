//! Formularios: lo que la persona escribe y marca en los controles de la página, y el envío.
//!
//! - **Estado**: lo escrito en cada campo, las casillas marcadas, la opción elegida de cada lista
//!   y el campo con el foco. La maquetación lo lee para dibujar los controles ([`Formulario`]).
//!   Se vacía al cambiar de página.
//! - **Envío** (HTML, «form submission»): se juntan los controles con `name` del `<form>` (las
//!   casillas solo si están marcadas; el botón solo el que se pulsó), se codifican como
//!   `application/x-www-form-urlencoded` y se mandan a `action`: por GET en la consulta de la
//!   dirección, por POST en el cuerpo.

use quimera_html::{Documento, NodoId};
use quimera_maquetacion::Formulario;
use quimera_red::Url;
use std::collections::HashMap;

/// Lo que la persona cambió en los controles de la página.
#[derive(Debug, Default)]
pub struct Estado {
    /// Lo escrito en cada campo, o el texto de la opción elegida en cada lista.
    escrito: HashMap<NodoId, String>,
    /// Casillas y botones de opción.
    marcados: HashMap<NodoId, bool>,
    /// El campo donde va lo que se teclea.
    pub foco: Option<NodoId>,
}

impl Formulario for Estado {
    fn valor(&self, control: NodoId) -> Option<String> {
        self.escrito.get(&control).cloned()
    }
    fn marcado(&self, control: NodoId) -> Option<bool> {
        self.marcados.get(&control).copied()
    }
    fn foco(&self) -> Option<NodoId> {
        self.foco
    }
}

/// Qué es un control: el `type` de un `<input>` en minúsculas (`text` si no tiene), o el nombre
/// del elemento (`select`, `textarea`, `button`).
fn tipo(doc: &Documento, id: NodoId) -> Option<String> {
    let e = doc.elemento(id)?;
    match e.nombre.as_str() {
        "input" => Some(e.atributo("type").map_or_else(|| String::from("text"), str::to_ascii_lowercase)),
        "select" | "textarea" => Some(e.nombre.clone()),
        "button" => Some(e.atributo("type").map_or_else(|| String::from("submit"), str::to_ascii_lowercase)),
        _ => None,
    }
}

/// Si en un control de este tipo se escribe.
fn escribible(t: &str) -> bool {
    !matches!(
        t,
        "submit"
            | "image"
            | "reset"
            | "button"
            | "checkbox"
            | "radio"
            | "hidden"
            | "file"
            | "select"
            | "color"
            | "range"
    )
}

/// Si `id` es un campo donde se escribe.
pub fn es_campo(doc: &Documento, id: NodoId) -> bool {
    tipo(doc, id).is_some_and(|t| escribible(&t))
}

/// Las `<option>` de una lista: (nodo, texto).
fn opciones(doc: &Documento, lista: NodoId) -> Vec<(NodoId, String)> {
    doc.descendientes(lista)
        .filter(|&o| doc.elemento(o).is_some_and(|e| e.nombre == "option"))
        .map(|o| (o, String::from(doc.texto(o).trim())))
        .collect()
}

/// El `<form>` al que pertenece un control (el antepasado más cercano).
pub fn formulario_de(doc: &Documento, control: NodoId) -> Option<NodoId> {
    let mut actual = doc.nodo(control).padre;
    while let Some(p) = actual {
        if doc.elemento(p).is_some_and(|e| e.nombre == "form") {
            return Some(p);
        }
        actual = doc.nodo(p).padre;
    }
    None
}

impl Estado {
    /// Lo que vale un campo de texto ahora: lo escrito, o lo que trae la página.
    fn texto_de(&self, doc: &Documento, id: NodoId) -> String {
        if let Some(t) = self.escrito.get(&id) {
            return t.clone();
        }
        match doc.elemento(id) {
            Some(e) if e.nombre == "textarea" => doc.texto(id),
            Some(e) => String::from(e.atributo("value").unwrap_or("")),
            None => String::new(),
        }
    }

    /// Si una casilla está marcada ahora.
    fn marcada(&self, doc: &Documento, id: NodoId) -> bool {
        self.marcados
            .get(&id)
            .copied()
            .unwrap_or_else(|| doc.elemento(id).is_some_and(|e| e.atributo("checked").is_some()))
    }

    /// La opción elegida de una lista: la que eligió la persona, la marcada con `selected` o la
    /// primera.
    fn elegida(&self, doc: &Documento, lista: NodoId) -> Option<NodoId> {
        let ops = opciones(doc, lista);
        if let Some(t) = self.escrito.get(&lista) {
            return ops.iter().find(|(_, x)| x == t).map(|(o, _)| *o);
        }
        let marcada = ops.iter().find(|(o, _)| doc.elemento(*o).is_some_and(|e| e.atributo("selected").is_some()));
        marcada.or(ops.first()).map(|(o, _)| *o)
    }

    /// Se pulsó el control `id`. Devuelve el botón si hay que enviar su formulario.
    pub fn pulsar(&mut self, doc: &Documento, id: NodoId) -> Option<NodoId> {
        let t = tipo(doc, id)?;
        self.foco = None;
        match t.as_str() {
            "submit" | "image" => return Some(id),
            "checkbox" => {
                let m = self.marcada(doc, id);
                self.marcados.insert(id, !m);
            }
            "radio" => {
                // Al marcar uno se desmarcan los de su grupo (mismo `name` en el mismo formulario).
                let nombre = doc.elemento(id).and_then(|e| e.atributo("name")).map(String::from);
                let raiz = formulario_de(doc, id).unwrap_or(Documento::RAIZ);
                for otro in doc.descendientes(raiz) {
                    let mismo = doc.elemento(otro).is_some_and(|e| {
                        e.nombre == "input"
                            && e.atributo("type").is_some_and(|x| x.eq_ignore_ascii_case("radio"))
                            && e.atributo("name").map(String::from) == nombre
                    });
                    if mismo && nombre.is_some() {
                        self.marcados.insert(otro, false);
                    }
                }
                self.marcados.insert(id, true);
            }
            "select" => {
                // Sin lista desplegable todavía: cada clic pasa a la opción siguiente.
                let ops = opciones(doc, id);
                let actual = self.elegida(doc, id).and_then(|e| ops.iter().position(|(o, _)| *o == e));
                if let Some((_, texto)) = ops.get(actual.map_or(0, |i| (i + 1) % ops.len())) {
                    self.escrito.insert(id, texto.clone());
                }
            }
            "reset" => {
                let raiz = formulario_de(doc, id).unwrap_or(Documento::RAIZ);
                for d in doc.descendientes(raiz).collect::<Vec<_>>() {
                    self.escrito.remove(&d);
                    self.marcados.remove(&d);
                }
            }
            x if escribible(x) => self.foco = Some(id),
            _ => {}
        }
        None
    }

    /// Escribe `texto` en el campo con el foco.
    pub fn escribir(&mut self, doc: &Documento, texto: &str) {
        let Some(id) = self.foco else { return };
        let mut t = self.texto_de(doc, id);
        t.push_str(texto);
        self.escrito.insert(id, t);
    }

    /// Borra la última letra del campo con el foco.
    pub fn borrar(&mut self, doc: &Documento) {
        let Some(id) = self.foco else { return };
        let mut t = self.texto_de(doc, id);
        t.pop();
        self.escrito.insert(id, t);
    }

    /// Los pares (nombre, valor) que manda el formulario `form` al enviarlo con `boton`.
    pub fn pares(&self, doc: &Documento, form: NodoId, boton: Option<NodoId>) -> Vec<(String, String)> {
        let mut pares = Vec::new();
        for d in doc.descendientes(form) {
            let (Some(e), Some(t)) = (doc.elemento(d), tipo(doc, d)) else { continue };
            let Some(nombre) = e.atributo("name").filter(|n| !n.is_empty()) else { continue };
            if e.atributo("disabled").is_some() {
                continue;
            }
            let nombre = String::from(nombre);
            let valor = |defecto: &str| String::from(e.atributo("value").unwrap_or(defecto));
            match t.as_str() {
                "submit" | "button" | "reset" if boton != Some(d) => {}
                "submit" => pares.push((nombre, valor(""))),
                "image" if boton == Some(d) => {
                    pares.push((format!("{nombre}.x"), String::from("0")));
                    pares.push((format!("{nombre}.y"), String::from("0")));
                }
                "checkbox" | "radio" if self.marcada(doc, d) => pares.push((nombre, valor("on"))),
                "select" => {
                    if let Some(o) = self.elegida(doc, d) {
                        let v = doc.elemento(o).and_then(|x| x.atributo("value")).map(String::from);
                        pares.push((nombre, v.unwrap_or_else(|| String::from(doc.texto(o).trim()))));
                    }
                }
                "checkbox" | "radio" | "image" | "file" | "button" | "reset" => {}
                _ => pares.push((nombre, self.texto_de(doc, d))),
            }
        }
        pares
    }

    /// El envío del formulario de `control` (pulsado, o donde se dio Enter): la dirección y, si es
    /// por POST, el cuerpo. Con Enter se usa el primer botón de enviar del formulario.
    pub fn envio(&self, doc: &Documento, control: NodoId, base: &Url) -> Option<(Url, Option<Vec<u8>>)> {
        let form = formulario_de(doc, control)?;
        let boton = if tipo(doc, control).is_some_and(|t| matches!(t.as_str(), "submit" | "image")) {
            Some(control)
        } else {
            doc.descendientes(form).find(|&d| tipo(doc, d).is_some_and(|t| matches!(t.as_str(), "submit" | "image")))
        };
        let e = doc.elemento(form)?;
        let attr = |n: &str| boton.and_then(|b| doc.elemento(b)).and_then(|b| b.atributo(n)).or(e.atributo(n));
        let mut destino = base.unir(attr("formaction").or(attr("action")).unwrap_or("")).ok()?;
        destino.fragmento = None;
        let post = attr("formmethod").or(attr("method")).is_some_and(|m| m.eq_ignore_ascii_case("post"));
        let cuerpo = codificar(&self.pares(doc, form, boton));
        if post {
            Some((destino, Some(cuerpo.into_bytes())))
        } else {
            destino.consulta = Some(cuerpo);
            Some((destino, None))
        }
    }
}

/// Codifica los pares como `application/x-www-form-urlencoded` (UTF-8; espacio → `+`).
pub fn codificar(pares: &[(String, String)]) -> String {
    let uno = |s: &str| {
        let mut r = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => r.push(char::from(b)),
                b' ' => r.push('+'),
                _ => r.push_str(&format!("%{b:02X}")),
            }
        }
        r
    };
    pares.iter().map(|(k, v)| format!("{}={}", uno(k), uno(v))).collect::<Vec<_>>().join("&")
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use quimera_html::analizar;

    fn por_nombre(doc: &Documento, n: &str) -> NodoId {
        doc.descendientes(Documento::RAIZ)
            .find(|&d| doc.elemento(d).and_then(|e| e.atributo("name")) == Some(n))
            .expect("control")
    }

    #[test]
    fn escribir_marcar_y_enviar() {
        let doc = analizar(
            "<form action='/buscar?viejo=1#x'><input name=q value='hola'><input type=password name=clave>
             <input type=checkbox name=c><input type=radio name=r value=a checked><input type=radio name=r value=b>
             <select name=s><option value=1>Uno<option>Dos</select><textarea name=t>texto</textarea>
             <input type=hidden name=h value=oculto><input type=submit name=ir value=Ir><input name=sin disabled></form>",
        );
        let base = Url::analizar("https://ejemplo.com/a/b").expect("url");
        let mut f = Estado::default();
        let q = por_nombre(&doc, "q");
        assert_eq!(f.pulsar(&doc, q), None);
        assert_eq!(f.foco, Some(q));
        f.escribir(&doc, " mundo");
        f.borrar(&doc);
        f.escribir(&doc, "o & ñ");
        f.pulsar(&doc, por_nombre(&doc, "c"));
        let radios: Vec<NodoId> = doc
            .descendientes(Documento::RAIZ)
            .filter(|&d| doc.elemento(d).and_then(|e| e.atributo("value")) == Some("b"))
            .collect();
        f.pulsar(&doc, radios[0]);
        f.pulsar(&doc, por_nombre(&doc, "s"));
        let (url, cuerpo) = f.envio(&doc, q, &base).expect("envío");
        assert_eq!(cuerpo, None);
        assert_eq!(
            url.to_string(),
            "https://ejemplo.com/buscar?q=hola+mundo+%26+%C3%B1&clave=&c=on&r=b&s=Dos&t=texto&h=oculto&ir=Ir"
        );
    }

    #[test]
    fn post_con_el_boton_pulsado() {
        let doc = analizar(
            "<form method=POST action='https://html.duckduckgo.com/html/'><input name=q value='quimera rust'>
             <input type=submit name=a value=A><input type=submit name=b value=B></form>",
        );
        let base = Url::analizar("https://html.duckduckgo.com/html/").expect("url");
        let f = Estado::default();
        let (url, cuerpo) = f.envio(&doc, por_nombre(&doc, "b"), &base).expect("envío");
        assert_eq!(url.to_string(), "https://html.duckduckgo.com/html/");
        assert_eq!(cuerpo.as_deref(), Some(&b"q=quimera+rust&b=B"[..]));
        assert!(f.envio(&analizar("<input name=q>"), NodoId(3), &base).is_none(), "sin <form> no se envía");
    }
}
