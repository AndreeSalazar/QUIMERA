//! `encodeURIComponent`, `encodeURI`, `decodeURIComponent`, `decodeURI`, `btoa` y `atob`.
//!
//! El texto va en UTF-8 al codificar (`ñ` → `%C3%B1`), como manda la especificación.
//! `decodeURI` deja codificados los signos que separan partes de una URL (`%2F` sigue siendo
//! `%2F`). Una secuencia mal formada lanza `URIError`; un `atob` con caracteres que no son
//! base64, `InvalidCharacterError`.

use super::{arg, definir_nativa};
use crate::llamadas::Llamada;
use crate::{Motor, R_GLOBAL};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_objetos::Valor;

const ENCODE_COMPONENT: u32 = 700;
const ENCODE: u32 = 701;
const DECODE_COMPONENT: u32 = 702;
const DECODE: u32 = 703;
const BTOA: u32 = 704;
const ATOB: u32 = 705;

/// Lo que `encodeURIComponent` deja tal cual.
const SIN_CODIFICAR: &str = "-_.!~*'()";
/// Lo que además deja `encodeURI` (y no decodifica `decodeURI`).
const RESERVADOS: &str = ";,/?:@&=+$#";

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) fn preparar(m: &mut Motor) {
    for (nombre, id) in [
        ("encodeURIComponent", ENCODE_COMPONENT),
        ("encodeURI", ENCODE),
        ("decodeURIComponent", DECODE_COMPONENT),
        ("decodeURI", DECODE),
        ("btoa", BTOA),
        ("atob", ATOB),
    ] {
        definir_nativa(m, R_GLOBAL, nombre, id);
    }
}

fn codificar(s: &str, deja: &str) -> String {
    let mut r = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || deja.contains(c) {
            r.push(c);
        } else {
            let mut b = [0u8; 4];
            for byte in c.encode_utf8(&mut b).bytes() {
                r.push_str(&alloc::format!("%{byte:02X}"));
            }
        }
    }
    r
}

/// Decodifica `%XX` (como UTF-8). Lo que esté en `conserva` se deja codificado.
fn decodificar(s: &str, conserva: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut bytes: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = core::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            let n = u8::from_str_radix(h, 16).ok()?;
            if n < 0x80 && conserva.contains(char::from(n)) {
                bytes.extend_from_slice(&b[i..i + 3]);
            } else {
                bytes.push(n);
            }
            i += 3;
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(bytes).ok()
}

fn btoa(s: &str) -> Option<String> {
    // Cada carácter es un byte (Latin-1); uno mayor no cabe.
    let bytes: Vec<u8> = s.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect::<Option<_>>()?;
    let mut r = String::new();
    for trozo in bytes.chunks(3) {
        let n = (u32::from(trozo[0]) << 16)
            | (u32::from(*trozo.get(1).unwrap_or(&0)) << 8)
            | u32::from(*trozo.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= trozo.len() {
                r.push(char::from(BASE64[((n >> (18 - 6 * k)) & 63) as usize]));
            } else {
                r.push('=');
            }
        }
    }
    Some(r)
}

fn atob(s: &str) -> Option<String> {
    let limpio: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let sin_relleno = limpio.strip_suffix(b"==").or_else(|| limpio.strip_suffix(b"=")).unwrap_or(&limpio);
    let mut bits = 0u32;
    let mut n = 0;
    let mut r = String::new();
    for &c in sin_relleno {
        let v = BASE64.iter().position(|&x| x == c)? as u32;
        bits = (bits << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            r.push(char::from(((bits >> n) & 0xFF) as u8));
        }
    }
    Some(r)
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    let texto = m.texto(arg(m, ll, 0));
    let resultado = match id {
        ENCODE_COMPONENT => Some(codificar(&texto, SIN_CODIFICAR)),
        ENCODE => Some(codificar(&texto, &alloc::format!("{SIN_CODIFICAR}{RESERVADOS}"))),
        DECODE_COMPONENT => decodificar(&texto, ""),
        DECODE => decodificar(&texto, RESERVADOS),
        BTOA => btoa(&texto),
        _ => atob(&texto),
    };
    match resultado {
        Some(s) => Ok(m.mundo.cadena(&s)),
        None if matches!(id, BTOA | ATOB) => Err(m.error("InvalidCharacterError", "texto no válido para base64")),
        None => Err(m.error("URIError", "secuencia URI mal formada")),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn uri_y_base64() {
        assert_eq!(codificar("a b&ñ/€", SIN_CODIFICAR), "a%20b%26%C3%B1%2F%E2%82%AC");
        assert_eq!(codificar("https://x.com/a b?q=1#f", "-_.!~*'();,/?:@&=+$#"), "https://x.com/a%20b?q=1#f");
        assert_eq!(decodificar("a%20b%26%C3%B1%2F", "").as_deref(), Some("a b&ñ/"));
        assert_eq!(decodificar("a%2Fb%20c", RESERVADOS).as_deref(), Some("a%2Fb c"));
        assert_eq!(decodificar("%E0%A4%A", ""), None);
        assert_eq!(btoa("QUIMERA!").as_deref(), Some("UVVJTUVSQSE="));
        assert_eq!(btoa("ab").as_deref(), Some("YWI="));
        assert_eq!(atob("UVVJTUVSQSE=").as_deref(), Some("QUIMERA!"));
        assert_eq!(btoa("€"), None, "fuera de Latin-1");
        assert_eq!(atob("@@"), None);
    }
}
