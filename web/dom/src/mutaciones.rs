//! `MutationObserver`: el lado de Rust. El árbol anota sus cambios mientras alguien observa
//! (`quimera_html::Cambio`). Tras el primer cambio, el puente encola una microtarea
//! (`__quimera_programar_mutaciones`), como los navegadores: esta pide con
//! `__quimera_tomar_mutaciones` los cambios que tocan a un nodo observado (o a uno de dentro, con
//! `subtree`) y los reparte entre los observadores (`observadores.rs`). Al acabar cada tarea se
//! entrega también lo que quede.

use crate::{Estado, envolver, envolver_o_nulo, lista, microtareas_sin_mutaciones};
use alloc::vec::Vec;
use quimera_html::{Cambio, NodoId};
use quimera_vm::{Motor, Valor};

/// Vueltas como mucho por tarea (si los avisos siguen cambiando lo observado).
const MAX_VUELTAS: usize = 16;

/// Desde ahora se observa `nodo` (y lo de dentro, con `subtree`): el árbol empieza a anotar.
pub(crate) fn observar(est: &Estado, nodo: NodoId, subtree: bool) {
    let mut v = est.observados.borrow_mut();
    if !v.contains(&(nodo.0, subtree)) {
        v.push((nodo.0, subtree));
    }
    est.doc.borrow_mut().anotar_cambios(true);
}

/// Si el árbol cambió con observadores, encola la microtarea que los avisa.
pub(crate) fn programar(m: &mut Motor, est: &Estado) {
    if est.mutacion_pendiente.replace(false)
        && let Err(e) = m.llamar_global("__quimera_programar_mutaciones", &[])
    {
        est.errores.borrow_mut().push(e.completo());
    }
}

/// Los cambios pendientes como arreglo de JavaScript (para `__quimera_tomar_mutaciones`).
pub(crate) fn tomar(m: &mut Motor, est: &Estado) -> Valor {
    let cambios = pendientes(est);
    a_js(m, est, &cambios)
}

/// Al acabar la tarea: entrega lo que quede; después corren las microtareas de los avisos, y se
/// repite si estos cambiaron algo observado.
pub(crate) fn entregar(m: &mut Motor, est: &Estado) {
    for _ in 0..MAX_VUELTAS {
        est.mutacion_pendiente.set(false);
        let cambios = pendientes(est);
        if cambios.is_empty() {
            return;
        }
        let registros = a_js(m, est, &cambios);
        if let Err(e) = m.llamar_global("__quimera_mutaciones", &[registros]) {
            est.errores.borrow_mut().push(e.completo());
        }
        microtareas_sin_mutaciones(m, est);
    }
}

/// Los cambios anotados que tocan a algo observado.
fn pendientes(est: &Estado) -> Vec<Cambio> {
    let observados = est.observados.borrow();
    if observados.is_empty() {
        return Vec::new();
    }
    let mut doc = est.doc.borrow_mut();
    let cambios = doc.tomar_cambios();
    let objetivo = |c: &Cambio| match c {
        Cambio::Hijos { padre, .. } => *padre,
        Cambio::Atributo { nodo, .. } | Cambio::Texto { nodo, .. } => *nodo,
    };
    cambios
        .into_iter()
        .filter(|c| {
            let t = objetivo(c);
            observados.iter().any(|&(n, sub)| n == t.0 || (sub && doc.existe(t) && doc.contiene(NodoId(n), t)))
        })
        .collect()
}

/// Cada cambio como `[tipo, objetivo, anterior, siguiente, atributo, viejo, añadidos, quitados]`
/// (tipo 0: hijos, 1: atributo, 2: texto).
fn a_js(m: &mut Motor, est: &Estado, cambios: &[Cambio]) -> Valor {
    let todos = m.crear_arreglo(&[]);
    let marca = m.apilar(todos);
    for c in cambios {
        let (tipo, objetivo, anterior, siguiente, nombre, viejo, anadidos, quitados) = match c {
            Cambio::Hijos { padre, anadidos, quitados, anterior, siguiente } => {
                (0, *padre, *anterior, *siguiente, None, None, anadidos.as_slice(), quitados.as_slice())
            }
            Cambio::Atributo { nodo, nombre, viejo } => {
                (1, *nodo, None, None, Some(nombre), viejo.as_ref(), &[][..], &[][..])
            }
            Cambio::Texto { nodo, viejo } => (2, *nodo, None, None, None, Some(viejo), &[][..], &[][..]),
        };
        let uno = m.crear_arreglo(&[]);
        let k = m.apilar(uno);
        let campos: [&dyn Fn(&mut Motor) -> Valor; 8] = [
            &|_| Valor::entero(tipo),
            &|m| envolver(m, est, objetivo),
            &|m| envolver_o_nulo(m, est, anterior),
            &|m| envolver_o_nulo(m, est, siguiente),
            &|m| nombre.map_or(Valor::NULO, |n| m.cadena(n)),
            &|m| viejo.map_or(Valor::NULO, |v| m.cadena(v)),
            &|m| lista(m, est, anadidos),
            &|m| lista(m, est, quitados),
        ];
        for f in campos {
            let v = f(m);
            let uno = m.de_pila(k);
            m.mundo.empujar(uno, v);
        }
        let uno = m.de_pila(k);
        m.soltar_pila(k);
        let todos = m.de_pila(marca);
        m.mundo.empujar(todos, uno);
    }
    let todos = m.de_pila(marca);
    m.soltar_pila(marca);
    todos
}
