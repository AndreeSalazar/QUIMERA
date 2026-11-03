//! Elementos reemplazados (`<img>` y `<svg>` dentro del HTML): su tamaño como lo calcula CSS
//! (2.1, §10.3.2 y §10.6.2).
//!
//! Un `<svg>` sale como una imagen `data:` con su propio código, el color del texto (para
//! `currentColor`) y el doble de su tamaño en píxeles (para que se vea nítido): la ventana la
//! rasteriza con `quimera-vector` como a cualquier otra imagen cercana a la vista.
//!
//! - Con ancho y alto (de CSS o de los atributos, que el estilo ya convirtió en pistas), esos.
//! - Con uno solo, el otro guarda la proporción de la imagen.
//! - Sin ninguno, el tamaño propio de la imagen.
//! - `max-width` la reduce sin deformarla si el alto es automático (`max-width: 100%`).
//!
//! El tamaño propio de cada imagen lo da quien maqueta (la imagen ya descargada); si aún no
//! ha llegado, se usa la proporción de los atributos.

use crate::bloque::{Hecho, Maquetador};
use crate::pintura::Pieza;
use alloc::string::String;
use quimera_estilo::{Estilo, Medida, Relleno};
use quimera_html::NodoId;

/// Si el nodo es una imagen (`<img>` o `<svg>`).
pub(crate) fn es_imagen(m: &Maquetador<'_>, id: NodoId) -> bool {
    m.doc.elemento(id).is_some_and(|e| e.nombre == "img" || e.nombre == "svg")
}

/// Cuántas veces su tamaño se rasteriza un `<svg>`.
const NITIDEZ: f32 = 2.0;

impl Maquetador<'_> {
    /// (ancho, alto) de la caja de contenido de una imagen en un contenedor de `contenedor` píxeles.
    pub(crate) fn tam_imagen(&self, id: NodoId, e: &Estilo, contenedor: f32) -> (f32, f32) {
        let el = self.doc.elemento(id);
        let propio = match el {
            Some(el) if el.nombre == "svg" => {
                // El analizador guarda `viewBox` como en SVG (o en minúsculas, si venía así).
                let caja = el.atributo("viewBox").or_else(|| el.atributo("viewbox"));
                Some(quimera_vector::tam_de_atributos(el.atributo("width"), el.atributo("height"), caja))
            }
            _ => {
                el.and_then(|el| el.atributo("src")).and_then(|s| (self.imagenes)(s)).map(|(w, h)| (w as f32, h as f32))
            }
        };
        let atributo = |n: &str| {
            el.and_then(|el| el.atributo(n)).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
        };
        let proporcion = propio
            .filter(|(_, h)| *h > 0.0)
            .map(|(w, h)| w / h)
            .or_else(|| atributo("width").zip(atributo("height")).filter(|(_, h)| *h > 0.0).map(|(w, h)| w / h));
        let w_css = match e.ancho {
            Medida::Px(v) => Some(v),
            Medida::Porcentaje(p) if contenedor > 0.0 => Some(contenedor * p / 100.0),
            _ => None,
        };
        let h_css = match e.alto {
            Medida::Px(v) => Some(v),
            _ => None,
        };
        let (pw, ph) = propio.unwrap_or((0.0, 0.0));
        let (mut w, mut h) = match (w_css, h_css) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, proporcion.map_or(ph, |r| w / r)),
            (None, Some(h)) => (proporcion.map_or(pw, |r| h * r), h),
            (None, None) => (pw, ph),
        };
        let maximo = match e.ancho_max {
            Medida::Px(v) => Some(v),
            Medida::Porcentaje(p) if contenedor > 0.0 => Some(contenedor * p / 100.0),
            _ => None,
        };
        if let Some(max) = maximo.filter(|m| w > *m) {
            if h_css.is_none() && w > 0.0 {
                h = h * max / w;
            }
            w = max;
        }
        (w.max(0.0), h.max(0.0))
    }

    /// Lo que identifica la imagen: el `src` de un `<img>`, o el `data:` de un `<svg>` de `ancho` × `alto`.
    pub(crate) fn src_imagen(&self, id: NodoId, e: &Estilo, (ancho, alto): (f32, f32)) -> String {
        let Some(el) = self.doc.elemento(id) else { return String::new() };
        if el.nombre != "svg" {
            return String::from(el.atributo("src").unwrap_or(""));
        }
        let mut codigo = String::new();
        self.doc.escribir_html(id, &mut codigo);
        let c = e.color;
        // `fill` de las hojas de la página (como `fill: currentColor` en los iconos de YouTube).
        let relleno = match e.relleno_svg {
            Relleno::Negro => String::new(),
            Relleno::Actual => String::from(" fill=\"currentColor\""),
            Relleno::Ninguno => String::from(" fill=\"none\""),
            Relleno::Color(f) => {
                alloc::format!(" fill=\"rgba({}, {}, {}, {})\"", f.r, f.g, f.b, f32::from(f.a) / 255.0)
            }
        };
        // Los atributos que se añaden van primero: ganan a los que ya tenía (como el CSS a los
        // atributos de presentación).
        let propios = alloc::format!(
            "<svg color=\"rgb({}, {}, {})\"{relleno} width=\"{}\" height=\"{}\"",
            c.r,
            c.g,
            c.b,
            (ancho * NITIDEZ) as u32,
            (alto * NITIDEZ) as u32
        );
        let codigo = codigo.replacen("<svg", &propios, 1).replace('%', "%25").replace('#', "%23").replace('?', "%3F");
        alloc::format!("data:image/svg+xml;utf8,{codigo}")
    }

    /// Una imagen como bloque (`display: block`, o un elemento flexible): su caja con borde
    /// empieza en (x, y); `margin: auto` la centra.
    pub(crate) fn bloque_imagen(&mut self, id: NodoId, e: &Estilo, (x, y): (f32, f32), contenedor: f32) -> Hecho {
        let px = |m: Medida| m.px(contenedor, 0.0);
        let extras_h = px(e.relleno[1]) + px(e.relleno[3]) + e.borde[1].ocupa() + e.borde[3].ocupa();
        let extras_v = px(e.relleno[0]) + px(e.relleno[2]) + e.borde[0].ocupa() + e.borde[2].ocupa();
        let (ml, mr) = (px(e.margen[3]), px(e.margen[1]));
        let libre = self.ancho_libre == Some(id);
        let (mut w, mut h) = self.tam_imagen(id, e, contenedor);
        if libre {
            // Elemento flexible: el ancho que le da el contenedor (y el alto en proporción).
            let nuevo = (contenedor - ml - mr - extras_h).max(0.0);
            if w > 0.0 && e.alto == Medida::Auto {
                h = h * nuevo / w;
            }
            w = nuevo;
        }
        if let Some((_, forzado)) = self.alto_forzado.filter(|(n, _)| *n == id) {
            h = (forzado - extras_v).max(0.0);
        }
        let sobra = contenedor - w - extras_h - ml - mr;
        let izquierda = match (e.margen[3] == Medida::Auto, e.margen[1] == Medida::Auto, libre) {
            (true, true, false) => sobra.max(0.0) / 2.0,
            (true, false, false) => sobra.max(0.0) + ml,
            (_, _, false) if self.padre_centra(id) => ml + sobra.max(0.0) / 2.0,
            _ => ml,
        };
        let (cx, cy) =
            (x + izquierda + px(e.relleno[3]) + e.borde[3].ocupa(), y + px(e.relleno[0]) + e.borde[0].ocupa());
        let src = self.src_imagen(id, e, (w, h));
        if e.visible && w > 0.0 && h > 0.0 {
            self.piezas.push(Pieza::Imagen { x: cx, y: cy, ancho: w, alto: h, src });
        }
        let caja = (x + izquierda, w + extras_h);
        Hecho { alto: h + extras_v, margen_inferior: px(e.margen[2]), primera_base: None, caja }
    }
}
