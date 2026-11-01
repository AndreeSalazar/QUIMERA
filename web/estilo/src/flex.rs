//! Las propiedades de flexbox: `flex-direction`, `flex-wrap`, `justify-content`, `align-items`,
//! `align-self`, `flex-grow`, `flex-shrink`, `flex-basis`, `gap`, `order`, y los atajos `flex`,
//! `flex-flow` y `gap`. Ninguna se hereda.

use crate::propiedades::{Contexto, medida};
use crate::valores::Medida;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::{Token, piezas};

/// `flex-direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direccion {
    /// `row`
    Fila,
    /// `row-reverse`
    FilaInversa,
    /// `column`
    Columna,
    /// `column-reverse`
    ColumnaInversa,
}

impl Direccion {
    /// Si el eje principal es horizontal.
    pub fn es_fila(self) -> bool {
        matches!(self, Direccion::Fila | Direccion::FilaInversa)
    }

    /// Si va al revés.
    pub fn inversa(self) -> bool {
        matches!(self, Direccion::FilaInversa | Direccion::ColumnaInversa)
    }
}

/// `justify-content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justificar {
    /// `flex-start`, `start`, `normal`
    Inicio,
    /// `flex-end`, `end`
    Fin,
    /// `center`
    Centro,
    /// `space-between`
    Entre,
    /// `space-around`
    Alrededor,
    /// `space-evenly`
    Uniforme,
}

/// `align-items` / `align-self`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlinearFlex {
    /// `stretch`, `normal`
    Estirar,
    /// `flex-start`, `start`
    Inicio,
    /// `flex-end`, `end`
    Fin,
    /// `center`
    Centro,
    /// `baseline` (por ahora como `flex-start`)
    Base,
}

/// Las propiedades de flexbox de un elemento (como contenedor y como elemento flexible).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flex {
    /// `flex-direction`
    pub direccion: Direccion,
    /// `flex-wrap: wrap`
    pub envolver: bool,
    /// `justify-content`
    pub justificar: Justificar,
    /// `align-items`
    pub alinear_items: AlinearFlex,
    /// `align-self` (`None` = `auto`: el `align-items` del contenedor)
    pub alinear_propio: Option<AlinearFlex>,
    /// `flex-grow`
    pub crecer: f32,
    /// `flex-shrink`
    pub encoger: f32,
    /// `flex-basis`
    pub base: Medida,
    /// `row-gap`
    pub hueco_filas: Medida,
    /// `column-gap`
    pub hueco_columnas: Medida,
    /// `order`
    pub orden: i32,
}

impl Default for Flex {
    fn default() -> Self {
        Flex {
            direccion: Direccion::Fila,
            envolver: false,
            justificar: Justificar::Inicio,
            alinear_items: AlinearFlex::Estirar,
            alinear_propio: None,
            crecer: 0.0,
            encoger: 1.0,
            base: Medida::Auto,
            hueco_filas: Medida::Px(0.0),
            hueco_columnas: Medida::Px(0.0),
            orden: 0,
        }
    }
}

fn ident(t: &[Token]) -> Option<String> {
    match t {
        [Token::Ident(x)] => Some(x.to_ascii_lowercase()),
        _ => None,
    }
}

fn numero(t: &[Token]) -> Option<f32> {
    match t {
        [Token::Numero(n)] if *n >= 0.0 => Some(*n as f32),
        _ => None,
    }
}

fn direccion(n: &str) -> Option<Direccion> {
    Some(match n {
        "row" => Direccion::Fila,
        "row-reverse" => Direccion::FilaInversa,
        "column" => Direccion::Columna,
        "column-reverse" => Direccion::ColumnaInversa,
        _ => return None,
    })
}

fn justificar(n: &str) -> Option<Justificar> {
    Some(match n {
        "flex-start" | "start" | "left" | "normal" => Justificar::Inicio,
        "flex-end" | "end" | "right" => Justificar::Fin,
        "center" => Justificar::Centro,
        "space-between" => Justificar::Entre,
        "space-around" => Justificar::Alrededor,
        "space-evenly" => Justificar::Uniforme,
        _ => return None,
    })
}

fn alinear(n: &str) -> Option<AlinearFlex> {
    Some(match n {
        "stretch" | "normal" => AlinearFlex::Estirar,
        "flex-start" | "start" | "self-start" => AlinearFlex::Inicio,
        "flex-end" | "end" | "self-end" => AlinearFlex::Fin,
        "center" => AlinearFlex::Centro,
        "baseline" | "first" | "last" => AlinearFlex::Base,
        _ => return None,
    })
}

/// Los atajos `flex`, `flex-flow` y `gap` → sus propiedades largas (`None` si no es uno de ellos).
pub(crate) fn expandir(prop: &str, valor: &[Token]) -> Option<Vec<(String, Vec<Token>)>> {
    let p = piezas(valor);
    let largo = |n: &str, v: &[Token]| (String::from(n), v.to_vec());
    let num = |n: f64| alloc::vec![Token::Numero(n)];
    Some(match prop {
        "flex" => {
            let (crecer, encoger, base): (Vec<Token>, Vec<Token>, Vec<Token>) =
                match (p.as_slice(), ident(valor).as_deref()) {
                    (_, Some("none")) => (num(0.0), num(0.0), alloc::vec![Token::Ident("auto".into())]),
                    (_, Some("auto")) => (num(1.0), num(1.0), alloc::vec![Token::Ident("auto".into())]),
                    (_, Some("initial")) => (num(0.0), num(1.0), alloc::vec![Token::Ident("auto".into())]),
                    ([a], _) if numero(a).is_some() => (a.to_vec(), num(1.0), alloc::vec![Token::Porcentaje(0.0)]),
                    ([a], _) => (num(1.0), num(1.0), a.to_vec()),
                    ([a, b], _) if numero(b).is_some() => (a.to_vec(), b.to_vec(), alloc::vec![Token::Porcentaje(0.0)]),
                    ([a, b], _) => (a.to_vec(), num(1.0), b.to_vec()),
                    ([a, b, c], _) => (a.to_vec(), b.to_vec(), c.to_vec()),
                    _ => return Some(Vec::new()),
                };
            alloc::vec![
                (String::from("flex-grow"), crecer),
                (String::from("flex-shrink"), encoger),
                (String::from("flex-basis"), base)
            ]
        }
        "flex-flow" => p
            .iter()
            .map(|x| {
                if ident(x).is_some_and(|n| direccion(&n).is_some()) {
                    largo("flex-direction", x)
                } else {
                    largo("flex-wrap", x)
                }
            })
            .collect(),
        "gap" | "grid-gap" => match p.as_slice() {
            [a] => alloc::vec![largo("row-gap", a), largo("column-gap", a)],
            [a, b] => alloc::vec![largo("row-gap", a), largo("column-gap", b)],
            _ => Vec::new(),
        },
        _ => return None,
    })
}

const PROPIAS: &[&str] = &[
    "flex-direction",
    "flex-wrap",
    "justify-content",
    "align-items",
    "align-self",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "row-gap",
    "column-gap",
    "order",
];

/// Si `prop` es de flexbox.
pub(crate) fn es_propia(prop: &str) -> bool {
    PROPIAS.contains(&prop)
}

/// Calcula una propiedad de flexbox; `false` si el valor no vale.
pub(crate) fn aplicar(f: &mut Flex, prop: &str, v: &[Token], c: &Contexto) -> bool {
    let n = ident(v);
    let n = n.as_deref();
    let hueco = |v: &[Token]| {
        if n == Some("normal") { Some(Medida::Px(0.0)) } else { medida(v, c).filter(|m| *m != Medida::Auto) }
    };
    match prop {
        "flex-direction" => n.and_then(direccion).map(|d| f.direccion = d).is_some(),
        "flex-wrap" => match n {
            Some("wrap" | "wrap-reverse") => {
                f.envolver = true;
                true
            }
            Some("nowrap") => {
                f.envolver = false;
                true
            }
            _ => false,
        },
        "justify-content" => n.and_then(justificar).map(|j| f.justificar = j).is_some(),
        "align-items" => n.and_then(alinear).map(|a| f.alinear_items = a).is_some(),
        "align-self" if n == Some("auto") => {
            f.alinear_propio = None;
            true
        }
        "align-self" => n.and_then(alinear).map(|a| f.alinear_propio = Some(a)).is_some(),
        "flex-grow" => numero(v).map(|x| f.crecer = x).is_some(),
        "flex-shrink" => numero(v).map(|x| f.encoger = x).is_some(),
        "flex-basis" if n == Some("content") => {
            f.base = Medida::Auto;
            true
        }
        "flex-basis" => medida(v, c).map(|m| f.base = m).is_some(),
        "row-gap" => hueco(v).map(|m| f.hueco_filas = m).is_some(),
        "column-gap" => hueco(v).map(|m| f.hueco_columnas = m).is_some(),
        "order" => match v {
            [Token::Numero(x)] if (*x as i32) as f64 == *x => {
                f.orden = *x as i32;
                true
            }
            _ => false,
        },
        _ => false,
    }
}

/// Copia una propiedad de flexbox de `de` (para `inherit` / `initial`).
pub(crate) fn copiar(f: &mut Flex, de: &Flex, prop: &str) {
    match prop {
        "flex-direction" => f.direccion = de.direccion,
        "flex-wrap" => f.envolver = de.envolver,
        "justify-content" => f.justificar = de.justificar,
        "align-items" => f.alinear_items = de.alinear_items,
        "align-self" => f.alinear_propio = de.alinear_propio,
        "flex-grow" => f.crecer = de.crecer,
        "flex-shrink" => f.encoger = de.encoger,
        "flex-basis" => f.base = de.base,
        "row-gap" => f.hueco_filas = de.hueco_filas,
        "column-gap" => f.hueco_columnas = de.hueco_columnas,
        "order" => f.orden = de.orden,
        _ => {}
    }
}
