//! `String`, los métodos de cadenas (`charAt`, `indexOf`, `slice`, `split`, `trim`...) y
//! `join`/`indexOf` de arreglos.
//!
//! Los índices de JavaScript cuentan unidades UTF-16; aquí se respetan convirtiendo a UTF-16
//! (las cadenas guardadas en UTF-16 llegarán con `quimera-cadenas`).

use super::{arg, constructor, definir_nativa};
use crate::llamadas::Llamada;
use crate::{Motor, R_ARREGLO, R_CADENA, operaciones as op};
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use quimera_objetos::{Clase, Tipo, Valor};

const CHAR_AT: u32 = 200;
const CHAR_CODE_AT: u32 = 201;
const INDEX_OF: u32 = 202;
const SLICE: u32 = 203;
const TO_STRING: u32 = 204;
const STRING: u32 = 205;
const FROM_CHAR_CODE: u32 = 206;
const SPLIT: u32 = 207;
const TO_UPPER: u32 = 208;
const TO_LOWER: u32 = 209;
const TRIM: u32 = 210;
const INCLUDES: u32 = 211;
const STARTS_WITH: u32 = 212;
const ENDS_WITH: u32 = 213;
const JOIN: u32 = 220;
const ARR_INDEX_OF: u32 = 221;

pub(super) fn preparar(m: &mut Motor) {
    let metodos = [
        ("charAt", CHAR_AT),
        ("charCodeAt", CHAR_CODE_AT),
        ("indexOf", INDEX_OF),
        ("slice", SLICE),
        ("substring", SLICE),
        ("toString", TO_STRING),
        ("split", SPLIT),
        ("toUpperCase", TO_UPPER),
        ("toLowerCase", TO_LOWER),
        ("trim", TRIM),
        ("includes", INCLUDES),
        ("startsWith", STARTS_WITH),
        ("endsWith", ENDS_WITH),
    ];
    for (nombre, id) in metodos {
        definir_nativa(m, R_CADENA, nombre, id);
    }
    let string = constructor(m, "String", STRING, R_CADENA);
    definir_nativa(m, string, "fromCharCode", FROM_CHAR_CODE);
    definir_nativa(m, R_ARREGLO, "join", JOIN);
    definir_nativa(m, R_ARREGLO, "indexOf", ARR_INDEX_OF);
}

/// Un índice relativo (`slice(-2)`) llevado a `[0, largo]`.
pub(super) fn relativo(x: f64, largo: usize) -> usize {
    let x = if x.is_nan() { 0.0 } else { x };
    let i = if x < 0.0 { largo as f64 + x } else { x };
    if i <= 0.0 {
        0
    } else if i >= largo as f64 {
        largo
    } else {
        i as usize
    }
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    match id {
        JOIN | ARR_INDEX_OF => return arreglo(m, id, ll),
        STRING => {
            let t = if ll.n == 0 {
                String::new()
            } else {
                let v = m.a_primitivo(arg(m, ll, 0))?;
                m.texto(v)
            };
            return Ok(m.mundo.cadena(&t));
        }
        FROM_CHAR_CODE => {
            let unidades: Vec<u16> = (0..ll.n).map(|i| op::a_numero(&m.mundo, arg(m, ll, i)) as i64 as u16).collect();
            return Ok(m.mundo.cadena(&String::from_utf16_lossy(&unidades)));
        }
        SPLIT if m.mundo.regexp(arg(m, ll, 0)).is_some() => return super::regexp::dividir(m, ll),
        SPLIT..=ENDS_WITH => return mas_texto(m, id, ll),
        _ => {}
    }
    let este = m.pila(ll.este);
    if matches!(este.tipo(), Tipo::Indefinido | Tipo::Nulo) {
        return Err(m.error("TypeError", "método de cadena sobre null o undefined"));
    }
    let s: Vec<u16> = op::a_texto(&m.mundo, este).encode_utf16().collect();
    let (a, b) = (op::a_numero(&m.mundo, arg(m, ll, 0)), op::a_numero(&m.mundo, arg(m, ll, 1)));
    let i = if a.is_nan() { 0 } else { a as i64 };
    Ok(match id {
        CHAR_AT => {
            let c = usize::try_from(i).ok().and_then(|i| s.get(i)).copied();
            let texto = c.map(|c| String::from_utf16_lossy(&[c])).unwrap_or_default();
            m.mundo.cadena(&texto)
        }
        CHAR_CODE_AT => match usize::try_from(i).ok().and_then(|i| s.get(i)) {
            Some(c) => Valor::entero(i32::from(*c)),
            None => Valor::numero(f64::NAN),
        },
        INDEX_OF => {
            let buscada: Vec<u16> = op::a_texto(&m.mundo, arg(m, ll, 0)).encode_utf16().collect();
            let desde = relativo(b, s.len()).min(s.len());
            let pos = (desde..=s.len().saturating_sub(buscada.len())).find(|&p| s[p..].starts_with(&buscada));
            Valor::entero(pos.map_or(-1, |p| p as i32))
        }
        SLICE => {
            let inicio = relativo(a, s.len());
            let fin = if arg(m, ll, 1) == Valor::INDEFINIDO { s.len() } else { relativo(b, s.len()) };
            let trozo = if inicio < fin { String::from_utf16_lossy(&s[inicio..fin]) } else { String::new() };
            m.mundo.cadena(&trozo)
        }
        _ => {
            let texto = String::from_utf16_lossy(&s);
            m.mundo.cadena(&texto)
        }
    })
}

fn arreglo(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    if m.clase_en(ll.este) != Some(Clase::Arreglo) {
        return Err(m.error("TypeError", "join/indexOf necesitan un arreglo"));
    }
    let a = m.pila(ll.este);
    let n = m.mundo.longitud(a).unwrap_or(0);
    if id == ARR_INDEX_OF {
        let buscado = arg(m, ll, 0);
        let pos = (0..n).find(|&i| m.mundo.leer_elemento(a, i).is_some_and(|e| op::estricto(e, buscado)));
        return Ok(Valor::entero(pos.map_or(-1, |p| p as i32)));
    }
    let sep = match arg(m, ll, 0) {
        Valor::INDEFINIDO => String::from(","),
        v => op::a_texto(&m.mundo, v),
    };
    let mut texto = String::new();
    for i in 0..n {
        if i > 0 {
            texto.push_str(&sep);
        }
        let e = m.mundo.leer_elemento(a, i).unwrap_or(Valor::INDEFINIDO);
        if !matches!(e.tipo(), Tipo::Indefinido | Tipo::Nulo) {
            texto.push_str(&op::a_texto(&m.mundo, e));
        }
    }
    Ok(m.mundo.cadena(&texto))
}

/// `split`, `toUpperCase`, `toLowerCase`, `trim`, `includes`, `startsWith`, `endsWith`.
fn mas_texto(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    let este = m.pila(ll.este);
    if matches!(este.tipo(), Tipo::Indefinido | Tipo::Nulo) {
        return Err(m.error("TypeError", "método de cadena sobre null o undefined"));
    }
    let s = m.texto(este);
    let a = arg(m, ll, 0);
    let otra = m.texto(a);
    let texto = match id {
        TO_UPPER => s.to_uppercase(),
        TO_LOWER => s.to_lowercase(),
        TRIM => s.trim().to_string(),
        INCLUDES => return Ok(Valor::booleano(s.contains(otra.as_str()))),
        STARTS_WITH => return Ok(Valor::booleano(s.starts_with(otra.as_str()))),
        ENDS_WITH => return Ok(Valor::booleano(s.ends_with(otra.as_str()))),
        _ => {
            let partes: Vec<String> = if a == Valor::INDEFINIDO {
                alloc::vec![s]
            } else if otra.is_empty() {
                s.encode_utf16().map(|c| String::from_utf16_lossy(&[c])).collect()
            } else {
                s.split(otra.as_str()).map(String::from).collect()
            };
            let valores: Vec<Valor> = partes.iter().map(|p| m.mundo.cadena(p)).collect();
            return Ok(m.mundo.crear_arreglo(&valores, m.pila(R_ARREGLO)));
        }
    };
    Ok(m.mundo.cadena(&texto))
}
