//! Condiciones de `@media`: `screen and (max-width: 600px)`, `print`, `(prefers-color-scheme: dark)`.

use crate::Opciones;
use alloc::string::String;

/// Si la condición se cumple en esta pantalla. Una lista con comas se cumple si se cumple
/// alguna; lo que no se entiende no se cumple.
pub(crate) fn cumple(condicion: &str, op: &Opciones) -> bool {
    let c = condicion.to_ascii_lowercase();
    c.split(',').any(|parte| consulta(parte.trim(), op))
}

fn consulta(q: &str, op: &Opciones) -> bool {
    if q.is_empty() {
        return true;
    }
    let (negada, q) = match q.strip_prefix("not ") {
        Some(r) => (true, r.trim()),
        None => (false, q.strip_prefix("only ").unwrap_or(q).trim()),
    };
    let si = q.split(" and ").all(|p| condicion_simple(p.trim(), op));
    si != negada
}

fn condicion_simple(p: &str, op: &Opciones) -> bool {
    match p {
        "all" | "screen" => true,
        "print" | "speech" | "tty" | "tv" | "projection" | "handheld" => false,
        _ => match p.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
            Some(rasgo) => rasgo_cumple(rasgo, op),
            None => false,
        },
    }
}

/// `min-width: 600px`, `orientation: portrait`, `prefers-color-scheme: dark`...
fn rasgo_cumple(rasgo: &str, op: &Opciones) -> bool {
    let Some((nombre, valor)) = rasgo.split_once(':') else {
        return matches!(rasgo.trim(), "color" | "hover" | "pointer");
    };
    let (nombre, valor) = (nombre.trim(), valor.trim());
    let px = || longitud(valor);
    match nombre {
        "min-width" => px().is_some_and(|v| op.ancho >= v),
        "max-width" => px().is_some_and(|v| op.ancho <= v),
        "min-height" => px().is_some_and(|v| op.alto >= v),
        "max-height" => px().is_some_and(|v| op.alto <= v),
        "orientation" => (valor == "portrait") == (op.alto >= op.ancho),
        "prefers-color-scheme" => valor == "light",
        "prefers-reduced-motion" | "prefers-contrast" | "forced-colors" => valor == "no-preference" || valor == "none",
        "hover" => valor == "hover",
        "pointer" | "any-pointer" => valor == "fine",
        _ => false,
    }
}

/// `600px`, `37.5em` (16 px cada uno), `40rem`.
fn longitud(v: &str) -> Option<f32> {
    let numero: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let n: f32 = numero.parse().ok()?;
    match &v[numero.len()..] {
        "px" | "" => Some(n),
        "em" | "rem" => Some(n * 16.0),
        _ => None,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn consultas() {
        let op = Opciones { ancho: 800.0, alto: 600.0 };
        assert!(cumple("screen and (min-width: 700px)", &op));
        assert!(!cumple("screen and (max-width: 600px)", &op));
        assert!(cumple("print, (max-width: 50em)", &op), "800 <= 50em (800px)");
        assert!(!cumple("print", &op));
        assert!(cumple("not print", &op));
        assert!(cumple("(orientation: landscape)", &op));
        assert!(!cumple("(prefers-color-scheme: dark)", &op));
        assert!(!cumple("(algo-raro: 3)", &op));
    }
}
