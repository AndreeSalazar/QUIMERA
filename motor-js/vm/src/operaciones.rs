//! Conversiones y operadores de JavaScript (ECMAScript 2024, §7.1 y §13).
//!
//! Los enteros tienen camino rápido: `1 + 2` con dos `Entero` no toca ningún `f64`. Lo que
//! aún falta (convertir objetos llamando a `valueOf`/`toString`) da su resultado por defecto.

use alloc::format;
use alloc::string::String;
use quimera_frontend::bytecode::OpBin;
use quimera_objetos::{Clase, Mundo, Tipo, Valor};

/// `ToNumber`.
pub fn a_numero(w: &Mundo, v: Valor) -> f64 {
    match v.tipo() {
        Tipo::Numero | Tipo::Entero => v.como_numero().unwrap_or(f64::NAN),
        Tipo::Indefinido => f64::NAN,
        Tipo::Nulo => 0.0,
        Tipo::Booleano => f64::from(u8::from(v.como_booleano() == Some(true))),
        Tipo::Cadena => texto_a_numero(w.texto(v).unwrap_or("")),
        Tipo::Simbolo | Tipo::Referencia => f64::NAN,
    }
}

fn es_blanco(c: char) -> bool {
    c.is_whitespace() || c == '\u{FEFF}'
}

/// `StringToNumber`: blancos alrededor, `""` es 0, `0x..`, `Infinity`; lo demás, decimal o NaN.
pub fn texto_a_numero(s: &str) -> f64 {
    let s = s.trim_matches(es_blanco);
    if s.is_empty() {
        return 0.0;
    }
    let base = match s.get(..2) {
        Some("0x" | "0X") => 16,
        Some("0o" | "0O") => 8,
        Some("0b" | "0B") => 2,
        _ => 10,
    };
    if base != 10 {
        let digitos = &s[2..];
        if digitos.is_empty() || !digitos.chars().all(|c| c.is_digit(base)) {
            return f64::NAN;
        }
        return digitos.chars().fold(0.0, |n, d| n * base as f64 + d.to_digit(base).unwrap_or(0) as f64);
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    // Rust acepta "inf", "nan", "infinity"...; JavaScript no.
    if s.chars().any(|c| c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E')) {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `ToBoolean`.
pub fn a_booleano(w: &Mundo, v: Valor) -> bool {
    match v.verdad() {
        Some(b) => b,
        None => !w.texto(v).unwrap_or("").is_empty(),
    }
}

/// `Number::toString`: enteros sin decimales, `NaN`, `Infinity`, `-0` como `0`.
pub fn numero_a_texto(n: f64) -> String {
    if n.is_nan() {
        return String::from("NaN");
    }
    if n.is_infinite() {
        return String::from(if n > 0.0 { "Infinity" } else { "-Infinity" });
    }
    if n == 0.0 {
        return String::from("0");
    }
    if n > -9.0e15 && n < 9.0e15 && (n as i64) as f64 == n {
        return format!("{}", n as i64);
    }
    format!("{n}")
}

/// `ToString` (los objetos dan su forma por defecto; `toString` propio llegará más adelante).
pub fn a_texto(w: &Mundo, v: Valor) -> String {
    match v.tipo() {
        Tipo::Numero | Tipo::Entero => numero_a_texto(v.como_numero().unwrap_or(f64::NAN)),
        Tipo::Indefinido => String::from("undefined"),
        Tipo::Nulo => String::from("null"),
        Tipo::Booleano => String::from(if v.como_booleano() == Some(true) { "true" } else { "false" }),
        Tipo::Cadena => String::from(w.texto(v).unwrap_or("")),
        Tipo::Simbolo => String::from("Symbol()"),
        Tipo::Referencia => match w.clase(v) {
            Some(Clase::Arreglo) => unir(w, v),
            Some(Clase::Funcion | Clase::Nativa) => String::from("function () { [código] }"),
            _ => String::from("[object Object]"),
        },
    }
}

fn unir(w: &Mundo, arr: Valor) -> String {
    let n = w.longitud(arr).unwrap_or(0);
    let mut s = String::new();
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        let e = w.leer_elemento(arr, i).unwrap_or(Valor::INDEFINIDO);
        if !matches!(e.tipo(), Tipo::Indefinido | Tipo::Nulo) {
            s.push_str(&a_texto(w, e));
        }
    }
    s
}

/// `typeof`.
pub fn tipo_de(w: &Mundo, v: Valor) -> &'static str {
    match v.tipo() {
        Tipo::Numero | Tipo::Entero => "number",
        Tipo::Indefinido => "undefined",
        Tipo::Nulo => "object",
        Tipo::Booleano => "boolean",
        Tipo::Cadena => "string",
        Tipo::Simbolo => "symbol",
        Tipo::Referencia => match w.clase(v) {
            Some(Clase::Funcion | Clase::Nativa) => "function",
            _ => "object",
        },
    }
}

/// `===`: números por valor (`1 === 1.0`, `NaN !== NaN`, `0 === -0`), cadenas internadas por número.
pub fn estricto(a: Valor, b: Valor) -> bool {
    if let (Some(x), Some(y)) = (a.como_numero(), b.como_numero()) {
        return x == y;
    }
    a == b
}

/// `==`.
pub fn flexible(w: &Mundo, a: Valor, b: Valor) -> bool {
    let nulo = |v: Valor| matches!(v.tipo(), Tipo::Nulo | Tipo::Indefinido);
    if nulo(a) || nulo(b) {
        return nulo(a) && nulo(b);
    }
    let primitivo = |v: Valor| !matches!(v.tipo(), Tipo::Referencia);
    if a.tipo() == b.tipo() || !(primitivo(a) && primitivo(b)) {
        return estricto(a, b);
    }
    a_numero(w, a) == a_numero(w, b)
}

/// `<`, `<=`, `>`, `>=`: cadenas por orden, lo demás como números (NaN siempre da falso).
pub fn comparar(w: &Mundo, op: OpBin, a: Valor, b: Valor) -> bool {
    if let (Some(x), Some(y)) = (a.como_entero(), b.como_entero()) {
        return match op {
            OpBin::Menor => x < y,
            OpBin::MenorIgual => x <= y,
            OpBin::Mayor => x > y,
            _ => x >= y,
        };
    }
    if let (Some(x), Some(y)) = (w.texto(a), w.texto(b)) {
        return match op {
            OpBin::Menor => x < y,
            OpBin::MenorIgual => x <= y,
            OpBin::Mayor => x > y,
            _ => x >= y,
        };
    }
    let (x, y) = (a_numero(w, a), a_numero(w, b));
    match op {
        OpBin::Menor => x < y,
        OpBin::MenorIgual => x <= y,
        OpBin::Mayor => x > y,
        _ => x >= y,
    }
}

/// Un número como valor: entero si lo es exactamente (y no es -0).
pub fn numero(n: f64) -> Valor {
    let i = n as i32;
    if i as f64 == n && !(n == 0.0 && n.is_sign_negative()) { Valor::entero(i) } else { Valor::numero(n) }
}

/// `ToInt32`.
pub fn a_int32(w: &Mundo, v: Valor) -> i32 {
    if let Some(i) = v.como_entero() {
        return i;
    }
    let n = a_numero(w, v);
    if !n.is_finite() {
        return 0;
    }
    // Truncar y reducir módulo 2^32 (los f64 de más de 2^63 ya son múltiplos de 2^32).
    if n.abs_seguro() < 9.2e18 { (n as i64) as i32 } else { 0 }
}

trait AbsSeguro {
    fn abs_seguro(self) -> f64;
}

impl AbsSeguro for f64 {
    /// `abs` sin `std`.
    fn abs_seguro(self) -> f64 {
        f64::from_bits(self.to_bits() & !(1u64 << 63))
    }
}

/// `x ** y` con exponente entero, por cuadrados (sin `std` no hay `powf`).
pub fn potencia(x: f64, y: f64) -> Option<f64> {
    if y.is_nan() || (x.abs_seguro() == 1.0 && y.is_infinite()) {
        return Some(f64::NAN);
    }
    if !(y > -2147483648.0 && y < 2147483648.0) || (y as i32) as f64 != y {
        return None;
    }
    let (mut base, mut e, mut r) = (x, (y as i64).unsigned_abs(), 1.0f64);
    while e > 0 {
        if e & 1 == 1 {
            r *= base;
        }
        base *= base;
        e >>= 1;
    }
    Some(if y < 0.0 { 1.0 / r } else { r })
}

/// Aritmética y bits: `None` significa "operador que no es de este grupo".
pub fn aritmetica(w: &Mundo, op: OpBin, a: Valor, b: Valor) -> Option<Valor> {
    if let (Some(x), Some(y)) = (a.como_entero(), b.como_entero()) {
        let rapido = match op {
            OpBin::Suma => x.checked_add(y),
            OpBin::Resta => x.checked_sub(y),
            // 0 * -5 es -0 en JavaScript: ese caso va por f64.
            OpBin::Mul => x.checked_mul(y).filter(|r| *r != 0 || (x >= 0 && y >= 0)),
            OpBin::Resto if y > 0 && x >= 0 => Some(x % y),
            _ => None,
        };
        if let Some(r) = rapido {
            return Some(Valor::entero(r));
        }
    }
    let bits = |f: fn(i32, i32) -> i32| Some(Valor::entero(f(a_int32(w, a), a_int32(w, b))));
    match op {
        OpBin::YBits => return bits(|x, y| x & y),
        OpBin::OBits => return bits(|x, y| x | y),
        OpBin::XorBits => return bits(|x, y| x ^ y),
        OpBin::Izq => return bits(|x, y| x.wrapping_shl(y as u32 & 31)),
        OpBin::Der => return bits(|x, y| x >> (y as u32 & 31)),
        OpBin::DerSinSigno => {
            let r = (a_int32(w, a) as u32) >> (a_int32(w, b) as u32 & 31);
            return Some(numero(f64::from(r)));
        }
        _ => {}
    }
    let (x, y) = (a_numero(w, a), a_numero(w, b));
    let r = match op {
        OpBin::Suma => x + y,
        OpBin::Resta => x - y,
        OpBin::Mul => x * y,
        OpBin::Div => x / y,
        OpBin::Resto => x % y,
        _ => return None,
    };
    Some(numero(r))
}
