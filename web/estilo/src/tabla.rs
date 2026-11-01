//! Las propiedades de las tablas: `border-collapse` y `border-spacing` (se heredan) y
//! `vertical-align` (no se hereda; la hoja del navegador hace que las celdas lo tomen de su
//! fila con `inherit`).

use crate::propiedades::{Contexto, medida};
use crate::valores::Medida;
use quimera_css::{Token, piezas};

/// `vertical-align` (de las celdas: dónde va su contenido si la fila es más alta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlinearVertical {
    /// `baseline` (por ahora como `top`: la primera línea arriba).
    Base,
    /// `top`
    Arriba,
    /// `middle`
    Medio,
    /// `bottom`
    Abajo,
}

/// Lo de las tablas en un elemento.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tabla {
    /// `border-collapse: collapse`: las celdas comparten sus bordes.
    pub colapsar: bool,
    /// `border-spacing` (horizontal, vertical) en píxeles.
    pub espaciado: (f32, f32),
    /// `vertical-align`
    pub vertical: AlinearVertical,
}

impl Default for Tabla {
    fn default() -> Self {
        Tabla { colapsar: false, espaciado: (0.0, 0.0), vertical: AlinearVertical::Base }
    }
}

impl Tabla {
    /// Lo que hereda un hijo (todo menos `vertical-align`).
    pub(crate) fn heredada(&self) -> Tabla {
        Tabla { vertical: AlinearVertical::Base, ..*self }
    }
}

/// Si la propiedad es de las tablas.
pub(crate) fn es_propia(p: &str) -> bool {
    matches!(p, "border-collapse" | "border-spacing" | "vertical-align")
}

fn vertical(n: &str) -> Option<AlinearVertical> {
    Some(match n {
        "baseline" | "sub" | "super" | "text-top" | "text-bottom" => AlinearVertical::Base,
        "top" => AlinearVertical::Arriba,
        "middle" => AlinearVertical::Medio,
        "bottom" => AlinearVertical::Abajo,
        _ => return None,
    })
}

/// Calcula una propiedad sobre `t`; devuelve si el valor era válido.
pub(crate) fn aplicar(t: &mut Tabla, prop: &str, v: &[Token], c: &Contexto) -> bool {
    let n = match v {
        [Token::Ident(x)] => Some(x.to_ascii_lowercase()),
        _ => None,
    };
    match prop {
        "border-collapse" => match n.as_deref() {
            Some("collapse") => {
                t.colapsar = true;
                true
            }
            Some("separate") => {
                t.colapsar = false;
                true
            }
            _ => false,
        },
        "border-spacing" => {
            let px = |p: &[Token]| match medida(p, c) {
                Some(Medida::Px(x)) if x >= 0.0 => Some(x),
                _ => None,
            };
            let espaciado = match piezas(v).as_slice() {
                [a] => px(a).map(|a| (a, a)),
                [a, b] => px(a).zip(px(b)),
                _ => None,
            };
            espaciado.map(|e| t.espaciado = e).is_some()
        }
        // Las longitudes (`vertical-align: 3px`) cuentan como la línea base.
        "vertical-align" => match n.as_deref().map(vertical) {
            Some(Some(a)) => {
                t.vertical = a;
                true
            }
            Some(None) => false,
            None => medida(v, c).map(|_| t.vertical = AlinearVertical::Base).is_some(),
        },
        _ => false,
    }
}

/// Copia una propiedad de `de` (para `inherit` / `initial`).
pub(crate) fn copiar(t: &mut Tabla, de: &Tabla, prop: &str) {
    match prop {
        "border-collapse" => t.colapsar = de.colapsar,
        "border-spacing" => t.espaciado = de.espaciado,
        "vertical-align" => t.vertical = de.vertical,
        _ => {}
    }
}
