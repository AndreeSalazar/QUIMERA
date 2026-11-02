//! `Math` y los métodos de los números (`toFixed`, `toPrecision`, `toString`).

use super::{arg, constructor, definir_nativa, definir_valor, nuevo_objeto};
use crate::llamadas::Llamada;
use crate::matematicas as mat;
use crate::trigonometria as trig;
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
const PARSE_INT: u32 = 130;
const PARSE_FLOAT: u32 = 131;
const IS_NAN: u32 = 132;
const IS_FINITE: u32 = 133;
pub(super) const NUMBER: u32 = 134;
const SIN: u32 = 111;
const COS: u32 = 112;
const TAN: u32 = 113;
const ATAN: u32 = 114;
const ATAN2: u32 = 115;
const ASIN: u32 = 116;
const ACOS: u32 = 117;
const ASINH: u32 = 118;
const ACOSH: u32 = 119;
const ATANH: u32 = 123;

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
        ("sin", SIN),
        ("cos", COS),
        ("tan", TAN),
        ("atan", ATAN),
        ("atan2", ATAN2),
        ("asin", ASIN),
        ("acos", ACOS),
        ("asinh", ASINH),
        ("acosh", ACOSH),
        ("atanh", ATANH),
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
    for (nombre, id) in
        [("parseInt", PARSE_INT), ("parseFloat", PARSE_FLOAT), ("isNaN", IS_NAN), ("isFinite", IS_FINITE)]
    {
        definir_nativa(m, R_GLOBAL, nombre, id);
    }
    constructor(m, "Number", NUMBER, R_NUMERO);
}

/// `parseInt(texto, base)`: espacios, signo, `0x` y tantas cifras válidas como haya.
fn parse_int(texto: &str, base: f64) -> f64 {
    let t = texto.trim_start();
    let (negativo, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut base = if base.is_nan() { 0 } else { base as i64 as u32 };
    let hex = t.len() >= 2 && (t.starts_with("0x") || t.starts_with("0X"));
    let t = if hex && (base == 0 || base == 16) {
        base = 16;
        &t[2..]
    } else {
        t
    };
    let base = if base == 0 { 10 } else { base };
    if !(2..=36).contains(&base) {
        return f64::NAN;
    }
    let mut r = 0.0;
    let mut alguna = false;
    for c in t.chars() {
        let Some(d) = c.to_digit(base) else { break };
        r = r * base as f64 + d as f64;
        alguna = true;
    }
    if !alguna {
        return f64::NAN;
    }
    if negativo { -r } else { r }
}

/// `parseFloat(texto)`: el número más largo al principio del texto.
fn parse_float(texto: &str) -> f64 {
    let t = texto.trim_start();
    let b = t.as_bytes();
    let mut fin = 0;
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    if t[i..].starts_with("Infinity") {
        return if b.first() == Some(&b'-') { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let (mut punto, mut cifras) = (false, false);
    while i < b.len() {
        match b[i] {
            b'0'..=b'9' => cifras = true,
            b'.' if !punto => punto = true,
            b'e' | b'E' if cifras => {
                let mut j = i + 1;
                if matches!(b.get(j), Some(b'+' | b'-')) {
                    j += 1;
                }
                if b.get(j).is_some_and(u8::is_ascii_digit) {
                    while b.get(j).is_some_and(u8::is_ascii_digit) {
                        j += 1;
                    }
                    fin = j;
                }
                break;
            }
            _ => break,
        }
        i += 1;
        if cifras {
            fin = i;
        }
    }
    if cifras { op::texto_a_numero(&t[..fin]) } else { f64::NAN }
}

/// Un número en otra base (`(255).toString(16)`), con decimales si los tiene.
fn en_base(x: f64, base: u32) -> String {
    if base == 10 || !x.is_finite() {
        return op::numero_a_texto(x);
    }
    let digito = |d: u32| core::char::from_digit(d, base).unwrap_or('0');
    let (negativo, x) = (x < 0.0, if x < 0.0 { -x } else { x });
    let mut entera = mat::suelo(x);
    let mut frac = x - entera;
    let mut cifras = alloc::vec::Vec::new();
    loop {
        let resto = entera - mat::suelo(entera / base as f64) * base as f64;
        cifras.push(digito(resto as u32));
        entera = mat::suelo(entera / base as f64);
        if entera < 1.0 {
            break;
        }
    }
    let mut s: String = if negativo { String::from("-") } else { String::new() };
    s.extend(cifras.iter().rev());
    if frac > 0.0 {
        s.push('.');
        for _ in 0..52 {
            frac *= base as f64;
            let d = mat::suelo(frac);
            s.push(digito(d as u32));
            frac -= d;
            if frac == 0.0 {
                break;
            }
        }
    }
    s
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
    if matches!(id, PARSE_INT | PARSE_FLOAT) {
        let texto = m.texto(arg(m, ll, 0));
        let r = if id == PARSE_INT { parse_int(&texto, numero_arg(m, ll, 1)?) } else { parse_float(&texto) };
        return Ok(op::numero(r));
    }
    if id == NUMBER && ll.n == 0 {
        return Ok(Valor::entero(0));
    }
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
        SIN => trig::seno(x),
        COS => trig::coseno(x),
        TAN => trig::tangente(x),
        ATAN => trig::arcotangente(x),
        ATAN2 => trig::arcotangente2(x, numero_arg(m, ll, 1)?),
        ASIN => trig::arcoseno(x),
        ACOS => trig::arcocoseno(x),
        ASINH => trig::arcoseno_h(x),
        ACOSH => trig::arcocoseno_h(x),
        ATANH => trig::arcotangente_h(x),
        RANDOM => {
            m.azar ^= m.azar << 13;
            m.azar ^= m.azar >> 7;
            m.azar ^= m.azar << 17;
            (m.azar >> 11) as f64 / (1u64 << 53) as f64
        }
        TO_FIXED | TO_PRECISION | TO_STRING => return metodo(m, id, ll),
        IS_NAN => return Ok(Valor::booleano(x.is_nan())),
        IS_FINITE => return Ok(Valor::booleano(x.is_finite())),
        NUMBER if ll.construir => return Ok(super::envoltorio(m, ll, R_NUMERO, op::numero(x), None)),
        NUMBER => x,
        _ => return Ok(Valor::INDEFINIDO),
    };
    Ok(op::numero(r))
}

fn metodo(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    let este = m.pila(ll.este);
    let x = m.a_numero(este)?;
    let n = numero_arg(m, ll, 0)?;
    let texto = match id {
        TO_STRING => {
            let base = if arg(m, ll, 0) == Valor::INDEFINIDO { 10.0 } else { n };
            if !(2.0..=36.0).contains(&base) {
                return Err(m.error("RangeError", "toString necesita una base entre 2 y 36"));
            }
            en_base(x, base as u32)
        }
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
