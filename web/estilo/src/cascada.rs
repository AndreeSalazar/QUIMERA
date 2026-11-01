//! La cascada: para cada elemento, qué declaraciones le tocan y cuál gana.
//!
//! Orden (de menos a más fuerza): navegador normal < página normal < página `!important` <
//! navegador `!important`; dentro de cada uno, más especificidad gana, y a igualdad, la que
//! viene después. El atributo `style` gana a cualquier selector de la página.
//!
//! Las reglas se indexan por la parte derecha de su selector (id, primera clase o etiqueta),
//! como hace Stylo: cada elemento solo se prueba contra las reglas que pueden casar con él.

use crate::elemento::Nodo;
use crate::medios;
use crate::propiedades::{self, Contexto};
use crate::valores::{Display, Estilo};
use crate::{Estilos, Opciones};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::{Declaracion, Hoja, Regla, Selector, Token};
use quimera_html::{Dato, Documento, NodoId};

/// La hoja del navegador.
const NAVEGADOR: &str = include_str!("navegador.css");

/// Un selector de una regla, con lo que hace falta para ordenarlo.
struct Entrada<'h> {
    selector: &'h Selector,
    declaraciones: &'h [Declaracion],
    /// 0 navegador, 1 página.
    origen: u8,
    especificidad: (u32, u32, u32),
    orden: u32,
}

/// Las reglas de todas las hojas, indexadas.
struct Indice<'h> {
    entradas: Vec<Entrada<'h>>,
    por_id: BTreeMap<&'h str, Vec<usize>>,
    por_clase: BTreeMap<&'h str, Vec<usize>>,
    por_etiqueta: BTreeMap<&'h str, Vec<usize>>,
    resto: Vec<usize>,
}

impl<'h> Indice<'h> {
    fn nuevo(hojas: &'h [(u8, Hoja)], op: &Opciones) -> Indice<'h> {
        let mut ind = Indice {
            entradas: Vec::new(),
            por_id: BTreeMap::new(),
            por_clase: BTreeMap::new(),
            por_etiqueta: BTreeMap::new(),
            resto: Vec::new(),
        };
        for (origen, hoja) in hojas {
            let medios = hoja.medios.iter().filter(|m| medios::cumple(&m.condicion, op)).flat_map(|m| m.reglas.iter());
            for regla in hoja.reglas.iter().chain(medios) {
                ind.anadir(*origen, regla);
            }
        }
        ind
    }

    fn anadir(&mut self, origen: u8, regla: &'h Regla) {
        for selector in &regla.selectores {
            let i = self.entradas.len();
            self.entradas.push(Entrada {
                selector,
                declaraciones: &regla.declaraciones,
                origen,
                especificidad: selector.especificidad(),
                orden: i as u32,
            });
            let Some(derecha) = selector.partes.last() else { continue };
            if let Some(id) = derecha.ids.first() {
                self.por_id.entry(id.as_str()).or_default().push(i);
            } else if let Some(c) = derecha.clases.first() {
                self.por_clase.entry(c.as_str()).or_default().push(i);
            } else if let Some(t) = &derecha.etiqueta {
                self.por_etiqueta.entry(t.as_str()).or_default().push(i);
            } else {
                self.resto.push(i);
            }
        }
    }

    /// Las entradas que casan con el elemento, en el orden de la cascada.
    fn casan(&self, e: Nodo<'_>) -> Vec<&Entrada<'h>> {
        let Some(el) = e.doc.elemento(e.id) else { return Vec::new() };
        let mut candidatas: Vec<usize> = self.resto.clone();
        let mut juntar = |v: Option<&Vec<usize>>| candidatas.extend(v.into_iter().flatten());
        if let Some(id) = el.atributo("id") {
            juntar(self.por_id.get(id));
        }
        if let Some(clases) = el.atributo("class") {
            for c in clases.split_ascii_whitespace() {
                juntar(self.por_clase.get(c));
            }
        }
        juntar(self.por_etiqueta.get(el.nombre.as_str()));
        candidatas.sort_unstable();
        candidatas.dedup();
        candidatas.into_iter().map(|i| &self.entradas[i]).filter(|en| en.selector.casa(e)).collect()
    }
}

/// Las hojas del documento: la del navegador y, en orden de documento, los `<style>` y los
/// `<link rel="stylesheet">` (cuyo texto da `externa` a partir del `href`).
fn hojas(doc: &Documento, externa: &dyn Fn(&str) -> Option<String>) -> Vec<(u8, Hoja)> {
    let mut v = alloc::vec![(0, quimera_css::analizar(NAVEGADOR))];
    for id in doc.descendientes(Documento::RAIZ) {
        let Some(e) = doc.elemento(id) else { continue };
        if e.nombre == "style" {
            v.push((1, quimera_css::analizar(&doc.texto(id))));
        }
        let es_hoja =
            e.atributo("rel").is_some_and(|r| r.split_ascii_whitespace().any(|x| x.eq_ignore_ascii_case("stylesheet")));
        let alternativa = e.atributo("rel").is_some_and(|r| r.contains("alternate"));
        if e.nombre == "link"
            && es_hoja
            && !alternativa
            && let Some(texto) = e.atributo("href").and_then(externa)
        {
            v.push((1, quimera_css::analizar(&texto)));
        }
    }
    v
}

/// Calcula el estilo de todos los elementos.
pub(crate) fn calcular(doc: &Documento, op: &Opciones, externa: &dyn Fn(&str) -> Option<String>) -> Estilos {
    let hojas = hojas(doc, externa);
    let indice = Indice::nuevo(&hojas, op);
    let mut estilos: Vec<Option<Estilo>> = alloc::vec![None; doc.total()];
    let raiz = Estilo::default();
    let letra_raiz = doc
        .buscar("html")
        .map(|h| estilo_de(doc, h, &raiz, &indice, op, raiz.tam_letra).tam_letra)
        .unwrap_or(raiz.tam_letra);
    // Recorrido en orden de documento con una pila (sin recursión): (nodo, estilo del padre).
    let mut pila: Vec<(NodoId, usize)> =
        doc.nodo(Documento::RAIZ).hijos.iter().rev().map(|&h| (h, usize::MAX)).collect();
    while let Some((id, padre)) = pila.pop() {
        if !matches!(doc.nodo(id).dato, Dato::Elemento(_)) {
            continue;
        }
        let estilo_padre = estilos.get(padre).and_then(Option::as_ref).unwrap_or(&raiz);
        let e = estilo_de(doc, id, estilo_padre, &indice, op, letra_raiz);
        let sigue = e.display != Display::Ninguno;
        estilos[id.0 as usize] = Some(e);
        if sigue {
            pila.extend(doc.nodo(id).hijos.iter().rev().map(|&h| (h, id.0 as usize)));
        }
    }
    Estilos { estilos }
}

/// El estilo de un elemento a partir del de su padre.
fn estilo_de(doc: &Documento, id: NodoId, padre: &Estilo, ind: &Indice<'_>, op: &Opciones, letra_raiz: f32) -> Estilo {
    let mut entradas = ind.casan(Nodo { doc, id });
    entradas.sort_by_key(|en| (en.especificidad, en.orden));
    // (fuerza, declaración): la fuerza ordena orígenes e !important; el orden estable hace el resto.
    let en_linea =
        doc.elemento(id).and_then(|e| e.atributo("style")).map(quimera_css::declaraciones).unwrap_or_default();
    let mut todas: Vec<(u8, &Declaracion)> = Vec::new();
    for en in &entradas {
        for d in en.declaraciones {
            todas.push((fuerza(en.origen, d.importante), d));
        }
    }
    for d in &en_linea {
        todas.push((fuerza(1, d.importante), d));
    }
    todas.sort_by_key(|(f, _)| *f);
    let mut ganadoras: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    for (_, d) in todas {
        for (prop, valor) in propiedades::expandir(&d.propiedad, &d.valor) {
            if propiedades::valida(&prop, &valor) {
                ganadoras.insert(prop, valor);
            }
        }
    }
    calcular_valores(padre, &ganadoras, op, letra_raiz)
}

/// 0 navegador, 1 página, 2 página !important, 3 navegador !important.
fn fuerza(origen: u8, importante: bool) -> u8 {
    if importante { 3 - origen } else { origen }
}

fn calcular_valores(
    padre: &Estilo,
    ganadoras: &BTreeMap<String, Vec<Token>>,
    op: &Opciones,
    letra_raiz: f32,
) -> Estilo {
    let mut e = Estilo::heredado(padre);
    let mut c =
        Contexto { letra: padre.tam_letra, letra_padre: padre.tam_letra, letra_raiz, ventana: (op.ancho, op.alto) };
    // Primero la letra (las demás longitudes en `em` dependen de ella) y el color (`currentcolor`).
    if let Some(v) = ganadoras.get("font-size") {
        if let Some(t) = propiedades::tam_letra(v, &c) {
            e.tam_letra = t;
        }
        c.letra = e.tam_letra;
    }
    if let Some(v) = ganadoras.get("color") {
        propiedades::aplicar(&mut e, padre, "color", v, &c);
    }
    for (prop, v) in ganadoras {
        if prop != "font-size" && prop != "color" {
            propiedades::aplicar(&mut e, padre, prop, v, &c);
        }
    }
    e
}
