//! Gráficos vectoriales de QUIMERA: SVG → imagen RGBA, propio y `no_std`.
//!
//! Lo que dibuja (lo que usan los iconos y logotipos de las páginas, como los de YouTube):
//!
//! - **Formas**: `path` (todas las órdenes de `d`, también arcos), `rect` (con esquinas
//!   redondeadas), `circle`, `ellipse`, `polygon`, `polyline`; grupos `g`, `svg` anidados, `a`,
//!   `switch` y `use` (`href="#id"`).
//! - **Relleno**: `fill` (colores con nombre, `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`,
//!   `currentColor`), `fill-opacity`, `opacity`, `fill-rule`, heredados, también desde
//!   `style="…"`. Un degradado (`url(#id)`) se pinta con su primer color.
//! - **`transform`** (`matrix`, `translate`, `scale`, `rotate`, `skewX`, `skewY`) y **`viewBox`**
//!   con `preserveAspectRatio` (`none` y `meet` centrado).
//!
//! Falta: trazos (`stroke`), texto, recortes (`clipPath`, `mask`), filtros, degradados de
//! verdad y las hojas `<style>` de dentro del SVG.

#![no_std]

extern crate alloc;

mod mates;
mod relleno;
mod ruta;
mod xml;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
pub use relleno::{Lienzo, Rgba};
use ruta::{Matriz, Trazador, numeros, transformacion};
use xml::Elemento;

/// Lado máximo de la imagen que se rasteriza.
pub const MAX_LADO: u32 = 4096;

/// El tamaño propio de un SVG: `width` y `height` en píxeles; si falta uno, sale del otro con la
/// proporción del `viewBox`; sin nada, 300 × 150 (como `<img>` en los navegadores).
pub fn tam_propio(svg: &[u8]) -> Option<(u32, u32)> {
    let texto = core::str::from_utf8(svg).ok()?;
    let els = xml::leer(texto);
    let raiz = raiz_svg(&els)?;
    let (w, h) = tam_de(&els[raiz]);
    Some(((w as u32).clamp(1, MAX_LADO), (h as u32).clamp(1, MAX_LADO)))
}

/// Rasteriza un SVG a su tamaño propio.
pub fn rasterizar(svg: &[u8]) -> Option<Lienzo> {
    let (w, h) = tam_propio(svg)?;
    rasterizar_a(svg, w, h)
}

/// Rasteriza un SVG a `ancho` × `alto` píxeles (su `viewBox` se ajusta a ese tamaño).
pub fn rasterizar_a(svg: &[u8], ancho: u32, alto: u32) -> Option<Lienzo> {
    let texto = core::str::from_utf8(svg).ok()?;
    let els = xml::leer(texto);
    let raiz = raiz_svg(&els)?;
    let (ancho, alto) = (ancho.clamp(1, MAX_LADO), alto.clamp(1, MAX_LADO));
    let mut ids = BTreeMap::new();
    for (i, e) in els.iter().enumerate() {
        if let Some(id) = e.atributo("id") {
            ids.entry(String::from(id)).or_insert(i);
        }
    }
    let mut d = Dibujo { els: &els, ids, lienzo: Lienzo::nuevo(ancho, alto), profundidad: 0 };
    let m = encaje(&els[raiz], ancho as f32, alto as f32);
    // La raíz también aporta lo suyo (`color`, `fill`...); su `transform` no cuenta.
    let estado = d.heredar(&Estado::inicial(), &propiedades(&els[raiz]));
    d.hijos(raiz, &estado, &m);
    Some(d.lienzo)
}

fn raiz_svg(els: &[Elemento]) -> Option<usize> {
    els.first()?.hijos.iter().copied().find(|&i| els[i].nombre == "svg")
}

/// Un número de `width`/`height` en píxeles (los porcentajes no cuentan).
fn longitud(v: Option<&str>) -> Option<f32> {
    let v = v?.trim();
    if v.ends_with('%') {
        return None;
    }
    numeros(v).first().copied().filter(|n| *n > 0.0)
}

fn caja_de_vista(el: &Elemento) -> Option<[f32; 4]> {
    let v = numeros(el.atributo("viewBox")?);
    (v.len() == 4 && v[2] > 0.0 && v[3] > 0.0).then(|| [v[0], v[1], v[2], v[3]])
}

fn tam_de(el: &Elemento) -> (f32, f32) {
    tam_de_atributos(el.atributo("width"), el.atributo("height"), el.atributo("viewBox"))
}

/// El tamaño propio a partir de los atributos `width`, `height` y `viewBox` de un `<svg>` (para
/// quien maqueta un `<svg>` dentro de HTML sin tener que leerlo entero).
pub fn tam_de_atributos(ancho: Option<&str>, alto: Option<&str>, caja: Option<&str>) -> (f32, f32) {
    let (w, h) = (longitud(ancho), longitud(alto));
    let caja = caja.map(numeros).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
    match (w, h, caja) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some(v)) => (w, w * v[3] / v[2]),
        (None, Some(h), Some(v)) => (h * v[2] / v[3], h),
        (None, None, Some(v)) => (v[2], v[3]),
        _ => (w.unwrap_or(300.0), h.unwrap_or(150.0)),
    }
}

/// La matriz que lleva el `viewBox` al lienzo de `ancho` × `alto`.
fn encaje(el: &Elemento, ancho: f32, alto: f32) -> Matriz {
    let Some([vx, vy, vw, vh]) = caja_de_vista(el) else {
        let (w, h) = tam_de(el);
        return Matriz([ancho / w, 0.0, 0.0, alto / h, 0.0, 0.0]);
    };
    let (sx, sy) = (ancho / vw, alto / vh);
    if el.atributo("preserveAspectRatio").is_some_and(|p| p.trim().starts_with("none")) {
        return Matriz([sx, 0.0, 0.0, sy, -vx * sx, -vy * sy]);
    }
    let s = sx.min(sy);
    let (dx, dy) = ((ancho - vw * s) / 2.0, (alto - vh * s) / 2.0);
    Matriz([s, 0.0, 0.0, s, dx - vx * s, dy - vy * s])
}

/// Lo que heredan los hijos.
#[derive(Clone, Copy)]
struct Estado {
    relleno: Option<Rgba>,
    opacidad_relleno: f32,
    opacidad: f32,
    color: Rgba,
    par_impar: bool,
    visible: bool,
}

impl Estado {
    fn inicial() -> Estado {
        let negro = Rgba { r: 0, g: 0, b: 0, a: 1.0 };
        Estado {
            relleno: Some(negro),
            opacidad_relleno: 1.0,
            opacidad: 1.0,
            color: negro,
            par_impar: false,
            visible: true,
        }
    }
}

/// Elementos que no se dibujan donde están.
const NO_SE_DIBUJAN: &[&str] = &[
    "defs",
    "clipPath",
    "mask",
    "symbol",
    "linearGradient",
    "radialGradient",
    "pattern",
    "marker",
    "style",
    "title",
    "desc",
    "metadata",
    "filter",
    "text",
    "script",
];

struct Dibujo<'a> {
    els: &'a [Elemento],
    ids: BTreeMap<String, usize>,
    lienzo: Lienzo,
    /// Contra los `use` que se usan a sí mismos.
    profundidad: u32,
}

impl Dibujo<'_> {
    fn hijos(&mut self, i: usize, estado: &Estado, m: &Matriz) {
        for &h in &self.els[i].hijos {
            self.elemento(h, estado, m);
        }
    }

    fn elemento(&mut self, i: usize, padre: &Estado, m: &Matriz) {
        let el = &self.els[i];
        if NO_SE_DIBUJAN.contains(&el.nombre.as_str()) || self.profundidad > 32 {
            return;
        }
        let props = propiedades(el);
        if prop(&props, "display") == Some("none") {
            return;
        }
        let estado = self.heredar(padre, &props);
        let m = match el.atributo("transform") {
            Some(t) => m.por(&transformacion(t)),
            None => *m,
        };
        match el.nombre.as_str() {
            "svg" | "g" | "a" | "switch" => self.hijos(i, &estado, &m),
            "use" => {
                let Some(destino) = el.atributo("href").and_then(|h| h.strip_prefix('#')).and_then(|h| self.ids.get(h))
                else {
                    return;
                };
                let n = |k: &str| el.atributo(k).and_then(|v| numeros(v).first().copied()).unwrap_or(0.0);
                let m = m.por(&Matriz([1.0, 0.0, 0.0, 1.0, n("x"), n("y")]));
                let destino = *destino;
                self.profundidad += 1;
                if self.els[destino].nombre == "symbol" {
                    self.hijos(destino, &estado, &m);
                } else {
                    self.elemento(destino, &estado, &m);
                }
                self.profundidad -= 1;
            }
            _ => self.forma(el, &estado, &m),
        }
    }

    /// El estado de un elemento a partir del de su padre y sus propiedades.
    fn heredar(&self, padre: &Estado, props: &[(String, String)]) -> Estado {
        let prop = |k: &str| prop(props, k);
        let mut e = *padre;
        if let Some(c) = prop("color").and_then(|c| color(c, padre.color)) {
            e.color = c;
        }
        if let Some(f) = prop("fill") {
            e.relleno = self.pintura(f, e.color);
        }
        let num = |k: &str| prop(k).and_then(|v| numeros(v).first().copied()).map(|n| n.clamp(0.0, 1.0));
        if let Some(o) = num("fill-opacity") {
            e.opacidad_relleno = o;
        }
        if let Some(o) = num("opacity") {
            e.opacidad *= o;
        }
        match prop("fill-rule").map(str::trim) {
            Some("evenodd") => e.par_impar = true,
            Some("nonzero") => e.par_impar = false,
            _ => {}
        }
        match prop("visibility").map(str::trim) {
            Some("hidden" | "collapse") => e.visible = false,
            Some("visible") => e.visible = true,
            _ => {}
        }
        e
    }

    /// `fill`: un color, `none` o un degradado (su primer color).
    fn pintura(&self, v: &str, actual: Rgba) -> Option<Rgba> {
        let v = v.trim();
        if let Some(id) = v.strip_prefix("url(").and_then(|r| r.split(')').next()) {
            let id = id.trim().trim_matches(|c| c == '"' || c == '\'').trim_start_matches('#');
            let g = &self.els[*self.ids.get(id)?];
            let parada = &self.els[*g.hijos.first()?];
            let props = propiedades(parada);
            let mut c = color(prop(&props, "stop-color").unwrap_or("black"), actual)?;
            c.a *= prop(&props, "stop-opacity").and_then(|o| numeros(o).first().copied()).unwrap_or(1.0);
            return Some(c);
        }
        color(v, actual)
    }

    fn forma(&mut self, el: &Elemento, e: &Estado, m: &Matriz) {
        let Some(mut c) = e.relleno.filter(|_| e.visible) else { return };
        c.a *= e.opacidad_relleno * e.opacidad;
        let mut t = Trazador::nuevo(*m);
        if el.nombre == "path" {
            ruta::trazar_d(el.atributo("d").unwrap_or(""), &mut t);
        } else {
            ruta::trazar_forma(el, &mut t);
        }
        self.lienzo.rellenar(&t.poligonos, e.par_impar, c);
    }
}

/// Los atributos de presentación y, después (ganan), las declaraciones de `style`.
fn propiedades(el: &Elemento) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = el.atributos.clone();
    if let Some(estilo) = el.atributo("style") {
        for decl in estilo.split(';') {
            if let Some((k, val)) = decl.split_once(':') {
                let val = val.trim().trim_end_matches("!important").trim();
                v.push((String::from(k.trim()), String::from(val)));
            }
        }
    }
    v
}

/// La última declaración de `k` (la de `style` gana a la del atributo).
fn prop<'a>(props: &'a [(String, String)], k: &str) -> Option<&'a str> {
    props.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

/// Un color CSS. `currentColor` es `actual`; `none` y `transparent` no pintan.
pub fn color(v: &str, actual: Rgba) -> Option<Rgba> {
    let v = v.trim();
    let minus = v.to_ascii_lowercase();
    if minus == "currentcolor" || minus == "inherit" {
        return Some(actual);
    }
    if let Some(h) = v.strip_prefix('#') {
        return hexadecimal(h);
    }
    if let Some(dentro) = minus.strip_prefix("rgba(").or_else(|| minus.strip_prefix("rgb(")) {
        let partes: Vec<&str> = dentro.trim_end_matches(')').split([',', '/', ' ']).filter(|p| !p.is_empty()).collect();
        let canal = |i: usize| -> u8 {
            let p = partes.get(i).copied().unwrap_or("0");
            let n = numeros(p).first().copied().unwrap_or(0.0);
            (if p.ends_with('%') { n * 2.55 } else { n }).clamp(0.0, 255.0) as u8
        };
        let a = partes.get(3).map_or(1.0, |p| {
            let n = numeros(p).first().copied().unwrap_or(1.0);
            if p.ends_with('%') { n / 100.0 } else { n }
        });
        return Some(Rgba { r: canal(0), g: canal(1), b: canal(2), a: a.clamp(0.0, 1.0) });
    }
    let (r, g, b) = match minus.as_str() {
        "none" | "transparent" => return None,
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "purple" => (128, 0, 128),
        "lime" => (0, 255, 0),
        _ => return None,
    };
    Some(Rgba { r, g, b, a: 1.0 })
}

fn hexadecimal(h: &str) -> Option<Rgba> {
    let d = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
    let corto = |i: usize| d(i, 1).map(|v| v * 17);
    let (r, g, b, a) = match h.len() {
        3 => (corto(0)?, corto(1)?, corto(2)?, 255),
        4 => (corto(0)?, corto(1)?, corto(2)?, corto(3)?),
        6 => (d(0, 2)?, d(2, 2)?, d(4, 2)?, 255),
        8 => (d(0, 2)?, d(2, 2)?, d(4, 2)?, d(6, 2)?),
        _ => return None,
    };
    Some(Rgba { r, g, b, a: f32::from(a) / 255.0 })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pixel(l: &Lienzo, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * l.ancho + x) * 4) as usize;
        [l.rgba[i], l.rgba[i + 1], l.rgba[i + 2], l.rgba[i + 3]]
    }

    #[test]
    fn tamanos() {
        assert_eq!(tam_propio(br#"<svg viewBox="0 0 24 24"/>"#), Some((24, 24)));
        assert_eq!(tam_propio(br#"<svg width="93" viewBox="0 0 93 20"/>"#), Some((93, 20)));
        assert_eq!(tam_propio(br#"<svg width="100%" height="10px"/>"#), Some((300, 10)));
        assert_eq!(tam_propio(b"<svg/>"), Some((300, 150)));
    }

    #[test]
    fn un_icono_con_current_color_escalado() {
        let svg = br##"<svg viewBox="0 0 10 10" color="#ff0000"><g fill="currentColor"><path d="M0 0h5v10H0z"/></g><rect x="5" y="0" width="5" height="10" fill="#00f" fill-opacity="0.5"/></svg>"##;
        let l = rasterizar_a(svg, 20, 20).expect("svg");
        assert_eq!(pixel(&l, 2, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&l, 15, 10), [0, 0, 255, 128]);
    }

    #[test]
    fn use_degradado_y_ocultos() {
        let svg = br##"<svg width="10" height="10"><defs><linearGradient id="g"><stop offset="0" stop-color="rgb(0, 255, 0)"/></linearGradient><path id="p" d="M0 0h4v4H0z"/></defs><use href="#p" x="5" fill="url(#g)"/><circle cx="2" cy="8" r="2" style="display:none"/></svg>"##;
        let l = rasterizar(svg).expect("svg");
        assert_eq!(pixel(&l, 7, 2), [0, 255, 0, 255]);
        assert_eq!(pixel(&l, 2, 2), [0, 0, 0, 0], "lo de <defs> no se dibuja donde está");
        assert_eq!(pixel(&l, 2, 8)[3], 0);
    }
}
