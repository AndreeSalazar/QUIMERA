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
use quimera_css::{Atributo, Combinador, Compuesto, Pseudo, Selector};
use quimera_html::{Dato, Documento, Elemento, NodoId, Toque, Toques, huella_atributo};

/// Lo que pide un elemento para poder ser el sujeto de un `:has()`: lo que dice la parte del
/// selector que lo lleva (etiqueta, ids, clases y atributos que tiene que tener).
#[derive(Debug, Clone, PartialEq, Default)]
struct Sujeto {
    etiqueta: Option<String>,
    ids: Vec<String>,
    clases: Vec<String>,
    atributos: Vec<String>,
}

impl Sujeto {
    fn de(parte: &Compuesto) -> Sujeto {
        let raiz = parte.pseudos.contains(&Pseudo::Raiz);
        Sujeto {
            etiqueta: parte.etiqueta.clone().or_else(|| raiz.then(|| String::from("html"))),
            ids: parte.ids.clone(),
            clases: parte.clases.clone(),
            atributos: parte.atributos.iter().map(nombre_de_atributo).map(String::from).collect(),
        }
    }

    fn admite(&self, el: &Elemento) -> bool {
        self.etiqueta.as_ref().is_none_or(|t| *t == el.nombre)
            && self.ids.iter().all(|i| el.atributo("id") == Some(i.as_str()))
            && self.clases.iter().all(|c| el.tiene_clase(c))
            && self.atributos.iter().all(|a| el.atributo(a).is_some())
    }
}

fn nombre_de_atributo(a: &Atributo) -> &str {
    match a {
        Atributo::Existe(n)
        | Atributo::Igual(n, _)
        | Atributo::Palabra(n, _)
        | Atributo::Prefijo(n, _)
        | Atributo::Sufijo(n, _)
        | Atributo::Contiene(n, _)
        | Atributo::Guion(n, _) => n,
    }
}

/// Lo que el estilo incremental necesita saber de las reglas: el sujeto de cada `:has()`, si
/// alguno mira a los hermanos y qué atributos miran los selectores (por la huella del nombre).
#[derive(Debug, Default)]
pub(crate) struct ConHas {
    sujetos: Vec<Sujeto>,
    hermanos: bool,
    atributos: BTreeSet<u32>,
}

/// Atributos que cambian el estilo aunque ningún selector los nombre: el `style`, las pistas de
/// presentación (`width`, `bgcolor`...), los enlaces (`:link`) y las hojas (`rel`, `media`).
const SIEMPRE: &[&str] = &[
    "style",
    "class",
    "id",
    "href",
    "width",
    "height",
    "bgcolor",
    "border",
    "cellspacing",
    "cellpadding",
    "align",
    "valign",
    "nowrap",
    "rel",
    "media",
    "type",
    "lang",
    "dir",
];

impl ConHas {
    /// Apunta los `:has()` de un selector (también dentro de `:is()` y `:not()`) y los atributos
    /// que mira.
    pub(crate) fn anotar(&mut self, s: &Selector) {
        self.nombres(s);
        for parte in &s.partes {
            self.anotar_parte(parte, &Sujeto::de(parte));
        }
    }

    /// Los atributos que mira un selector, también los de dentro de `:not()`, `:is()` y `:has()`.
    fn nombres(&mut self, s: &Selector) {
        for parte in &s.partes {
            self.atributos.extend(parte.atributos.iter().map(|a| huella_atributo(nombre_de_atributo(a))));
            if !parte.clases.is_empty() {
                self.atributos.insert(huella_atributo("class"));
            }
            if !parte.ids.is_empty() {
                self.atributos.insert(huella_atributo("id"));
            }
            for p in &parte.pseudos {
                match p {
                    Pseudo::No(v) | Pseudo::Alguno(v, _) => v.iter().for_each(|x| self.nombres(x)),
                    Pseudo::Tiene(r) => r.iter().for_each(|x| self.nombres(&x.selector)),
                    _ => {}
                }
            }
        }
    }

    /// Si cambiar el atributo de huella `h` puede cambiar qué reglas casan.
    fn mirado(&self, h: u32) -> bool {
        self.atributos.contains(&h)
    }

    /// Los `:has()` de una parte. `sujeto`: el elemento al que se aplica (en un `:not(:has(…))`
    /// o un `:is(…:has(…))` es la parte de fuera, no la de dentro).
    fn anotar_parte(&mut self, parte: &Compuesto, sujeto: &Sujeto) {
        for p in &parte.pseudos {
            match p {
                Pseudo::Tiene(relativos) => {
                    self.hermanos |=
                        relativos.iter().any(|r| matches!(r.combinador, Combinador::Adyacente | Combinador::Hermano));
                    if !self.sujetos.contains(sujeto) {
                        self.sujetos.push(sujeto.clone());
                    }
                }
                Pseudo::No(v) | Pseudo::Alguno(v, _) => {
                    for s in v {
                        let ultima = s.partes.len().saturating_sub(1);
                        for (i, dentro) in s.partes.iter().enumerate() {
                            // En la última parte, el sujeto sigue siendo el de fuera; en las demás, el
                            // `:has()` es de un antepasado o hermano: su propia parte.
                            if i == ultima {
                                self.anotar_parte(dentro, sujeto)
                            } else {
                                self.anotar_parte(dentro, &Sujeto::de(dentro))
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Si `el` puede ser el sujeto de algún `:has()`.
    fn puede(&self, el: &Elemento) -> bool {
        self.sujetos.iter().any(|s| s.admite(el))
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
    /// Del último cálculo (si no fue entero): los elementos cuyo estilo cambió de verdad (o es
    /// nuevo), y los que tienen un pseudoelemento que se recalculó con ellos. La maquetación
    /// incremental solo tira lo que depende de ellos.
    pub cambiados: Vec<NodoId>,
}

/// Lo que hay que recalcular a partir de los toques: (raíces, que se recalculan con todo lo de
/// dentro; candidatos de `:has()`, que se recalculan solos y solo arrastran lo de dentro si su
/// estilo cambió).
fn raices(doc: &Documento, toques: &Toques, tiene: &ConHas) -> (BTreeSet<NodoId>, BTreeSet<NodoId>) {
    let (mut r, mut candidatos) = (BTreeSet::new(), BTreeSet::new());
    let valido = |id: NodoId| doc.existe(id);
    let siempre: Vec<u32> = SIEMPRE.iter().map(|n| huella_atributo(n)).collect();
    for t in toques.lista.iter().copied() {
        let tocado = match t {
            // Un atributo que ningún selector mira (ni es del `style` o una pista) no cambia nada.
            Toque::Atributo(e, h) if valido(e) && !tiene.mirado(h) && !siempre.contains(&h) => continue,
            Toque::Atributo(e, h) if valido(e) && !tiene.mirado(h) => {
                r.insert(e);
                e
            }
            Toque::Atributo(e, _) if valido(e) => {
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
            por_has(doc, tocado, tiene, &mut candidatos);
        }
    }
    (r, candidatos)
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

/// Un elemento al recorrerlo: su padre (índice), si viene forzado, su estilo anterior y si es
/// raíz o candidato de `:has()`.
struct Visita<'p> {
    id: NodoId,
    padre: usize,
    forzado: bool,
    previo: Option<&'p Estilo>,
    en_raices: bool,
    candidato: bool,
}

/// Lo fijo de un cálculo.
struct Contexto<'c> {
    doc: &'c Documento,
    indice: &'c Indice,
    op: &'c Opciones,
    letra_raiz: f32,
    originales: usize,
}

impl Contexto<'_> {
    /// El estilo de un elemento: (estilo, si lo de dentro se recalcula, si se calculó, y el nodo
    /// que cuenta como cambiado para la maquetación).
    fn visitar(&self, v: &Visita<'_>, estilo_padre: &Estilo) -> (Estilo, bool, bool, Option<NodoId>) {
        let recalcular = v.forzado || v.previo.is_none() || v.en_raices;
        let calcular = || estilo_de(self.doc, v.id, estilo_padre, self.indice, self.op, self.letra_raiz);
        match v.previo {
            Some(p) if !recalcular && v.candidato => {
                // Candidato de `:has()`: se recalcula solo; si no cambió, lo de dentro sigue igual.
                let nuevo = calcular();
                let cambio = nuevo != *p;
                (nuevo, cambio, true, cambio.then_some(v.id))
            }
            Some(p) if !recalcular => (p.clone(), false, false, None),
            _ => {
                let nuevo = calcular();
                let cambio = if v.id.0 as usize >= self.originales {
                    // Un pseudoelemento (número nuevo en cada cálculo): cuenta para su elemento si
                    // este se recalculó.
                    (v.forzado && v.padre != usize::MAX).then_some(NodoId(v.padre as u32))
                } else {
                    (v.previo != Some(&nuevo)).then_some(v.id)
                };
                (nuevo, true, true, cambio)
            }
        }
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
    let (raices, candidatos) = match mismo {
        true => {
            let (r, c) = raices(doc, toques, &indice.tiene);
            (Some(r), c)
        }
        false => (None, BTreeSet::new()),
    };
    let ctx = Contexto { doc, indice: &indice, op, letra_raiz, originales };
    let mut estilos: Vec<Option<Estilo>> = alloc::vec![None; doc.total()];
    let mut recalculados = 0;
    let mut cambiados: Vec<NodoId> = Vec::new();
    // (nodo, estilo del padre, si se recalcula todo lo de dentro)
    let mut pila: Vec<(NodoId, usize, bool)> =
        doc.nodo(Documento::RAIZ).hijos.iter().rev().map(|&h| (h, usize::MAX, false)).collect();
    while let Some((id, padre, forzado)) = pila.pop() {
        if !matches!(doc.nodo(id).dato, Dato::Elemento(_)) {
            continue;
        }
        let i = id.0 as usize;
        let previo = memoria.crudos.get(i).and_then(Option::as_ref).filter(|_| i < originales);
        let en_raices = raices.as_ref().is_none_or(|r| r.contains(&id));
        let estilo_padre = estilos.get(padre).and_then(Option::as_ref).unwrap_or(&raiz);
        let visita = Visita { id, padre, forzado, previo, en_raices, candidato: candidatos.contains(&id) };
        let (e, recalcular, hecho, cambio) = ctx.visitar(&visita, estilo_padre);
        recalculados += usize::from(hecho);
        cambiados.extend(cambio);
        let sigue = e.display != Display::Ninguno;
        estilos[i] = Some(e);
        if sigue {
            pila.extend(doc.nodo(id).hijos.iter().rev().map(|&h| (h, i, recalcular)));
        }
    }
    let con_estilo = estilos.iter().flatten().count();
    let mut finales = estilos.clone();
    estilos.truncate(originales);
    *memoria = Memoria {
        crudos: estilos,
        indice: Some(indice),
        letra_raiz,
        ultimo: (recalculados, con_estilo, !mismo),
        cambiados,
    };
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
        section:not(:has(.m2)) { padding-left: 1px } [data-d]:has(.m3) { padding-top: 2px }
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
    fn el_sujeto_de_un_has_es_la_parte_de_fuera_y_cuenta_sus_atributos() {
        let hoja = quimera_css::analizar(
            "ytd-app[oculto]:not(:has(> .m)) { color: red } [dialog]:has(.x) { color: blue } :root:has(.y) { color: green }",
        );
        let mut t = ConHas::default();
        hoja.reglas.iter().flat_map(|r| &r.selectores).for_each(|s| t.anotar(s));
        let doc = quimera_html::analizar("<ytd-app oculto></ytd-app><ytd-app></ytd-app><div dialog></div><div></div>");
        let els: Vec<&Elemento> = doc.descendientes(Documento::RAIZ).filter_map(|n| doc.elemento(n)).collect();
        let puede = |nombre: &str, attr: Option<&str>| {
            els.iter().any(|e| e.nombre == nombre && attr.is_none_or(|a| e.atributo(a).is_some()) && t.puede(e))
        };
        assert!(puede("ytd-app", Some("oculto")) && puede("div", Some("dialog")) && puede("html", None));
        let sin = els.iter().filter(|e| e.nombre == "div" && e.atributo("dialog").is_none()).all(|e| !t.puede(e));
        assert!(sin, "un div sin [dialog] no puede ser sujeto");
        let app_sin =
            els.iter().filter(|e| e.nombre == "ytd-app" && e.atributo("oculto").is_none()).all(|e| !t.puede(e));
        assert!(app_sin, "el :not(:has()) es de ytd-app[oculto], no de cualquiera");
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
        // :not(:has()) y [atributo]:has(): marcar el <i> de dentro.
        doc.poner_atributo(i, "class", "marca m2 m3");
        let sec = doc.nodo(i).padre.unwrap();
        doc.poner_atributo(sec, "data-d", "1");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        doc.poner_atributo(i, "class", "m3");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        // Sin cambios: nada que recalcular salvo <html>, que sale entero de la memoria.
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        assert_eq!(m.ultimo.0, 0, "sin toques no se recalcula nada");
        // Un atributo que ninguna regla mira: nada que recalcular.
        doc.poner_atributo(div, "data-nadie-lo-mira", "1");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
        assert_eq!(m.ultimo.0, 0, "atributo sin reglas: nada");
        // El style: el elemento y lo de dentro (la herencia), sin los hermanos.
        doc.poner_atributo(div, "style", "color: green");
        let t = doc.tomar_toques();
        comprobar(&doc, &cache, &mut m, &t);
    }
}
