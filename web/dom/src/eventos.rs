//! Eventos y temporizadores. Todo lo que guardan (las funciones de JavaScript) vive en el montón
//! de la VM, en dos arreglos de la raíz del anfitrión:
//!
//! - `eventos`: de tres en tres, `[nodo, tipo, función]` (el nodo de `window` es -1). Quitar un
//!   listener deja su función en `undefined`.
//! - `temporizadores`: de cuatro en cuatro, `[número, función, cuándo, cada]` (`cada` es -1 si
//!   no se repite). Uno que ya corrió o se canceló queda con la función en `undefined`; cuando no
//!   queda ninguno vivo, el arreglo se vacía.
//!
//! Un evento sube del nodo a sus antepasados y a `window` (todos burbujean, por ahora). En cada
//! uno se llama su propiedad `on<tipo>` (o, si no la tiene, su atributo `on<tipo>="..."`) y
//! después sus listeners.

use crate::{Estado, VENTANA, envolver, microtareas};
use alloc::format;
use alloc::vec::Vec;
use quimera_html::NodoId;
use quimera_vm::operaciones::estricto;
use quimera_vm::{Motor, Valor};

fn arreglo(m: &mut Motor, nombre: &str) -> Valor {
    let raiz = m.raiz_anfitrion();
    m.leer_nombre(raiz, nombre)
}

fn elemento(m: &mut Motor, lista: &str, i: u32) -> Valor {
    let a = arreglo(m, lista);
    m.mundo.leer_elemento(a, i).unwrap_or(Valor::INDEFINIDO)
}

fn longitud(m: &mut Motor, lista: &str) -> u32 {
    let a = arreglo(m, lista);
    m.mundo.longitud(a).unwrap_or(0)
}

/// Añade valores al final de un arreglo de la raíz (los objetos, leídos de la pila cada vez).
fn empujar(m: &mut Motor, lista: &str, valores: &[Valor]) {
    let marca = m.mundo.m.marca_raices();
    for &v in valores {
        m.apilar(v);
    }
    for i in 0..valores.len() {
        let (a, v) = (arreglo(m, lista), m.de_pila(marca + i));
        m.mundo.empujar(a, v);
    }
    m.soltar_pila(marca);
}

fn escribir(m: &mut Motor, lista: &str, i: u32, v: Valor) {
    let a = arreglo(m, lista);
    m.mundo.escribir_elemento(a, i, v);
}

fn numero_de_nodo(n: NodoId) -> Valor {
    Valor::entero(n.0 as i32)
}

/// `addEventListener(tipo, f)` (dos veces la misma función no se añade dos veces).
pub(crate) fn anadir(m: &mut Motor, nodo: NodoId, tipo: &str, f: Valor) {
    if !m.es_funcion(f) || buscar_oyente(m, nodo, tipo, f).is_some() {
        return;
    }
    let t = m.cadena(tipo);
    empujar(m, "eventos", &[numero_de_nodo(nodo), t, f]);
}

/// `removeEventListener(tipo, f)`.
pub(crate) fn quitar(m: &mut Motor, nodo: NodoId, tipo: &str, f: Valor) {
    if let Some(i) = buscar_oyente(m, nodo, tipo, f) {
        escribir(m, "eventos", i, Valor::INDEFINIDO);
    }
}

/// Dónde está la función `f` de un listener (el hueco de la función).
fn buscar_oyente(m: &mut Motor, nodo: NodoId, tipo: &str, f: Valor) -> Option<u32> {
    oyentes(m, nodo, tipo).into_iter().find(|&i| estricto(elemento(m, "eventos", i), f))
}

/// Los huecos de las funciones que escuchan `tipo` en `nodo`, en el orden en que se añadieron.
fn oyentes(m: &mut Motor, nodo: NodoId, tipo: &str) -> Vec<u32> {
    let mut r = Vec::new();
    let n = longitud(m, "eventos");
    for i in (0..n).step_by(3) {
        let (n, t, f) = (elemento(m, "eventos", i), elemento(m, "eventos", i + 1), elemento(m, "eventos", i + 2));
        if n == numero_de_nodo(nodo) && m.texto(t) == tipo && m.es_funcion(f) {
            r.push(i + 2);
        }
    }
    r
}

/// Llama a `f` con `this` = el objeto de `nodo` y el evento (en la pila, en `marca_evento`).
fn llamar(m: &mut Motor, est: &Estado, f: Valor, nodo: NodoId, marca_evento: usize) {
    let marca = m.apilar(f);
    let este = envolver(m, est, nodo);
    let (f, ev) = (m.de_pila(marca), m.de_pila(marca_evento));
    if let Err(e) = m.llamar_funcion(f, este, &[ev]) {
        est.errores.borrow_mut().push(e.mensaje);
    }
    m.soltar_pila(marca);
    microtareas(m, est);
}

/// El manejador del atributo `on<tipo>="..."` de un elemento, como función.
fn manejador_de_atributo(m: &mut Motor, est: &Estado, nodo: NodoId, tipo: &str) -> Option<Valor> {
    if nodo.0 == VENTANA {
        return None;
    }
    let codigo = {
        let doc = est.doc.borrow();
        let e = doc.elemento(nodo)?;
        alloc::string::String::from(e.atributo(&format!("on{tipo}"))?)
    };
    if let Err(e) = m.ejecutar(&format!("__quimera_manejador = function (event) {{\n{codigo}\n}};")) {
        est.errores.borrow_mut().push(e.mensaje);
        return None;
    }
    let g = m.global();
    Some(m.leer_nombre(g, "__quimera_manejador"))
}

/// Lanza `tipo` en `nodo` y lo sube hasta `window`. Devuelve si se llamó a `preventDefault()`.
pub(crate) fn despachar(m: &mut Motor, est: &Estado, nodo: NodoId, tipo: &str) -> bool {
    let mut ruta = Vec::new();
    let mut actual = Some(nodo).filter(|n| n.0 != VENTANA);
    while let Some(n) = actual {
        ruta.push(n);
        actual = est.doc.borrow().nodo(n).padre;
    }
    ruta.push(NodoId(VENTANA));
    let antes = (est.prevenido.replace(false), est.detenido.replace(false));
    let raiz = m.raiz_anfitrion();
    let proto = m.leer_nombre(raiz, "p_evento");
    let ev = m.mundo.crear_objeto(proto);
    let marca = m.apilar(ev);
    let t = m.cadena(tipo);
    m.definir(m.de_pila(marca), "type", t);
    m.definir(m.de_pila(marca), "bubbles", Valor::VERDADERO);
    m.definir(m.de_pila(marca), "defaultPrevented", Valor::FALSO);
    let objetivo = envolver(m, est, nodo);
    m.definir(m.de_pila(marca), "target", objetivo);
    for n in ruta {
        let actual = envolver(m, est, n);
        m.definir(m.de_pila(marca), "currentTarget", actual);
        // Como Chromium: el manejador `on<tipo>` (que se registra al analizar el HTML) va antes
        // que los listeners que añaden los scripts.
        let w = envolver(m, est, n);
        let propiedad = m.leer_nombre(w, &format!("on{tipo}"));
        if m.es_funcion(propiedad) {
            llamar(m, est, propiedad, n, marca);
        } else if let Some(f) = manejador_de_atributo(m, est, n, tipo) {
            llamar(m, est, f, n, marca);
        }
        for i in oyentes(m, n, tipo) {
            let f = elemento(m, "eventos", i);
            llamar(m, est, f, n, marca);
        }
        if est.detenido.get() {
            break;
        }
    }
    m.soltar_pila(marca);
    let prevenido = est.prevenido.get();
    est.prevenido.set(antes.0);
    est.detenido.set(antes.1);
    prevenido
}

/// `setTimeout` / `setInterval`: devuelve su número.
pub(crate) fn temporizador(m: &mut Motor, est: &Estado, f: Valor, ms: f64, repetir: bool) -> i32 {
    let id = est.siguiente_temporizador.get();
    est.siguiente_temporizador.set(id.wrapping_add(1));
    let cuando = Valor::numero(est.ahora.get() + ms);
    let cada = Valor::numero(if repetir { ms.max(1.0) } else { -1.0 });
    empujar(m, "temporizadores", &[Valor::entero(id), f, cuando, cada]);
    id
}

/// `clearTimeout` / `clearInterval`.
pub(crate) fn cancelar(m: &mut Motor, id: i32) {
    let n = longitud(m, "temporizadores");
    for i in (0..n).step_by(4) {
        if elemento(m, "temporizadores", i) == Valor::entero(id) {
            escribir(m, "temporizadores", i + 1, Valor::INDEFINIDO);
        }
    }
}

/// Los temporizadores vivos: (hueco, cuándo).
fn vivos(m: &mut Motor) -> Vec<(u32, f64)> {
    let n = longitud(m, "temporizadores");
    let mut r = Vec::new();
    for i in (0..n).step_by(4) {
        let f = elemento(m, "temporizadores", i + 1);
        if m.es_funcion(f) {
            r.push((i, elemento(m, "temporizadores", i + 2).como_numero().unwrap_or(0.0)));
        }
    }
    r
}

/// Cuándo toca el próximo.
pub(crate) fn proximo_temporizador(m: &mut Motor) -> Option<f64> {
    vivos(m).into_iter().map(|(_, c)| c).reduce(f64::min)
}

/// Ejecuta los que tocan a la hora `ahora`, del más antiguo al más nuevo.
pub(crate) fn correr_temporizadores(m: &mut Motor, est: &Estado, ahora: f64) {
    let mut toca: Vec<(u32, f64)> = vivos(m).into_iter().filter(|(_, c)| *c <= ahora).collect();
    toca.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (i, cuando) in toca {
        let f = elemento(m, "temporizadores", i + 1);
        if !m.es_funcion(f) {
            continue; // lo canceló otro temporizador
        }
        let cada = elemento(m, "temporizadores", i + 3).como_numero().unwrap_or(-1.0);
        if cada >= 0.0 {
            escribir(m, "temporizadores", i + 2, Valor::numero((cuando + cada).max(ahora)));
        } else {
            escribir(m, "temporizadores", i + 1, Valor::INDEFINIDO);
        }
        let marca = m.apilar(f);
        let g = m.global();
        if let Err(e) = m.llamar_funcion(m.de_pila(marca), g, &[]) {
            est.errores.borrow_mut().push(e.mensaje);
        }
        m.soltar_pila(marca);
        microtareas(m, est);
    }
    if vivos(m).is_empty() {
        let a = arreglo(m, "temporizadores");
        let largo = m.mundo.clave_length;
        m.mundo.escribir(a, largo, Valor::entero(0));
    }
}

/// Escribe en la consola de la página (`console.log`).
pub(crate) fn consola(m: &mut Motor, texto: &str) {
    let g = m.global();
    let console = m.leer_nombre(g, "console");
    let log = m.leer_nombre(console, "log");
    let t = m.cadena(texto);
    let _ = m.llamar_funcion(log, console, &[t]);
}
