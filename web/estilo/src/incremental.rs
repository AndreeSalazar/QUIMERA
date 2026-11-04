//! Estilo incremental: tras un cambio de JavaScript, se recalcula solo lo tocado y lo que puede
//! depender de ello. Lo demás reutiliza el estilo del cálculo anterior.
//!
//! El documento apunta sus toques (`quimera_html::Toques`). De cada toque sale un conjunto de
//! **raíces**: elementos que se recalculan con todo lo que llevan dentro (por la herencia).
//!
//! - **Atributo** de `e` (`class`, `id`, `style`...): `e` y sus hermanos de después (`e + f`, `e ~ f`).
//! - **Hijos** de `p`: `p` (`:empty`) y todos sus hijos (`:nth-child`, `:first-child`, `+`, `~`).
//! - **Texto** de un nodo: su padre (`:empty`).
//! - **`:has()`**: los antepasados del nodo tocado que pueden ser el sujeto de un `:has()` de las
//!   hojas (por su etiqueta, id y clases). Si algún `:has()` mira a los hermanos (`+`, `~`), también
//!   los hermanos de antes de cada uno.
//!
//! Se rehace todo si cambian las hojas o la pantalla (otro índice), si cambia la letra de `<html>`
//! (los `rem`), si hubo demasiados toques o si no hay cálculo anterior. Los pseudoelementos
//! (`::before`, nodos sintéticos con números nuevos en cada cálculo) se recalculan siempre.

use crate::cascada::{Indice, estilo_de, hojas};
use crate::valores::{Display, Estilo};
use crate::{CacheHojas, Estilos, Opciones};
use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::{Combinador, Pseudo, Selector};
use quimera_html::{Dato, Documento, Elemento, NodoId, Toque, Toques};

/// Lo que piden los `:has()` de las hojas: el sujeto de cada uno (etiqueta, ids y clases de su
/// parte del selector) y si alguno mira a los hermanos.
#[derive(Debug, Default)]
pub(crate) struct ConHas {
    sujetos: Vec<(Option<String>, Vec<String>, Vec<String>)>,
    hermanos: bool,
}

impl ConHas {
    /// Apunta los `:has()` de un selector (también dentro de `:is()` y `:not()`).
    pub(crate) fn anotar(&mut self, s: &Selector) {
        for parte in &s.partes {
            for p in &parte.pseudos {
                match p {
                    Pseudo::Tiene(relativos) => {
                        self.hermanos |= relativos
                            .iter()
                            .any(|r| matches!(r.combinador, Combinador::Adyacente | Combinador::Hermano));
                        let sujeto = (parte.etiqueta.clone(), parte.ids.clone(), parte.clases.clone());
                        if !self.sujetos.contains(&sujeto) {
                            self.sujetos.push(sujeto);
                        }
                    }
                    Pseudo::No(v) | Pseudo::Alguno(v, _) => v.iter().for_each(|x| self.anotar(x)),
                    _ => {}
                }
            }
        }
    }

    /// Si `el` puede ser el sujeto de algún `:has()`.
    fn puede(&self, el: &Elemento) -> bool {
        self.sujetos.iter().any(|(etiqueta, ids, clases)| {
            etiqueta.as_ref().is_none_or(|t| *t == el.nombre)
                && ids.iter().all(|i| el.atributo("id") == Some(i.as_str()))
                && clases.iter().all(|c| el.tiene_clase(c))
        })
    }
}

/// El cálculo anterior, para el siguiente.
#[derive(Default)]
pub struct Memoria {
    /// El estilo de cada nodo original (sin los pseudoelementos), antes de la opacidad.
    crudos: Vec<Option<Estilo>>,
    indice: Option<Rc<Indice>>,
    letra_raiz: f32,
    /// Del último cálculo: (elementos recalculados, elementos con estilo, si se rehízo entero).
    pub ultimo: (usize, usize, bool),
}

/// Los elementos que hay que recalcular con todo lo de dentro, a partir de los toques.
fn raices(doc: &Documento, toques: &Toques, tiene: &ConHas) -> BTreeSet<NodoId> {
    let mut r = BTreeSet::new();
    let valido = |id: NodoId| doc.existe(id);
    for t in toques.lista.iter().copied() {
        let tocado = match t {
            Toque::Atributo(e) if valido(e) => {
                r.insert(e);
                if let Some(p) = doc.nodo(e).padre {
                    r.extend(doc.nodo(p).hijos.iter().copied().skip_while(|&h| h != e));
                }
                e
            }
            Toque::Hijos(p) if valido(p) => {
                r.insert(p);
                r.extend(doc.nodo(p).hijos.iter().copied());
                p
            }
            Toque::Texto(x) if valido(x) => match doc.nodo(x).padre {
                Some(p) => {
                    r.insert(p);
                    p
                }
                None => continue,
            },
            _ => continue,
        };
        if !tiene.sujetos.is_empty() {
            por_has(doc, tocado, tiene, &mut r);
        }
    }
    r
}

/// Los antepasados de `tocado` (y él) que pueden ser sujeto de un `:has()`, y sus hermanos de antes
/// si algún `:has()` mira a los hermanos.
fn por_has(doc: &Documento, tocado: NodoId, tiene: &ConHas, r: &mut BTreeSet<NodoId>) {
    let mut actual = Some(tocado);
    while let Some(c) = actual {
        if doc.elemento(c).is_some_and(|el| tiene.puede(el)) {
            r.insert(c);
        }
        if tiene.hermanos
            && let Some(p) = doc.nodo(c).padre
        {
            for &h in doc.nodo(p).hijos.iter().take_while(|&&h| h != c) {
                if doc.elemento(h).is_some_and(|el| tiene.puede(el)) {
                    r.insert(h);
                }
            }
        }
        actual = doc.nodo(c).padre;
    }
}

/// Como `cascada::calcular`, reutilizando de `memoria` lo que los toques no alcanzan. `originales`
/// son los nodos del documento antes de añadir los pseudoelementos.
pub fn calcular(
    doc: &Documento,
    op: &Opciones,
    externa: &dyn Fn(&str) -> Option<String>,
    cache: &CacheHojas,
    memoria: &mut Memoria,
    originales: usize,
    toques: &Toques,
) -> Estilos {
    let indice = cache.indice(hojas(doc, externa, cache), op, false);
    let raiz = Estilo::default();
    let letra_raiz = doc
        .buscar("html")
        .map(|h| estilo_de(doc, h, &raiz, &indice, op, raiz.tam_letra).tam_letra)
        .unwrap_or(raiz.tam_letra);
    let mismo = memoria.indice.as_ref().is_some_and(|i| Rc::ptr_eq(i, &indice))
        && !toques.todo
        && memoria.letra_raiz.to_bits() == letra_raiz.to_bits();
    let raices = mismo.then(|| raices(doc, toques, &indice.tiene));
    let mut estilos: Vec<Option<Estilo>> = alloc::vec![None; doc.total()];
    let mut recalculados = 0;
    // (nodo, estilo del padre, si se recalcula todo lo de dentro)
    let mut pila: Vec<(NodoId, usize, bool)> =
        doc.nodo(Documento::RAIZ).hijos.iter().rev().map(|&h| (h, usize::MAX, false)).collect();
    while let Some((id, padre, forzado)) = pila.pop() {
        if !matches!(doc.nodo(id).dato, Dato::Elemento(_)) {
            continue;
        }
        let i = id.0 as usize;
        let previo = memoria.crudos.get(i).and_then(Option::as_ref).filter(|_| i < originales);
        let recalcular = forzado || previo.is_none() || raices.as_ref().is_none_or(|r| r.contains(&id));
        let e = match previo {
            Some(p) if !recalcular => p.clone(),
            _ => {
                recalculados += 1;
                let estilo_padre = estilos.get(padre).and_then(Option::as_ref).unwrap_or(&raiz);
                estilo_de(doc, id, estilo_padre, &indice, op, letra_raiz)
            }
        };
        let sigue = e.display != Display::Ninguno;
        estilos[i] = Some(e);
        if sigue {
            pila.extend(doc.nodo(id).hijos.iter().rev().map(|&h| (h, i, recalcular)));
        }
    }
    let con_estilo = estilos.iter().flatten().count();
    let mut finales = estilos.clone();
    estilos.truncate(originales);
    *memoria =
        Memoria { crudos: estilos, indice: Some(indice), letra_raiz, ultimo: (recalculados, con_estilo, !mismo) };
    finales.iter_mut().flatten().filter(|e| e.opacidad < 1.0).for_each(crate::cascada::aplicar_opacidad);
    Estilos { estilos: finales }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const PAGINA: &str = "<!doctype html><style>
        .a { color: red } .a .b { margin-left: 3px } li:first-child { font-size: 20px }
        .x + .y { color: blue } .c:has(> .marca) { background: #00f } p:empty { height: 7px }
        div[data-s=si] span { font-weight: bold }
    </style><body><div id=d><span>uno</span></div><ul><li>a</li><li>b</li></ul>
    <p class=x>x</p><p class=y>y</p><section class=c><i>sin</i></section><p id=vacio>t</p>";

    fn comprobar(doc: &Documento, cache: &CacheHojas, m: &mut Memoria, toques: &Toques) {
        let op = Opciones { ancho: 800.0, alto: 600.0 };
        let inc = calcular(doc, &op, &|_| None, cache, m, doc.total(), toques);
        let entero = crate::cascada::calcular(doc, &op, &|_| None, &CacheHojas::default());
        for id in 0..doc.total() as u32 {
            assert_eq!(inc.de(NodoId(id)), entero.de(NodoId(id)), "nodo {id} distinto del cálculo entero ({toques:?})");
        }
    }

    #[test]
    fn da_lo_mismo_que_el_calculo_entero_tras_cada_cambio() {
        let mut doc = quimera_html::analizar(PAGINA);
        let (cache, mut m) = (CacheHojas::default(), Memoria::default());
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        assert!(m.ultimo.2, "el primero, entero");
        let id = |doc: &Documento, s: &str| {
            doc.descendientes(Documento::RAIZ)
                .find(|&n| doc.elemento(n).is_some_and(|e| e.atributo("id") == Some(s) || e.nombre == s))
                .unwrap()
        };
        // Una clase: .a y lo de dentro (.a .b).
        let div = id(&doc, "d");
        doc.poner_atributo(div, "class", "a");
        let span = doc.nodo(div).hijos[0];
        doc.poner_atributo(span, "class", "b");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        assert!(!m.ultimo.2 && m.ultimo.0 < m.ultimo.1, "solo lo tocado: {:?}", m.ultimo);
        // Un hijo nuevo al principio: el :first-child cambia de dueño.
        let ul = id(&doc, "ul");
        let nuevo = doc.crear_elemento("li");
        let primero = doc.nodo(ul).hijos.first().copied();
        doc.insertar(ul, nuevo, primero);
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        // Un hermano: .x + .y deja de casar al quitar la clase x.
        let x =
            doc.descendientes(Documento::RAIZ).find(|&n| doc.elemento(n).is_some_and(|e| e.tiene_clase("x"))).unwrap();
        doc.quitar_atributo(x, "class");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        // :has(): marcar al hijo cambia el fondo del padre.
        let i = id(&doc, "i");
        doc.poner_atributo(i, "class", "marca");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        // :empty: vaciar el texto.
        let p = id(&doc, "vacio");
        doc.vaciar(p);
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        // Un atributo en un antepasado: div[data-s=si] span.
        doc.poner_atributo(div, "data-s", "si");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        // Sin cambios: nada que recalcular salvo <html>, que sale entero de la memoria.
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        assert_eq!(m.ultimo.0, 0, "sin toques no se recalcula nada");
    }
}
