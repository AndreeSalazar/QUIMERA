//! Los métodos del DOM: funciones del anfitrión en los prototipos de nodo, elemento y documento,
//! en `classList`, `style`, los eventos y `window` (temporizadores).

use crate::eventos;
use crate::puente::{declaraciones, poner_estilo};
use crate::selectores::Selector;
use crate::{BIT_CLASES, BIT_ESTILO, Estado, VENTANA, envolver, envolver_o_nulo, lista};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use quimera_html::{Dato, Documento, NodoId};
use quimera_vm::operaciones::{a_booleano, a_numero};
use quimera_vm::{Argumentos, Motor, Valor};

const APPEND_CHILD: u32 = 1;
const REMOVE_CHILD: u32 = 2;
const INSERT_BEFORE: u32 = 3;
const REPLACE_CHILD: u32 = 4;
const CONTAINS: u32 = 5;
const HAS_CHILD_NODES: u32 = 6;
const CLONE_NODE: u32 = 7;
const REMOVE: u32 = 8;
const APPEND: u32 = 9;
const ADD_LISTENER: u32 = 10;
const REMOVE_LISTENER: u32 = 11;
const QUERY: u32 = 12;
const QUERY_ALL: u32 = 13;
const POR_ETIQUETA: u32 = 14;
const POR_CLASE: u32 = 15;
const GET_ATTRIBUTE: u32 = 20;
const SET_ATTRIBUTE: u32 = 21;
const REMOVE_ATTRIBUTE: u32 = 22;
const HAS_ATTRIBUTE: u32 = 23;
const MATCHES: u32 = 24;
const CLOSEST: u32 = 25;
const CLICK: u32 = 26;
const NADA: u32 = 27;
const POR_ID: u32 = 30;
const CREATE_ELEMENT: u32 = 31;
const CREATE_TEXT: u32 = 32;
const CREATE_COMMENT: u32 = 33;
const CREATE_FRAGMENT: u32 = 34;
const PREVENT_DEFAULT: u32 = 40;
const STOP_PROPAGATION: u32 = 41;
const CLASE_ADD: u32 = 50;
const CLASE_REMOVE: u32 = 51;
const CLASE_TOGGLE: u32 = 52;
const CLASE_CONTAINS: u32 = 53;
const ESTILO_SET: u32 = 60;
const ESTILO_GET: u32 = 61;
const ESTILO_REMOVE: u32 = 62;
const SET_TIMEOUT: u32 = 70;
const SET_INTERVAL: u32 = 71;
const CLEAR_TIMEOUT: u32 = 72;
const ANIMATION_FRAME: u32 = 73;
const ALERT: u32 = 74;
const PEDIR: u32 = 75;
const DISPATCH: u32 = 16;
/// `__quimera_color(texto)`: un color de CSS normalizado como lo da un canvas, o `null`.
const COLOR: u32 = 17;

/// Qué métodos lleva cada prototipo (`window`: el objeto global).
pub(crate) const TABLA: &[(&str, &[(&str, u32)])] = &[
    (
        "p_destino",
        &[("addEventListener", ADD_LISTENER), ("removeEventListener", REMOVE_LISTENER), ("dispatchEvent", DISPATCH)],
    ),
    (
        "p_nodo",
        &[
            ("appendChild", APPEND_CHILD),
            ("removeChild", REMOVE_CHILD),
            ("insertBefore", INSERT_BEFORE),
            ("replaceChild", REPLACE_CHILD),
            ("contains", CONTAINS),
            ("hasChildNodes", HAS_CHILD_NODES),
            ("cloneNode", CLONE_NODE),
            ("remove", REMOVE),
            ("append", APPEND),
            ("querySelector", QUERY),
            ("querySelectorAll", QUERY_ALL),
            ("getElementsByTagName", POR_ETIQUETA),
            ("getElementsByClassName", POR_CLASE),
        ],
    ),
    (
        "p_elemento",
        &[
            ("getAttribute", GET_ATTRIBUTE),
            ("setAttribute", SET_ATTRIBUTE),
            ("removeAttribute", REMOVE_ATTRIBUTE),
            ("hasAttribute", HAS_ATTRIBUTE),
            ("matches", MATCHES),
            ("closest", CLOSEST),
            ("click", CLICK),
            ("focus", NADA),
            ("blur", NADA),
        ],
    ),
    (
        "p_documento",
        &[
            ("getElementById", POR_ID),
            ("createElement", CREATE_ELEMENT),
            ("createTextNode", CREATE_TEXT),
            ("createComment", CREATE_COMMENT),
            ("createDocumentFragment", CREATE_FRAGMENT),
        ],
    ),
    ("p_evento", &[("preventDefault", PREVENT_DEFAULT), ("stopPropagation", STOP_PROPAGATION)]),
    (
        "p_clases",
        &[("add", CLASE_ADD), ("remove", CLASE_REMOVE), ("toggle", CLASE_TOGGLE), ("contains", CLASE_CONTAINS)],
    ),
    ("p_estilo", &[("setProperty", ESTILO_SET), ("getPropertyValue", ESTILO_GET), ("removeProperty", ESTILO_REMOVE)]),
    (
        "window",
        &[
            ("setTimeout", SET_TIMEOUT),
            ("setInterval", SET_INTERVAL),
            ("clearTimeout", CLEAR_TIMEOUT),
            ("clearInterval", CLEAR_TIMEOUT),
            ("requestAnimationFrame", ANIMATION_FRAME),
            ("cancelAnimationFrame", CLEAR_TIMEOUT),
            ("alert", ALERT),
            ("__quimera_pedir", PEDIR),
            ("__quimera_color", COLOR),
            ("addEventListener", ADD_LISTENER),
            ("removeEventListener", REMOVE_LISTENER),
            ("dispatchEvent", DISPATCH),
        ],
    ),
];

/// El nodo de un valor (un objeto de nodo que existe), si lo es.
fn nodo_de(m: &Motor, est: &Estado, v: Valor) -> Option<NodoId> {
    let d = m.dato_anfitrion(v).filter(|d| d & (BIT_ESTILO | BIT_CLASES | crate::BIT_DATOS) == 0)?;
    est.doc.borrow().existe(NodoId(d)).then_some(NodoId(d))
}

fn texto_arg(m: &Motor, a: &Argumentos, i: usize) -> String {
    m.texto(a.arg(m, i))
}

/// Copia un nodo (y sus descendientes si `profundo`), suelto.
fn clonar(doc: &mut Documento, id: NodoId, profundo: bool) -> NodoId {
    let nuevo = doc.crear(doc.nodo(id).dato.clone());
    if profundo {
        for h in doc.nodo(id).hijos.clone() {
            let c = clonar(doc, h, true);
            doc.insertar(nuevo, c, None);
        }
    }
    nuevo
}

/// Los elementos bajo `raiz` que cumplen `f`, en orden de documento.
fn buscar(est: &Estado, raiz: NodoId, f: impl Fn(&Documento, NodoId) -> bool) -> Vec<NodoId> {
    let doc = est.doc.borrow();
    doc.descendientes(raiz).filter(|&d| doc.elemento(d).is_some() && f(&doc, d)).collect()
}

/// Ejecuta el método `id` del anfitrión.
pub(crate) fn llamar(m: &mut Motor, est: &Estado, id: u32, a: &Argumentos) -> Result<Valor, Valor> {
    match id {
        SET_TIMEOUT..=ALERT => return ventana(m, est, id, a),
        PEDIR => return pedir(m, est, a),
        COLOR => {
            let texto = texto_arg(m, a, 0);
            return Ok(match quimera_css::color(&quimera_css::tokens(&texto)) {
                Some(c) if c.a == 255 => m.cadena(&alloc::format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
                Some(c) => {
                    let alfa = quimera_vm::matematicas::redondear(f64::from(c.a) / 255.0 * 1000.0) / 1000.0;
                    m.cadena(&alloc::format!("rgba({}, {}, {}, {alfa})", c.r, c.g, c.b))
                }
                None => Valor::NULO,
            });
        }
        DISPATCH => {
            // `objetivo.dispatchEvent(evento)`: el evento lo creó la página (`new Event(...)`).
            let nodo = if a.este(m) == m.global() { Some(NodoId(VENTANA)) } else { nodo_de(m, est, a.este(m)) };
            let Some(nodo) = nodo else { return Ok(Valor::VERDADERO) };
            let ev = a.arg(m, 0);
            if !m.mundo.es_objeto(ev) {
                return Err(m.error_de_tipo("dispatchEvent necesita un evento"));
            }
            let tipo = m.leer_nombre(ev, "type");
            let tipo = m.texto(tipo);
            let prevenido = eventos::despachar_evento(m, est, nodo, &tipo, Some(ev));
            return Ok(Valor::booleano(!prevenido));
        }
        ADD_LISTENER | REMOVE_LISTENER => {
            let nodo = if a.este(m) == m.global() { Some(NodoId(VENTANA)) } else { nodo_de(m, est, a.este(m)) };
            let Some(nodo) = nodo else { return Ok(Valor::INDEFINIDO) };
            let tipo = texto_arg(m, a, 0);
            if id == ADD_LISTENER {
                eventos::anadir(m, nodo, &tipo, a.arg(m, 1));
            } else {
                eventos::quitar(m, nodo, &tipo, a.arg(m, 1));
            }
            return Ok(Valor::INDEFINIDO);
        }
        PREVENT_DEFAULT | STOP_PROPAGATION => {
            if id == PREVENT_DEFAULT {
                est.prevenido.set(true);
                m.definir(a.este(m), "defaultPrevented", Valor::VERDADERO);
            } else {
                est.detenido.set(true);
            }
            return Ok(Valor::INDEFINIDO);
        }
        CLASE_ADD..=CLASE_CONTAINS => return clases(m, est, id, a),
        ESTILO_SET..=ESTILO_REMOVE => return estilo(m, est, id, a),
        _ => {}
    }
    let Some(este) = nodo_de(m, est, a.este(m)) else {
        return Err(m.error_de_tipo("el método necesita un nodo como this"));
    };
    match id {
        APPEND_CHILD..=APPEND => arbol(m, est, id, este, a),
        QUERY..=POR_CLASE | MATCHES | CLOSEST | POR_ID => consultas(m, est, id, este, a),
        _ => elemento(m, est, id, este, a),
    }
}

/// Mover, quitar y copiar nodos.
fn arbol(m: &mut Motor, est: &Estado, id: u32, este: NodoId, a: &Argumentos) -> Result<Valor, Valor> {
    let arg = |i: usize| nodo_de(m, est, a.arg(m, i));
    let (n0, n1) = (arg(0), arg(1));
    // Para `append`: cada argumento, un nodo o un texto (antes de tomar el árbol para cambiarlo).
    let piezas: Vec<Result<NodoId, String>> =
        if id == APPEND { (0..a.n).map(|i| arg(i).ok_or_else(|| texto_arg(m, a, i))).collect() } else { Vec::new() };
    let mut doc = est.doc.borrow_mut();
    let ok = match (id, n0) {
        (APPEND_CHILD, Some(n)) => doc.insertar(este, n, None),
        (INSERT_BEFORE, Some(n)) => doc.insertar(este, n, n1),
        (REMOVE_CHILD, Some(n)) if doc.nodo(n).padre == Some(este) => {
            doc.quitar(n);
            true
        }
        (REPLACE_CHILD, Some(n)) if n1.is_some_and(|v| doc.nodo(v).padre == Some(este)) => {
            let viejo = n1.unwrap_or(n);
            let ok = doc.insertar(este, n, Some(viejo));
            doc.quitar(viejo);
            drop(doc);
            est.cambiado.set(true);
            return if ok { Ok(a.arg(m, 1)) } else { Err(m.error_de_tipo("HierarchyRequestError")) };
        }
        (CONTAINS, n) => return Ok(Valor::booleano(n.is_some_and(|n| doc.contiene(este, n)))),
        (HAS_CHILD_NODES, _) => return Ok(Valor::booleano(!doc.nodo(este).hijos.is_empty())),
        (CLONE_NODE, _) => {
            let profundo = a_booleano(&m.mundo, a.arg(m, 0));
            let c = clonar(&mut doc, este, profundo);
            drop(doc);
            return Ok(envolver(m, est, c));
        }
        (REMOVE, _) => {
            doc.quitar(este);
            est.cambiado.set(true);
            return Ok(Valor::INDEFINIDO);
        }
        (APPEND, _) => {
            for p in piezas {
                let hijo = p.unwrap_or_else(|t| doc.crear(Dato::Texto(t)));
                doc.insertar(este, hijo, None);
            }
            est.cambiado.set(true);
            return Ok(Valor::INDEFINIDO);
        }
        _ => false,
    };
    drop(doc);
    if !ok {
        return Err(m.error_de_tipo("HierarchyRequestError: ese nodo no puede ir ahí"));
    }
    est.cambiado.set(true);
    if let Some(n) = n0 {
        crate::red::scripts_insertados(m, est, n);
    }
    Ok(a.arg(m, 0))
}

/// `querySelector`, `getElementsBy...`, `matches`, `closest`, `getElementById`.
fn consultas(m: &mut Motor, est: &Estado, id: u32, este: NodoId, a: &Argumentos) -> Result<Valor, Valor> {
    let t = texto_arg(m, a, 0);
    let selector = || Selector::analizar(&t);
    let encontrados = match id {
        POR_ID => buscar(est, Documento::RAIZ, |d, n| d.elemento(n).and_then(|e| e.atributo("id")) == Some(&t)),
        POR_ETIQUETA => {
            let t = t.to_ascii_lowercase();
            buscar(est, este, |d, n| t == "*" || d.elemento(n).is_some_and(|e| e.nombre == t))
        }
        POR_CLASE => {
            let clases: Vec<&str> = t.split_ascii_whitespace().collect();
            buscar(est, este, |d, n| d.elemento(n).is_some_and(|e| clases.iter().all(|c| e.tiene_clase(c))))
        }
        MATCHES => {
            let doc = est.doc.borrow();
            return Ok(Valor::booleano(selector().is_some_and(|s| s.coincide(&doc, este))));
        }
        CLOSEST => {
            let doc = est.doc.borrow();
            let s = selector();
            let mut actual = Some(este);
            while let Some(x) = actual.filter(|&x| !s.as_ref().is_some_and(|s| s.coincide(&doc, x))) {
                actual = doc.nodo(x).padre;
            }
            drop(doc);
            return Ok(envolver_o_nulo(m, est, actual.filter(|_| s.is_some())));
        }
        _ => match selector() {
            Some(s) => buscar(est, este, |d, n| s.coincide(d, n)),
            None => Vec::new(),
        },
    };
    Ok(match id {
        POR_ID | QUERY => envolver_o_nulo(m, est, encontrados.first().copied()),
        _ => lista(m, est, &encontrados),
    })
}

/// Atributos, `click()`, y `createElement` y compañía.
fn elemento(m: &mut Motor, est: &Estado, id: u32, este: NodoId, a: &Argumentos) -> Result<Valor, Valor> {
    let (t0, t1) = (texto_arg(m, a, 0), texto_arg(m, a, 1));
    let mut doc = est.doc.borrow_mut();
    let nuevo = match id {
        GET_ATTRIBUTE => {
            let v = doc.elemento(este).and_then(|e| e.atributo(&t0.to_ascii_lowercase())).map(String::from);
            drop(doc);
            return Ok(v.map_or(Valor::NULO, |v| m.cadena(&v)));
        }
        HAS_ATTRIBUTE => {
            return Ok(Valor::booleano(
                doc.elemento(este).is_some_and(|e| e.atributo(&t0.to_ascii_lowercase()).is_some()),
            ));
        }
        SET_ATTRIBUTE | REMOVE_ATTRIBUTE => {
            if id == SET_ATTRIBUTE {
                doc.poner_atributo(este, &t0, &t1);
            } else {
                doc.quitar_atributo(este, &t0);
            }
            est.cambiado.set(true);
            return Ok(Valor::INDEFINIDO);
        }
        CLICK => {
            drop(doc);
            eventos::despachar(m, est, este, "click");
            return Ok(Valor::INDEFINIDO);
        }
        CREATE_ELEMENT => doc.crear_elemento(&t0),
        CREATE_TEXT => doc.crear(Dato::Texto(t0)),
        CREATE_COMMENT => doc.crear(Dato::Comentario(t0)),
        CREATE_FRAGMENT => doc.crear_elemento(quimera_html::FRAGMENTO),
        _ => return Ok(Valor::INDEFINIDO),
    };
    drop(doc);
    Ok(envolver(m, est, nuevo))
}

/// `classList.add/remove/toggle/contains`.
fn clases(m: &mut Motor, est: &Estado, id: u32, a: &Argumentos) -> Result<Valor, Valor> {
    let Some(nodo) = m.dato_anfitrion(a.este(m)).filter(|d| d & BIT_CLASES != 0).map(|d| NodoId(d & !BIT_CLASES))
    else {
        return Ok(Valor::INDEFINIDO);
    };
    let pedidas: Vec<String> = (0..a.n).map(|i| texto_arg(m, a, i)).collect();
    let mut doc = est.doc.borrow_mut();
    let mut actuales: Vec<String> = doc
        .elemento(nodo)
        .and_then(|e| e.atributo("class"))
        .unwrap_or("")
        .split_ascii_whitespace()
        .map(String::from)
        .collect();
    let primera = pedidas.first().cloned().unwrap_or_default();
    let tiene = actuales.contains(&primera);
    let resultado = match id {
        CLASE_CONTAINS => return Ok(Valor::booleano(tiene)),
        CLASE_ADD => {
            for c in pedidas {
                if !actuales.contains(&c) {
                    actuales.push(c);
                }
            }
            Valor::INDEFINIDO
        }
        CLASE_REMOVE => {
            actuales.retain(|c| !pedidas.contains(c));
            Valor::INDEFINIDO
        }
        _ => {
            let forzar = (a.n > 1).then(|| a_booleano(&m.mundo, a.arg(m, 1)));
            let poner = forzar.unwrap_or(!tiene);
            actuales.retain(|c| *c != primera);
            if poner {
                actuales.push(primera);
            }
            Valor::booleano(poner)
        }
    };
    doc.poner_atributo(nodo, "class", &actuales.join(" "));
    est.cambiado.set(true);
    Ok(resultado)
}

/// `style.setProperty/getPropertyValue/removeProperty`.
fn estilo(m: &mut Motor, est: &Estado, id: u32, a: &Argumentos) -> Result<Valor, Valor> {
    let Some(nodo) = m.dato_anfitrion(a.este(m)).filter(|d| d & BIT_ESTILO != 0).map(|d| NodoId(d & !BIT_ESTILO))
    else {
        return Ok(Valor::INDEFINIDO);
    };
    let (prop, valor) = (texto_arg(m, a, 0).to_ascii_lowercase(), texto_arg(m, a, 1));
    let mut doc = est.doc.borrow_mut();
    if id == ESTILO_GET {
        let estilo = doc.elemento(nodo).and_then(|e| e.atributo("style")).unwrap_or("").to_string();
        drop(doc);
        let v = declaraciones(&estilo).into_iter().find(|(k, _)| *k == prop).map(|(_, v)| v).unwrap_or_default();
        return Ok(m.cadena(&v));
    }
    poner_estilo(&mut doc, nodo, &prop, if id == ESTILO_SET { &valor } else { "" });
    est.cambiado.set(true);
    Ok(Valor::INDEFINIDO)
}

/// `__quimera_pedir(url, método, cuerpo, cabeceras, credenciales)`: apunta la petición y
/// devuelve su número (ver `red.rs`).
fn pedir(m: &mut Motor, est: &Estado, a: &Argumentos) -> Result<Valor, Valor> {
    let cuerpo = a.arg(m, 2);
    let p = crate::red::PeticionJs {
        id: 0,
        url: texto_arg(m, a, 0),
        metodo: texto_arg(m, a, 1),
        cuerpo: (cuerpo != Valor::NULO && cuerpo != Valor::INDEFINIDO).then(|| m.texto(cuerpo)),
        cabeceras: crate::red::pares_de(m, a.arg(m, 3)),
        credenciales: texto_arg(m, a, 4),
        es_script: false,
    };
    Ok(Valor::entero(crate::red::pedir(est, p) as i32))
}

/// Temporizadores y `alert`.
fn ventana(m: &mut Motor, est: &Estado, id: u32, a: &Argumentos) -> Result<Valor, Valor> {
    match id {
        SET_TIMEOUT | SET_INTERVAL | ANIMATION_FRAME => {
            let f = a.arg(m, 0);
            if !m.es_funcion(f) {
                return Ok(Valor::entero(0)); // con texto (`setTimeout("codigo")`) todavía no
            }
            let ms = if id == ANIMATION_FRAME { 16.0 } else { a_numero(&m.mundo, a.arg(m, 1)) };
            let ms = if ms.is_finite() { ms.max(0.0) } else { 0.0 };
            Ok(Valor::entero(eventos::temporizador(m, est, f, ms, id == SET_INTERVAL)))
        }
        CLEAR_TIMEOUT => {
            if let Some(n) = a.arg(m, 0).como_numero() {
                eventos::cancelar(m, n as i32);
            }
            Ok(Valor::INDEFINIDO)
        }
        _ => {
            let texto = alloc::format!("[alert] {}", texto_arg(m, a, 0));
            eventos::consola(m, &texto);
            Ok(Valor::INDEFINIDO)
        }
    }
}
