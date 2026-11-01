//! `Math` y los métodos de los números (`toFixed`, `toPrecision`, `toString`).

use super::{arg, definir_nativa, definir_valor, nuevo_objeto};
use crate::llamadas::Llamada;
use crate::matematicas as mat;
use crate::{Motor, R_GLOBAL, R_NUMERO, operaciones as op};
use alloc::format;
use alloc::string::String;
use quimera_objetos::Valor;

const FLOOR: u32 = 100;
const CEIL: u32 = 101;
const ROUND: u32 = 102;
const ABS: u32 = 103;
const MAX: u32 = 104;
const MIN: u32 = 105;
const SQRT: u32 = 106;
const RANDOM: u32 = 107;
const LOG: u32 = 108;
const EXP: u32 = 109;
const POW: u32 = 110;
const TO_FIXED: u32 = 120;
const TO_PRECISION: u32 = 121;
const TO_STRING: u32 = 122;

pub(super) fn preparar(m: &mut Motor) {
    let math = nuevo_objeto(m, R_GLOBAL, "Math");
    let funciones = [
        ("floor", FLOOR),
        ("ceil", CEIL),
        ("round", ROUND),
        ("abs", ABS),
        ("max", MAX),
        ("min", MIN),
        ("sqrt", SQRT),
        ("random", RANDOM),
        ("log", LOG),
        ("exp", EXP),
        ("pow", POW),
    ];
    for (nombre, id) in funciones {
        definir_nativa(m, math, nombre, id);
    }
    definir_valor(m, math, "PI", Some(Valor::numero(core::f64::consts::PI)));
    definir_valor(m, math, "E", Some(Valor::numero(core::f64::consts::E)));
    definir_valor(m, math, "LN2", Some(Valor::numero(core::f64::consts::LN_2)));
    for (nombre, id) in [("toFixed", TO_FIXED), ("toPrecision", TO_PRECISION), ("toString", TO_STRING)] {
        definir_nativa(m, R_NUMERO, nombre, id);
    }
}

/// El argumento `i` como número (con `valueOf` si es un objeto).
fn numero_arg(m: &mut Motor, ll: &Llamada, i: usize) -> Result<f64, Valor> {
    let v = arg(m, ll, i);
    m.a_numero(v)
}

/// El máximo o el mínimo de los argumentos (NaN si alguno lo es).
fn extremo(m: &mut Motor, ll: &Llamada, maximo: bool) -> Result<f64, Valor> {
    let mut r = if maximo { f64::NEG_INFINITY } else { f64::INFINITY };
    for i in 0..ll.n {
        let x = numero_arg(m, ll, i)?;
        if x.is_nan() {
            return Ok(f64::NAN);
        }
        if (maximo && x > r) || (!maximo && x < r) {
            r = x;
        }
    }
    Ok(r)
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    let x = numero_arg(m, ll, 0)?;
    let r = match id {
        FLOOR => mat::suelo(x),
        CEIL => mat::techo(x),
        ROUND => mat::redondear(x),
        ABS => f64::from_bits(x.to_bits() & !(1 << 63)),
        MAX => extremo(m, ll, true)?,
        MIN => extremo(m, ll, false)?,
        SQRT => mat::raiz(x),
        LOG => mat::ln(x),
        EXP => mat::exp(x),
        POW => mat::potencia(x, numero_arg(m, ll, 1)?),
        RANDOM => {
            m.azar ^= m.azar << 13;
            m.azar ^= m.azar >> 7;
            m.azar ^= m.azar << 17;
            (m.azar >> 11) as f64 / (1u64 << 53) as f64
        }
        TO_FIXED | TO_PRECISION | TO_STRING => return metodo(m, id, ll),
        _ => return Ok(Valor::INDEFINIDO),
    };
    Ok(op::numero(r))
}

fn metodo(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    let este = m.pila(ll.este);
    let x = m.a_numero(este)?;
    let n = numero_arg(m, ll, 0)?;
    let texto = match id {
        TO_STRING => op::numero_a_texto(x),
        TO_FIXED => {
            let d = if n.is_nan() { 0 } else { n as i64 };
            if !(0..=100).contains(&d) {
                return Err(m.error("RangeError", "toFixed necesita entre 0 y 100 decimales"));
            }
            fijo(x, d as usize)
        }
        _ => {
            if arg(m, ll, 0) == Valor::INDEFINIDO {
                op::numero_a_texto(x)
            } else {
                let p = n as i64;
                if !(1..=100).contains(&p) {
                    return Err(m.error("RangeError", "toPrecision necesita entre 1 y 100 cifras"));
                }
                precision(x, p as usize)
            }
        }
    };
    Ok(m.mundo.cadena(&texto))
}

/// `Number.prototype.toFixed`.
pub(crate) fn fijo(x: f64, decimales: usize) -> String {
    if !x.is_finite() || !(-1e21..1e21).contains(&x) {
        return op::numero_a_texto(x);
    }
    let s = format!("{x:.decimales$}");
    // -0.0001.toFixed(2) es "-0.00" en JavaScript, pero (-0).toFixed(2) es "0.00".
    if x == 0.0 { s.trim_start_matches('-').into() } else { s }
}

/// `Number.prototype.toPrecision`: `p` cifras significativas, en notación fija o exponencial.
pub(crate) fn precision(x: f64, p: usize) -> String {
    if !x.is_finite() {
        return op::numero_a_texto(x);
    }
    if x == 0.0 {
        return fijo(0.0, p - 1);
    }
    // El exponente después de redondear a p cifras (9.99 con 2 cifras es 1.0e1).
    let exp_texto = format!("{x:.*e}", p - 1);
    let (mantisa, e) = exp_texto.split_once('e').unwrap_or((&exp_texto, "0"));
    let e: i64 = e.parse().unwrap_or(0);
    if e < -6 || e >= p as i64 {
        let signo = if e >= 0 { "+" } else { "-" };
        return format!("{mantisa}e{signo}{}", e.unsigned_abs());
    }
    fijo(x, (p as i64 - 1 - e).max(0) as usize)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn formatos_de_javascript() {
        assert_eq!(fijo(1.23456, 2), "1.23");
        assert_eq!(fijo(35302.0, 0), "35302");
        assert_eq!(fijo(-0.0, 2), "0.00");
        assert_eq!(precision(123.456, 3), "123");
        assert_eq!(precision(1.23456, 3), "1.23");
        assert_eq!(precision(0.000123, 2), "0.00012");
        assert_eq!(precision(123456.0, 3), "1.23e+5");
        assert_eq!(precision(9.99, 2), "10");
        assert_eq!(precision(0.0, 3), "0.00");
    }
}
