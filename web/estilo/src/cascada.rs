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
/// Lo que cambia en las páginas sin `<!doctype html>` (modo de compatibilidad).
const COMPATIBILIDAD: &str = include_str!("compatibilidad.css");

/// Si la página va en modo de compatibilidad: sin `<!doctype>`, o con uno que no es `html`.
/// (Los doctypes antiguos con identificador público también lo activan en los navegadores; el
/// árbol aún no guarda ese identificador.)
fn en_compatibilidad(doc: &Documento) -> bool {
    !doc.nodo(Documento::RAIZ)
        .hijos
        .iter()
        .any(|&h| matches!(&doc.nodo(h).dato, Dato::Doctype(n) if n.eq_ignore_ascii_case("html")))
}

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
    if en_compatibilidad(doc) {
        v.push((0, quimera_css::analizar(COMPATIBILIDAD)));
    }
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
    let pistas = pistas_de_atributos(doc, id);
    let mut todas: Vec<(u8, &Declaracion)> = Vec::new();
    // Las pistas de los atributos van primero entre las de la página: cualquier regla les gana.
    for d in &pistas {
        todas.push((1, d));
    }
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

/// Las «pistas de presentación» del estándar de HTML: atributos que cuentan como declaraciones
/// de la página con la menor fuerza (cualquier regla de CSS les gana). Así `img { height: auto }`
/// gana al atributo `height` y la imagen no se deforma.
///
/// - `width` y `height` de `<img>` y otros reemplazados, de las tablas y sus celdas.
/// - Tablas antiguas: `border`, `cellspacing`, `cellpadding` (de la tabla, para sus celdas),
///   `bgcolor`, `align`, `valign` y `nowrap`.
fn pistas_de_atributos(doc: &Documento, id: NodoId) -> Vec<Declaracion> {
    let Some(e) = doc.elemento(id) else { return Vec::new() };
    let nombre = e.nombre.as_str();
    let mut css = String::new();
    if matches!(
        nombre,
        "img" | "video" | "canvas" | "iframe" | "embed" | "object" | "table" | "td" | "th" | "col" | "tr"
    ) {
        for prop in ["width", "height"] {
            if let Some(m) = e.atributo(prop).and_then(longitud_html) {
                css.push_str(&alloc::format!("{prop}:{m};"));
            }
        }
    }
    if matches!(nombre, "table" | "tr" | "td" | "th" | "thead" | "tbody" | "tfoot" | "body")
        && let Some(c) = e.atributo("bgcolor").and_then(color_html)
    {
        css.push_str(&alloc::format!("background-color:{c};"));
    }
    match nombre {
        "table" => {
            if let Some(b) = e.atributo("border").map(|b| b.trim().parse::<u32>().unwrap_or(1)) {
                css.push_str(&alloc::format!("border:{b}px outset gray;"));
            }
            if let Some(m) = e.atributo("cellspacing").and_then(longitud_html) {
                css.push_str(&alloc::format!("border-spacing:{m};"));
            }
            if e.atributo("align").is_some_and(|a| a.eq_ignore_ascii_case("center")) {
                css.push_str("margin-left:auto;margin-right:auto;");
            }
        }
        "td" | "th" | "tr" | "thead" | "tbody" | "tfoot" => {
            if let Some(a) = e.atributo("align").and_then(alinear_html) {
                css.push_str(&alloc::format!("text-align:{a};"));
            }
            if let Some(v) = e
                .atributo("valign")
                .map(str::to_ascii_lowercase)
                .filter(|v| matches!(v.as_str(), "top" | "middle" | "bottom" | "baseline"))
            {
                css.push_str(&alloc::format!("vertical-align:{v};"));
            }
            if matches!(nombre, "td" | "th") {
                if e.atributo("nowrap").is_some() {
                    css.push_str("white-space:nowrap;");
                }
                if let Some(t) = tabla_de(doc, id).and_then(|t| doc.elemento(t)) {
                    if let Some(p) = t.atributo("cellpadding").and_then(longitud_html) {
                        css.push_str(&alloc::format!("padding:{p};"));
                    }
                    if t.atributo("border").is_some_and(|b| b.trim().parse::<u32>().map_or(true, |n| n > 0)) {
                        css.push_str("border:1px inset gray;");
                    }
                }
            }
        }
        "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "caption" => {
            if let Some(a) = e.atributo("align").and_then(alinear_html) {
                css.push_str(&alloc::format!("text-align:{a};"));
            }
        }
        _ => {}
    }
    if css.is_empty() { Vec::new() } else { quimera_css::declaraciones(&css) }
}

/// `align` de HTML como `text-align`: `center` centra también los bloques de dentro (como en
/// Chromium, `-webkit-center`).
fn alinear_html(a: &str) -> Option<&'static str> {
    Some(match a.trim().to_ascii_lowercase().as_str() {
        "left" => "left",
        "right" => "right",
        "center" | "middle" => "-webkit-center",
        "justify" => "justify",
        _ => return None,
    })
}

/// Un número de un atributo de HTML (`100`, `50%`, `10px`) como longitud de CSS.
fn longitud_html(t: &str) -> Option<String> {
    let t = t.trim();
    let (numero, por_ciento) = match t.strip_suffix('%') {
        Some(n) => (n, true),
        None => (t.trim_end_matches("px"), false),
    };
    let n = numero.trim().parse::<f64>().ok().filter(|n| *n >= 0.0)?;
    Some(if por_ciento { alloc::format!("{n}%") } else { alloc::format!("{n}px") })
}

/// Un color de un atributo de HTML: un nombre o un hexadecimal (con o sin `#`).
fn color_html(t: &str) -> Option<String> {
    let t = t.trim();
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '#') {
        return None;
    }
    let hex = t.trim_start_matches('#');
    let es_hex = matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit());
    Some(if es_hex { alloc::format!("#{hex}") } else { String::from(t) })
}

/// La tabla a la que pertenece una celda (subiendo por sus padres).
fn tabla_de(doc: &Documento, id: NodoId) -> Option<NodoId> {
    let mut p = doc.nodo(id).padre;
    while let Some(n) = p {
        if doc.elemento(n).is_some_and(|e| e.nombre == "table") {
            return Some(n);
        }
        p = doc.nodo(n).padre;
    }
    None
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
