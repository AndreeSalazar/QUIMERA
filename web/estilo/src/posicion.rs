//! `position`, `top`/`right`/`bottom`/`left` (y su atajo `inset`) y `z-index`. Ninguna se hereda.

use crate::propiedades::{Contexto, medida};
use crate::valores::Medida;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::{Token, piezas};

/// `position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posicion {
    /// `static`: en el flujo, sin moverse.
    Estatica,
    /// `relative`: en el flujo y luego movido.
    Relativa,
    /// `absolute`: fuera del flujo, dentro de su antepasado posicionado.
    Absoluta,
    /// `fixed`: fuera del flujo, dentro de la ventana (por ahora, como `absolute` respecto a ella).
    Fija,
    /// `sticky` (por ahora, como `relative` sin moverse).
    Pegajosa,
}

impl Posicion {
    /// Si está posicionado (todo menos `static`): sus hijos `absolute` se colocan dentro de él.
    pub fn posicionado(self) -> bool {
        self != Posicion::Estatica
    }

    /// Si sale del flujo (`absolute` y `fixed`).
    pub fn fuera_del_flujo(self) -> bool {
        matches!(self, Posicion::Absoluta | Posicion::Fija)
    }
}

/// Lo de la posición en un elemento.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colocacion {
    /// `position`
    pub posicion: Posicion,
    /// `top`, `right`, `bottom`, `left` (`Auto` si no se dan).
    pub desplazar: [Medida; 4],
    /// `z-index` (`None` = `auto`).
    pub z: Option<i32>,
}

impl Default for Colocacion {
    fn default() -> Self {
        Colocacion { posicion: Posicion::Estatica, desplazar: [Medida::Auto; 4], z: None }
    }
}

const LADOS: [&str; 4] = ["top", "right", "bottom", "left"];

/// Si la propiedad es de la posición.
pub(crate) fn es_propia(p: &str) -> bool {
    matches!(p, "position" | "z-index") || LADOS.contains(&p)
}

/// `inset: 1px 2px` → `top`, `right`, `bottom`, `left`.
pub(crate) fn expandir(propiedad: &str, valor: &[Token]) -> Option<Vec<(String, Vec<Token>)>> {
    if propiedad != "inset" {
        return None;
    }
    let p = piezas(valor);
    let cuatro: [&[Token]; 4] = match p.as_slice() {
        [a] => [a, a, a, a],
        [a, b] => [a, b, a, b],
        [a, b, c] => [a, b, c, b],
        [a, b, c, d] => [a, b, c, d],
        _ => return Some(Vec::new()),
    };
    Some(LADOS.iter().zip(cuatro).map(|(l, v)| (String::from(*l), v.to_vec())).collect())
}

fn posicion(n: &str) -> Option<Posicion> {
    Some(match n {
        "static" => Posicion::Estatica,
        "relative" => Posicion::Relativa,
        "absolute" => Posicion::Absoluta,
        "fixed" => Posicion::Fija,
        "sticky" | "-webkit-sticky" => Posicion::Pegajosa,
        _ => return None,
    })
}

/// Calcula una propiedad sobre `c`; devuelve si el valor era válido.
pub(crate) fn aplicar(c: &mut Colocacion, prop: &str, v: &[Token], ctx: &Contexto) -> bool {
    let n = match v {
        [Token::Ident(x)] => Some(x.to_ascii_lowercase()),
        _ => None,
    };
    match prop {
        "position" => n.as_deref().and_then(posicion).map(|p| c.posicion = p).is_some(),
        "z-index" => match (v, n.as_deref()) {
            (_, Some("auto")) => {
                c.z = None;
                true
            }
            ([Token::Numero(x)], _) if f64::from(*x as i32) == *x => {
                c.z = Some(*x as i32);
                true
            }
            _ => false,
        },
        lado => {
            let Some(i) = LADOS.iter().position(|l| *l == lado) else { return false };
            medida(v, ctx).map(|m| c.desplazar[i] = m).is_some()
        }
    }
}

/// Copia una propiedad de `de` (para `inherit` / `initial`).
pub(crate) fn copiar(c: &mut Colocacion, de: &Colocacion, prop: &str) {
    match prop {
        "position" => c.posicion = de.posicion,
        "z-index" => c.z = de.z,
        lado => {
            if let Some(i) = LADOS.iter().position(|l| *l == lado) {
                c.desplazar[i] = de.desplazar[i];
            }
        }
    }
}
