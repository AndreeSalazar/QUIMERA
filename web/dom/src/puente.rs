//! El puente con la VM: las propiedades de los nodos (`textContent`, `id`, `style`...), y la
//! preparación de `window`, `document` y los prototipos con sus métodos (`metodos.rs`).

use crate::metodos::{self, TABLA};
use crate::{BIT_CLASES, BIT_DATOS, BIT_ESTILO, Estado, envolver, envolver_o_nulo, lista};
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use quimera_html::{Dato, Documento, NodoId};
use quimera_vm::operaciones::a_booleano;
use quimera_vm::{Anfitrion, Argumentos, Motor, Valor};

pub(crate) use crate::eventos::{correr_temporizadores, despachar, proximo_temporizador};

/// El anfitrión que ve la VM.
pub(crate) struct Puente(pub(crate) Rc<Estado>);

/// Lo que da una propiedad, antes de pasarlo a un valor de JavaScript.
enum Leido {
    Indefinido,
    Nodo(Option<NodoId>),
    Lista(Vec<NodoId>),
    Texto(String),
    Entero(i32),
    Booleano(bool),
    /// Un objeto del anfitrión sin caché (`style`, `classList`) con este número y prototipo.
    Objeto(u32, &'static str),
    Ventana,
}

/// Prepara la raíz del anfitrión, los prototipos y los globales (`window`, `document`...).
pub(crate) fn preparar(m: &mut Motor, est: &Estado) {
    for lista in ["nodos", "eventos", "temporizadores"] {
        let a = m.crear_arreglo(&[]);
        let raiz = m.raiz_anfitrion();
        m.definir(raiz, lista, a);
    }
    for (proto, padre) in [
        ("p_nodo", None),
        ("p_elemento", Some("p_nodo")),
        ("p_documento", Some("p_nodo")),
        ("p_evento", None),
        ("p_clases", None),
        ("p_estilo", None),
        ("p_datos", None),
    ] {
        let raiz = m.raiz_anfitrion();
        let p = match padre {
            Some(n) => m.leer_nombre(raiz, n),
            None => m.proto_objeto(),
        };
        let o = m.mundo.crear_objeto(p);
        let raiz = m.raiz_anfitrion();
        m.definir(raiz, proto, o);
    }
    for (dueno, metodos) in TABLA {
        for &(nombre, id) in *metodos {
            let f = m.crear_funcion_anfitrion(id);
            let raiz = m.raiz_anfitrion();
            let o = if *dueno == "window" { m.global() } else { m.leer_nombre(raiz, dueno) };
            m.definir(o, nombre, f);
        }
    }
    let global = m.global();
    for nombre in ["window", "self", "top", "parent"] {
        m.definir(global, nombre, global);
    }
    let documento = envolver(m, est, Documento::RAIZ);
    let global = m.global();
    m.definir(global, "document", documento);
    for (objeto, campos) in [
        ("location", [("href", est.url.as_str()), ("protocol", protocolo(&est.url))]),
        ("navigator", [("userAgent", "Mozilla/5.0 QUIMERA"), ("language", "es")]),
    ] {
        let o = m.crear_objeto();
        let marca = m.apilar(o);
        for (k, v) in campos {
            let v = m.cadena(v);
            let o = m.de_pila(marca);
            m.definir(o, k, v);
        }
        let (o, global) = (m.de_pila(marca), m.global());
        m.definir(global, objeto, o);
        m.soltar_pila(marca);
    }
    m.fijar_biblioteca();
}

fn protocolo(url: &str) -> &str {
    url.find(':').map_or("", |i| &url[..=i])
}

/// Los hermanos y su posición: (hijos del padre, índice del nodo).
fn posicion(doc: &Documento, id: NodoId) -> Option<(&[NodoId], usize)> {
    let hijos = &doc.nodo(doc.nodo(id).padre?).hijos;
    Some((hijos, hijos.iter().position(|&h| h == id)?))
}

fn es_elemento(doc: &Documento, id: NodoId) -> bool {
    doc.elemento(id).is_some()
}

/// Atributos que se leen y escriben como propiedades de texto (`a.href` → `href`).
fn atributo_reflejado(nombre: &str) -> Option<&'static str> {
    Some(match nombre {
        "id" => "id",
        "className" => "class",
        "href" => "href",
        "src" => "src",
        "name" => "name",
        "type" => "type",
        "alt" => "alt",
        "rel" => "rel",
        "lang" => "lang",
        "dir" => "dir",
        "placeholder" => "placeholder",
        "action" => "action",
        "method" => "method",
        "htmlFor" => "for",
        "target" => "target",
        "content" => "content",
        "width" => "width",
        "height" => "height",
        _ => return None,
    })
}

/// Atributos booleanos (`input.checked` → si existe `checked`).
fn atributo_booleano(nombre: &str) -> Option<&'static str> {
    Some(match nombre {
        "checked" => "checked",
        "disabled" => "disabled",
        "hidden" => "hidden",
        "selected" => "selected",
        "readOnly" => "readonly",
        "required" => "required",
        _ => return None,
    })
}

/// `nodeName`: la etiqueta en mayúsculas, o `#text`, `#comment`, `#document`.
fn nombre_de_nodo(doc: &Documento, id: NodoId) -> String {
    match &doc.nodo(id).dato {
        Dato::Elemento(e) if e.nombre == quimera_html::FRAGMENTO => e.nombre.clone(),
        Dato::Elemento(e) => e.nombre.to_ascii_uppercase(),
        Dato::Texto(_) => String::from("#text"),
        Dato::Comentario(_) => String::from("#comment"),
        Dato::Documento => String::from("#document"),
        Dato::Doctype(n) => n.clone(),
    }
}

/// El valor de un control (`input.value`): lo que trae su atributo, el texto de un `<textarea>`
/// o la opción elegida de un `<select>`.
fn valor_de_control(doc: &Documento, id: NodoId) -> String {
    match doc.elemento(id).map(|e| e.nombre.as_str()) {
        Some("textarea") => doc.texto(id),
        Some("select") => {
            let opciones: Vec<NodoId> =
                doc.descendientes(id).filter(|&o| doc.elemento(o).is_some_and(|e| e.nombre == "option")).collect();
            let elegida = opciones
                .iter()
                .find(|&&o| doc.elemento(o).is_some_and(|e| e.atributo("selected").is_some()))
                .or(opciones.first());
            elegida.map_or_else(String::new, |&o| {
                doc.elemento(o)
                    .and_then(|e| e.atributo("value"))
                    .map_or_else(|| doc.texto(o).trim().to_string(), String::from)
            })
        }
        _ => doc.elemento(id).and_then(|e| e.atributo("value")).unwrap_or("").to_string(),
    }
}

/// Las propiedades de árbol de un nodo (padres, hijos y hermanos).
fn leer_arbol(doc: &Documento, id: NodoId, nombre: &str) -> Option<Leido> {
    let n = doc.nodo(id);
    let elementos = |v: &[NodoId]| -> Vec<NodoId> { v.iter().copied().filter(|&h| es_elemento(doc, h)).collect() };
    Some(match nombre {
        "parentNode" => Leido::Nodo(n.padre),
        "parentElement" => Leido::Nodo(n.padre.filter(|&p| es_elemento(doc, p))),
        "firstChild" => Leido::Nodo(n.hijos.first().copied()),
        "lastChild" => Leido::Nodo(n.hijos.last().copied()),
        "childNodes" => Leido::Lista(n.hijos.clone()),
        "children" => Leido::Lista(elementos(&n.hijos)),
        "childElementCount" => Leido::Entero(elementos(&n.hijos).len() as i32),
        "firstElementChild" => Leido::Nodo(elementos(&n.hijos).first().copied()),
        "lastElementChild" => Leido::Nodo(elementos(&n.hijos).last().copied()),
        "nextSibling" => Leido::Nodo(posicion(doc, id).and_then(|(h, i)| h.get(i + 1).copied())),
        "previousSibling" => Leido::Nodo(posicion(doc, id).and_then(|(h, i)| i.checked_sub(1).map(|j| h[j]))),
        "nextElementSibling" => {
            Leido::Nodo(posicion(doc, id).and_then(|(h, i)| h[i + 1..].iter().copied().find(|&x| es_elemento(doc, x))))
        }
        "previousElementSibling" => Leido::Nodo(
            posicion(doc, id).and_then(|(h, i)| h[..i].iter().rev().copied().find(|&x| es_elemento(doc, x))),
        ),
        "ownerDocument" => Leido::Nodo(Some(Documento::RAIZ)),
        "isConnected" => Leido::Booleano(doc.contiene(Documento::RAIZ, id)),
        _ => return None,
    })
}

/// Las propiedades de `document`.
fn leer_documento(doc: &Documento, url: &str, nombre: &str) -> Option<Leido> {
    Some(match nombre {
        "body" => Leido::Nodo(doc.buscar("body")),
        "head" => Leido::Nodo(doc.buscar("head")),
        "documentElement" => Leido::Nodo(doc.buscar("html")),
        "title" => Leido::Texto(doc.buscar("title").map(|t| doc.texto(t).trim().to_string()).unwrap_or_default()),
        "URL" | "documentURI" => Leido::Texto(String::from(url)),
        "readyState" => Leido::Texto(String::from("complete")),
        "cookie" => Leido::Texto(String::new()),
        "defaultView" => Leido::Ventana,
        _ => return None,
    })
}

/// Las propiedades de un nodo.
fn leer_nodo(est: &Estado, id: NodoId, nombre: &str) -> Option<Leido> {
    let doc = est.doc.borrow();
    if !doc.existe(id) {
        return None;
    }
    if let Some(l) = leer_arbol(&doc, id, nombre) {
        return Some(l);
    }
    if matches!(doc.nodo(id).dato, Dato::Documento)
        && let Some(l) = leer_documento(&doc, &est.url, nombre)
    {
        return Some(l);
    }
    let el = doc.elemento(id);
    Some(match nombre {
        "nodeType" => Leido::Entero(match &doc.nodo(id).dato {
            Dato::Elemento(e) if e.nombre == quimera_html::FRAGMENTO => 11,
            Dato::Elemento(_) => 1,
            Dato::Texto(_) => 3,
            Dato::Comentario(_) => 8,
            Dato::Documento => 9,
            Dato::Doctype(_) => 10,
        }),
        "nodeName" => Leido::Texto(nombre_de_nodo(&doc, id)),
        "tagName" if el.is_some_and(|e| e.nombre != quimera_html::FRAGMENTO) => Leido::Texto(nombre_de_nodo(&doc, id)),
        "localName" => el.map_or(Leido::Indefinido, |e| Leido::Texto(e.nombre.clone())),
        "textContent" | "innerText" => Leido::Texto(doc.texto(id)),
        "nodeValue" | "data" => match &doc.nodo(id).dato {
            Dato::Texto(t) | Dato::Comentario(t) => Leido::Texto(t.clone()),
            _ => return None,
        },
        "innerHTML" => Leido::Texto(doc.html_de(id)),
        "outerHTML" => {
            let mut s = String::new();
            doc.escribir_html(id, &mut s);
            Leido::Texto(s)
        }
        "value" if el.is_some() => Leido::Texto(valor_de_control(&doc, id)),
        "style" if el.is_some() => Leido::Objeto(id.0 | BIT_ESTILO, "p_estilo"),
        "classList" if el.is_some() => Leido::Objeto(id.0 | BIT_CLASES, "p_clases"),
        "dataset" if el.is_some() => Leido::Objeto(id.0 | BIT_DATOS, "p_datos"),
        "title" => Leido::Texto(el.and_then(|e| e.atributo("title")).unwrap_or("").to_string()),
        _ => {
            let e = el?;
            if let Some(a) = atributo_reflejado(nombre) {
                Leido::Texto(e.atributo(a).unwrap_or("").to_string())
            } else {
                Leido::Booleano(e.atributo(atributo_booleano(nombre)?).is_some())
            }
        }
    })
}

/// `backgroundColor` → `background-color`.
fn nombre_css(js: &str) -> String {
    if js == "cssFloat" {
        return String::from("float");
    }
    let mut s = String::new();
    for c in js.chars() {
        if c.is_ascii_uppercase() {
            s.push('-');
            s.push(c.to_ascii_lowercase());
        } else {
            s.push(c);
        }
    }
    s
}

/// `dataset.miClave` → el atributo `data-mi-clave`. `None` para las claves internas.
fn atributo_de_dato(nombre: &str) -> Option<String> {
    if nombre.starts_with(['\u{1}', '\u{2}']) {
        return None;
    }
    let mut s = String::from("data-");
    for c in nombre.chars() {
        if c.is_ascii_uppercase() {
            s.push('-');
            s.push(c.to_ascii_lowercase());
        } else {
            s.push(c);
        }
    }
    Some(s)
}

/// Las declaraciones del atributo `style`: (propiedad, valor).
pub(crate) fn declaraciones(estilo: &str) -> Vec<(String, String)> {
    estilo
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// Cambia una propiedad en el atributo `style` (un valor vacío la quita).
pub(crate) fn poner_estilo(doc: &mut Documento, id: NodoId, propiedad: &str, valor: &str) {
    let actual = doc.elemento(id).and_then(|e| e.atributo("style")).unwrap_or("").to_string();
    let mut d = declaraciones(&actual);
    d.retain(|(k, _)| k != propiedad);
    if !valor.trim().is_empty() {
        d.push((String::from(propiedad), valor.trim().to_string()));
    }
    let texto: Vec<String> = d.iter().map(|(k, v)| alloc::format!("{k}: {v}")).collect();
    // Como Chromium: `a: b; c: d;`.
    let texto: String = texto.iter().map(|d| alloc::format!("{d};")).collect::<Vec<_>>().join(" ");
    doc.poner_atributo(id, "style", &texto);
}

impl Puente {
    fn leer_estilo(&self, id: NodoId, nombre: &str) -> Option<Leido> {
        if matches!(nombre, "setProperty" | "getPropertyValue" | "removeProperty") {
            return None;
        }
        let doc = self.0.doc.borrow();
        let estilo = doc.elemento(id)?.atributo("style").unwrap_or("");
        if nombre == "cssText" {
            return Some(Leido::Texto(String::from(estilo)));
        }
        let css = nombre_css(nombre);
        let v = declaraciones(estilo).into_iter().find(|(k, _)| *k == css).map(|(_, v)| v);
        Some(Leido::Texto(v.unwrap_or_default()))
    }

    fn a_valor(&self, m: &mut Motor, l: Leido) -> Valor {
        match l {
            Leido::Indefinido => Valor::INDEFINIDO,
            Leido::Nodo(n) => envolver_o_nulo(m, &self.0, n),
            Leido::Lista(v) => lista(m, &self.0, &v),
            Leido::Texto(t) => m.cadena(&t),
            Leido::Entero(n) => Valor::entero(n),
            Leido::Booleano(b) => Valor::booleano(b),
            Leido::Objeto(dato, proto) => {
                let raiz = m.raiz_anfitrion();
                let p = m.leer_nombre(raiz, proto);
                m.crear_objeto_anfitrion(dato, p)
            }
            Leido::Ventana => m.global(),
        }
    }

    /// `nodo.nombre = v`. `false` si no es una propiedad del anfitrión.
    fn escribir_nodo(&self, m: &Motor, id: NodoId, nombre: &str, v: Valor) -> bool {
        let texto = || m.texto(v);
        let mut doc = self.0.doc.borrow_mut();
        if !doc.existe(id) {
            return false;
        }
        let es_doc = matches!(doc.nodo(id).dato, Dato::Documento);
        match nombre {
            "textContent" | "innerText" | "nodeValue" | "data" => doc.poner_texto(id, &texto()),
            "innerHTML" => doc.poner_html(id, &texto()),
            "title" if es_doc => {
                if let Some(t) = doc.buscar("title") {
                    doc.poner_texto(t, &texto());
                }
            }
            "value" if doc.elemento(id).is_some_and(|e| e.nombre == "textarea") => doc.poner_texto(id, &texto()),
            "value" => doc.poner_atributo(id, "value", &texto()),
            "title" => doc.poner_atributo(id, "title", &texto()),
            _ => {
                if let Some(a) = atributo_reflejado(nombre) {
                    doc.poner_atributo(id, a, &texto());
                } else if let Some(a) = atributo_booleano(nombre) {
                    if a_booleano(&m.mundo, v) {
                        doc.poner_atributo(id, a, "");
                    } else {
                        doc.quitar_atributo(id, a);
                    }
                } else {
                    return false;
                }
            }
        }
        self.0.cambiado.set(true);
        true
    }
}

impl Anfitrion for Puente {
    fn llamar(&self, m: &mut Motor, id: u32, args: &Argumentos) -> Result<Valor, Valor> {
        metodos::llamar(m, &self.0, id, args)
    }

    fn leer(&self, m: &mut Motor, dato: u32, clave: u32) -> Option<Result<Valor, Valor>> {
        let nombre = m.mundo.cadenas.texto(clave).to_string();
        let leido = if dato & BIT_ESTILO != 0 {
            self.leer_estilo(NodoId(dato & !BIT_ESTILO), &nombre)?
        } else if dato & BIT_DATOS != 0 {
            let atributo = atributo_de_dato(&nombre)?;
            let doc = self.0.doc.borrow();
            match doc.elemento(NodoId(dato & !BIT_DATOS))?.atributo(&atributo) {
                Some(v) => Leido::Texto(String::from(v)),
                None => Leido::Indefinido,
            }
        } else if dato & BIT_CLASES != 0 {
            let doc = self.0.doc.borrow();
            let clases = doc.elemento(NodoId(dato & !BIT_CLASES))?.atributo("class").unwrap_or("");
            match nombre.as_str() {
                "length" => Leido::Entero(clases.split_ascii_whitespace().count() as i32),
                "value" => Leido::Texto(String::from(clases)),
                _ => return None,
            }
        } else {
            leer_nodo(&self.0, NodoId(dato), &nombre)?
        };
        Some(Ok(self.a_valor(m, leido)))
    }

    fn escribir(&self, m: &mut Motor, dato: u32, clave: u32, v: Valor) -> Option<Result<(), Valor>> {
        let nombre = m.mundo.cadenas.texto(clave).to_string();
        if dato & BIT_CLASES != 0 {
            return None;
        }
        if dato & BIT_DATOS != 0 {
            let atributo = atributo_de_dato(&nombre)?;
            let valor = m.texto(v);
            self.0.doc.borrow_mut().poner_atributo(NodoId(dato & !BIT_DATOS), &atributo, &valor);
            self.0.cambiado.set(true);
            return Some(Ok(()));
        }
        if dato & BIT_ESTILO != 0 {
            let id = NodoId(dato & !BIT_ESTILO);
            let valor = m.texto(v);
            let mut doc = self.0.doc.borrow_mut();
            if nombre == "cssText" {
                doc.poner_atributo(id, "style", &valor);
            } else {
                poner_estilo(&mut doc, id, &nombre_css(&nombre), &valor);
            }
            self.0.cambiado.set(true);
            return Some(Ok(()));
        }
        self.escribir_nodo(m, NodoId(dato), &nombre, v).then_some(Ok(()))
    }
}
