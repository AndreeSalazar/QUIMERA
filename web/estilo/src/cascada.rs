//! La cascada: para cada elemento, qué declaraciones le tocan y cuál gana.
//!
//! Orden (de menos a más fuerza): navegador normal < página normal < página `!important` <
//! navegador `!important`; dentro de cada uno, más especificidad gana, y a igualdad, la que
//! viene después. El atributo `style` gana a cualquier selector de la página.
//!
//! Las reglas se indexan por la parte derecha de su selector (id, primera clase o etiqueta),
//! como hace Stylo: cada elemento solo se prueba contra las reglas que pueden casar con él.

use crate::antepasados::Filtro;
use crate::elemento::Nodo;
use crate::medios;
use crate::propiedades::{self, Contexto};
use crate::valores::{Display, Estilo};
use crate::variables;
use crate::{Estilos, Opciones};
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
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

/// Un selector de una regla, con lo que hace falta para ordenarlo. Dónde está su regla va por
/// números (hoja, bloque `@media`, regla, selector): el índice guarda las hojas.
pub(crate) struct Entrada {
    hoja: u32,
    /// El bloque `@media` (`u32::MAX`: fuera de ninguno).
    medio: u32,
    regla: u32,
    sel: u32,
    /// 0 navegador, 1 página.
    origen: u8,
    especificidad: (u32, u32, u32),
    orden: u32,
    /// Lo que pide a los antepasados (ver `antepasados.rs`).
    antepasados: Filtro,
}

/// Las reglas de todas las hojas, indexadas. Guarda las hojas (así se puede guardar entre un
/// cálculo y el siguiente: ver [`CacheHojas`]).
pub(crate) struct Indice {
    hojas: Vec<(u8, Rc<Hoja>)>,
    pub(crate) entradas: Vec<Entrada>,
    pub(crate) por_id: BTreeMap<String, Vec<usize>>,
    pub(crate) por_clase: BTreeMap<String, Vec<usize>>,
    pub(crate) por_etiqueta: BTreeMap<String, Vec<usize>>,
    pub(crate) resto: Vec<usize>,
    /// Lo que piden los `:has()` (para el estilo incremental, `incremental.rs`).
    pub(crate) tiene: crate::incremental::ConHas,
}

impl Indice {
    pub(crate) fn nuevo(hojas: Vec<(u8, Rc<Hoja>)>, op: &Opciones) -> Indice {
        Indice::nuevo_con(hojas, op, |_| true)
    }

    /// Solo con los selectores que cumplen `incluir` (los de pseudoelementos, por ejemplo).
    fn nuevo_con(hojas: Vec<(u8, Rc<Hoja>)>, op: &Opciones, incluir: fn(&Selector) -> bool) -> Indice {
        let mut ind = Indice {
            hojas: Vec::new(),
            entradas: Vec::new(),
            por_id: BTreeMap::new(),
            por_clase: BTreeMap::new(),
            por_etiqueta: BTreeMap::new(),
            resto: Vec::new(),
            tiene: crate::incremental::ConHas::default(),
        };
        for (h, (origen, hoja)) in hojas.iter().enumerate() {
            for (r, regla) in hoja.reglas.iter().enumerate() {
                ind.anadir(*origen, [h, usize::MAX, r], regla, incluir);
            }
            for (m, medio) in hoja.medios.iter().enumerate().filter(|(_, m)| medios::cumple(&m.condicion, op)) {
                for (r, regla) in medio.reglas.iter().enumerate() {
                    ind.anadir(*origen, [h, m, r], regla, incluir);
                }
            }
        }
        ind.hojas = hojas;
        ind
    }

    fn anadir(&mut self, origen: u8, donde: [usize; 3], regla: &Regla, incluir: fn(&Selector) -> bool) {
        for (s, selector) in regla.selectores.iter().enumerate().filter(|(_, s)| incluir(s)) {
            let i = self.entradas.len();
            self.tiene.anotar(selector);
            self.entradas.push(Entrada {
                hoja: donde[0] as u32,
                medio: if donde[1] == usize::MAX { u32::MAX } else { donde[1] as u32 },
                regla: donde[2] as u32,
                sel: s as u32,
                origen,
                especificidad: selector.especificidad(),
                orden: i as u32,
                antepasados: Filtro::de_selector(selector),
            });
            let Some(derecha) = selector.partes.last() else { continue };
            if let Some(id) = derecha.ids.first() {
                self.por_id.entry(id.clone()).or_default().push(i);
            } else if let Some(c) = derecha.clases.first() {
                self.por_clase.entry(c.clone()).or_default().push(i);
            } else if let Some(t) = &derecha.etiqueta {
                self.por_etiqueta.entry(t.clone()).or_default().push(i);
            } else {
                self.resto.push(i);
            }
        }
    }

    fn regla(&self, en: &Entrada) -> &Regla {
        let hoja = &self.hojas[en.hoja as usize].1;
        match en.medio {
            u32::MAX => &hoja.reglas[en.regla as usize],
            m => &hoja.medios[m as usize].reglas[en.regla as usize],
        }
    }

    /// El selector de una entrada.
    pub(crate) fn selector(&self, en: &Entrada) -> &Selector {
        &self.regla(en).selectores[en.sel as usize]
    }

    /// Las declaraciones de una entrada.
    fn declaraciones(&self, en: &Entrada) -> &[Declaracion] {
        &self.regla(en).declaraciones
    }

    /// Las entradas que casan con el elemento, en el orden de la cascada.
    pub(crate) fn casan(&self, e: Nodo<'_>) -> Vec<&Entrada> {
        let Some(mut el) = e.doc.elemento(e.id) else { return Vec::new() };
        // Un `::before` sintético: sus reglas están indexadas por su elemento de origen.
        if el.nombre.starts_with("::")
            && let Some(origen) = e.doc.nodo(e.id).padre.and_then(|p| e.doc.elemento(p))
        {
            el = origen;
        }
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
        // Primero el filtro de antepasados (barato); solo lo que pasa se comprueba de verdad.
        let filtro = Filtro::de_elemento(e.doc, e.id);
        candidatas
            .into_iter()
            .map(|i| &self.entradas[i])
            .filter(|en| filtro.contiene(&en.antepasados) && self.selector(en).casa(e))
            .collect()
    }
}

/// Las hojas del documento: la del navegador y, en orden de documento, los `<style>` y los
/// `<link rel="stylesheet">` (cuyo texto da `externa` a partir del `href`).
pub(crate) fn hojas(
    doc: &Documento,
    externa: &dyn Fn(&str) -> Option<String>,
    cache: &CacheHojas,
) -> Vec<(u8, Rc<Hoja>)> {
    let mut v = alloc::vec![(0, cache.analizada(NAVEGADOR))];
    if en_compatibilidad(doc) {
        v.push((0, cache.analizada(COMPATIBILIDAD)));
    }
    for id in doc.descendientes(Documento::RAIZ) {
        let Some(e) = doc.elemento(id) else { continue };
        if e.nombre == "style" {
            v.push((1, cache.analizada(&doc.texto(id))));
        }
        let es_hoja =
            e.atributo("rel").is_some_and(|r| r.split_ascii_whitespace().any(|x| x.eq_ignore_ascii_case("stylesheet")));
        let alternativa = e.atributo("rel").is_some_and(|r| r.contains("alternate"));
        if e.nombre == "link"
            && es_hoja
            && !alternativa
            && let Some(hoja) = e.atributo("href").and_then(|h| cache.enlazada(h, externa))
        {
            v.push((1, hoja));
        }
    }
    v
}

/// Las hojas ya analizadas, por su texto: quien calcula el estilo muchas veces sobre la misma
/// página (las medidas que lee JavaScript) no vuelve a analizar el mismo CSS.
#[derive(Default)]
pub struct CacheHojas {
    /// (huella, longitud, hoja), la más reciente al final.
    hojas: RefCell<Vec<(u64, usize, Rc<Hoja>)>>,
    /// Las hojas enlazadas por su `href` (dentro de una página, siempre el mismo texto): así no
    /// se copia ni se recorre su texto en cada cálculo (la de YouTube pasa de 3 MB).
    enlazadas: RefCell<BTreeMap<String, Option<Rc<Hoja>>>>,
    /// Los índices construidos (el de todo y el de los pseudoelementos), por sus hojas y la
    /// pantalla: con las mismas, el mismo índice. En YouTube, 13 ms menos por cálculo.
    indices: RefCell<Vec<(ClaveIndice, Rc<Indice>)>>,
}

/// Con qué hojas (su puntero) y qué pantalla se hizo un índice, y si es el de pseudoelementos.
#[derive(PartialEq)]
struct ClaveIndice {
    seudo: bool,
    pantalla: (u32, u32),
    hojas: Vec<(u8, usize)>,
}

/// Cuántas hojas distintas guarda como mucho.
const MAX_CACHE: usize = 256;

impl CacheHojas {
    /// La hoja de `texto`, analizándola solo la primera vez.
    pub fn analizada(&self, texto: &str) -> Rc<Hoja> {
        // FNV-1a de 64 bits, junto con la longitud.
        let huella =
            texto.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
        let mut hojas = self.hojas.borrow_mut();
        if let Some(h) = hojas.iter().find(|(f, n, _)| *f == huella && *n == texto.len()) {
            return h.2.clone();
        }
        let hoja = Rc::new(quimera_css::analizar(texto));
        if hojas.len() >= MAX_CACHE {
            hojas.remove(0);
        }
        hojas.push((huella, texto.len(), hoja.clone()));
        hoja
    }

    /// El índice de estas hojas para esta pantalla, construyéndolo solo si cambió algo.
    pub(crate) fn indice(&self, hojas: Vec<(u8, Rc<Hoja>)>, op: &Opciones, seudo: bool) -> Rc<Indice> {
        let clave = ClaveIndice {
            seudo,
            pantalla: (op.ancho.to_bits(), op.alto.to_bits()),
            hojas: hojas.iter().map(|(o, h)| (*o, Rc::as_ptr(h) as usize)).collect(),
        };
        if let Some((_, i)) = self.indices.borrow().iter().find(|(c, _)| *c == clave) {
            return i.clone();
        }
        let i = Rc::new(if seudo {
            Indice::nuevo_con(hojas, op, |s| s.seudoelemento().is_some())
        } else {
            Indice::nuevo(hojas, op)
        });
        let mut v = self.indices.borrow_mut();
        v.retain(|(c, _)| c.seudo != seudo);
        v.push((clave, i.clone()));
        i
    }

    /// La hoja enlazada con `href` (su texto lo da `externa` la primera vez).
    fn enlazada(&self, href: &str, externa: &dyn Fn(&str) -> Option<String>) -> Option<Rc<Hoja>> {
        if let Some(h) = self.enlazadas.borrow().get(href) {
            return h.clone();
        }
        let hoja = externa(href).map(|t| self.analizada(&t));
        self.enlazadas.borrow_mut().insert(String::from(href), hoja.clone());
        hoja
    }
}

/// Los `::before` y `::after` con `content`: mete en `doc` un hijo sintético (`::before` al
/// principio, `::after` al final) con el texto de `content`, al que se aplican sus reglas. Así
/// la cascada y la maquetación los tratan como un elemento más. Entiende cadenas y `attr()`;
/// `counter()`, las comillas y las imágenes cuentan como texto vacío.
pub(crate) fn anadir_seudoelementos(
    doc: &mut Documento,
    op: &Opciones,
    externa: &dyn Fn(&str) -> Option<String>,
    cache: &CacheHojas,
) {
    let hojas = hojas(doc, externa, cache);
    // Solo las reglas de `::before` y `::after`: muy pocas al lado de las demás.
    let indice = cache.indice(hojas, op, true);
    if indice.entradas.is_empty() {
        return;
    }
    let elementos: Vec<NodoId> = doc
        .descendientes(Documento::RAIZ)
        .filter(|&id| doc.elemento(id).is_some_and(|e| !e.nombre.starts_with("::")))
        .collect();
    let mut nuevos: Vec<(NodoId, bool, String)> = Vec::new();
    for id in elementos {
        // Los antepasados del pseudoelemento son el elemento y los suyos.
        let mut filtro = None;
        for (antes, nombre) in [(true, "before"), (false, "after")] {
            let f = *filtro.get_or_insert_with(|| Filtro::de_elemento_y_antepasados(doc, id));
            if let Some(texto) = indice.contenido(Nodo { doc, id }, nombre, &f) {
                nuevos.push((id, antes, texto));
            }
        }
    }
    for (id, antes, texto) in nuevos {
        let s = doc.crear_elemento(if antes { "::before" } else { "::after" });
        if !texto.is_empty() {
            let t = doc.crear(Dato::Texto(texto));
            doc.insertar(s, t, None);
        }
        let primero = if antes { doc.nodo(id).hijos.first().copied() } else { None };
        doc.insertar(id, s, primero);
    }
}

impl Indice {
    /// El `content` del pseudoelemento `nombre` de `e`, si alguna regla lo pone.
    fn contenido(&self, e: Nodo<'_>, nombre: &str, filtro: &Filtro) -> Option<String> {
        let el = e.doc.elemento(e.id)?;
        let mut candidatas: Vec<usize> = self.resto.clone();
        let mut juntar = |v: Option<&Vec<usize>>| candidatas.extend(v.into_iter().flatten());
        if let Some(id) = el.atributo("id") {
            juntar(self.por_id.get(id));
        }
        for c in el.atributo("class").unwrap_or("").split_ascii_whitespace() {
            juntar(self.por_clase.get(c));
        }
        juntar(self.por_etiqueta.get(el.nombre.as_str()));
        let mejor = candidatas
            .into_iter()
            .map(|i| &self.entradas[i])
            .filter(|en| self.selector(en).seudoelemento() == Some(nombre))
            .filter_map(|en| self.declaraciones(en).iter().rev().find(|d| d.propiedad == "content").map(|d| (en, d)))
            .filter(|(en, _)| filtro.contiene(&en.antepasados) && self.selector(en).casa_origen(e))
            .max_by_key(|(en, d)| (d.importante, en.especificidad, en.orden))?;
        texto_de_content(&mejor.1.valor, el)
    }
}

/// El texto de `content`: `None` con `none`/`normal` (no hay pseudoelemento).
fn texto_de_content(valor: &[Token], el: &quimera_html::Elemento) -> Option<String> {
    let mut s = String::new();
    let mut i = 0;
    while i < valor.len() {
        match &valor[i] {
            Token::Ident(n) if n.eq_ignore_ascii_case("none") || n.eq_ignore_ascii_case("normal") => return None,
            Token::Cadena(t) => s.push_str(t),
            Token::Funcion(f) if f.eq_ignore_ascii_case("attr") => {
                if let Some(Token::Ident(a)) = valor.get(i + 1) {
                    s.push_str(el.atributo(&a.to_ascii_lowercase()).unwrap_or(""));
                }
                while i < valor.len() && valor[i] != Token::Cierra(')') {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    Some(s)
}

/// Calcula el estilo de todos los elementos.
pub(crate) fn calcular(
    doc: &Documento,
    op: &Opciones,
    externa: &dyn Fn(&str) -> Option<String>,
    cache: &CacheHojas,
) -> Estilos {
    let hojas = hojas(doc, externa, cache);
    let indice = cache.indice(hojas, op, false);
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
    // La opacidad, ya con la herencia hecha (si no, los hijos la heredarían dos veces en su color).
    estilos.iter_mut().flatten().filter(|e| e.opacidad < 1.0).for_each(aplicar_opacidad);
    Estilos { estilos }
}

/// Multiplica la opacidad en los colores del elemento (los de dentro llevan ya la suya, que
/// incluye esta). Como la opacidad de grupo de CSS cuando lo de dentro no se solapa.
pub(crate) fn aplicar_opacidad(e: &mut Estilo) {
    let por = |a: u8| (f32::from(a) * e.opacidad + 0.5) as u8;
    // Un borde sin color usa el del texto: el de antes de multiplicar.
    let color = e.color;
    e.color.a = por(e.color.a);
    e.fondo.a = por(e.fondo.a);
    for b in &mut e.borde {
        let mut c = b.color.unwrap_or(color);
        c.a = por(c.a);
        b.color = Some(c);
    }
    if e.opacidad <= 0.0 {
        e.visible = false;
    }
}

/// El estilo de un elemento a partir del de su padre.
pub(crate) fn estilo_de(
    doc: &Documento,
    id: NodoId,
    padre: &Estilo,
    ind: &Indice,
    op: &Opciones,
    letra_raiz: f32,
) -> Estilo {
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
        for d in ind.declaraciones(en) {
            todas.push((fuerza(en.origen, d.importante), d));
        }
    }
    for d in &en_linea {
        todas.push((fuerza(1, d.importante), d));
    }
    todas.sort_by_key(|(f, _)| *f);
    // Primero las propiedades personalizadas (`--x`), en orden de cascada: las demás las usan.
    let propias: Vec<(String, Vec<Token>)> = todas
        .iter()
        .filter(|(_, d)| d.propiedad.starts_with("--"))
        .map(|(_, d)| (d.propiedad.clone(), d.valor.clone()))
        .collect();
    let vars = variables::del_elemento(&padre.variables, propias);
    let mut ganadoras: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    for (_, d) in todas {
        if d.propiedad.starts_with("--") {
            continue;
        }
        // `var()` se sustituye antes de expandir: `margin: var(--m) 0` también vale.
        let sustituido;
        let valor: &[Token] = if variables::tiene_var(&d.valor) {
            sustituido = variables::valor_de(&d.valor, &vars);
            &sustituido
        } else {
            &d.valor
        };
        for (prop, valor) in propiedades::expandir(&d.propiedad, valor) {
            if propiedades::valida(&prop, &valor) {
                ganadoras.insert(prop, valor);
            }
        }
    }
    let mut e = calcular_valores(padre, &ganadoras, op, letra_raiz);
    e.variables = vars;
    e
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
            if let Some(lado) = e.atributo("align").and_then(lado_html) {
                css.push_str(&alloc::format!("float:{lado};"));
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
        "img" | "iframe" | "object" | "embed" => {
            if let Some(lado) = e.atributo("align").and_then(lado_html) {
                css.push_str(&alloc::format!("float:{lado};"));
            }
            for (atributo, lados) in [("hspace", "left:{};margin-right"), ("vspace", "top:{};margin-bottom")] {
                if let Some(m) = e.atributo(atributo).and_then(longitud_html) {
                    css.push_str(&alloc::format!("margin-{}:{m};", lados.replace("{}", &m)));
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

/// `align=left|right` de una imagen o una tabla: flota a ese lado.
fn lado_html(a: &str) -> Option<&'static str> {
    match a.trim().to_ascii_lowercase().as_str() {
        "left" => Some("left"),
        "right" => Some("right"),
        _ => None,
    }
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
    e.resolver_caja_borde();
    // `absolute` y `fixed` no flotan (CSS 2.1, 9.7).
    if e.colocacion.posicion.fuera_del_flujo() {
        e.flotar = crate::valores::Flotar::No;
    }
    // Un flotante o una caja fuera del flujo es siempre de bloque: lo que era en línea o parte de
    // una tabla pasa a ser un bloque.
    if (e.flotar != crate::valores::Flotar::No || e.colocacion.posicion.fuera_del_flujo())
        && matches!(
            e.display,
            Display::EnLinea
                | Display::BloqueEnLinea
                | Display::Fila
                | Display::Celda
                | Display::GrupoFilas
                | Display::Titulo
        )
    {
        e.display = Display::Bloque;
    }
    e
}

#[cfg(test)]
mod banco;
