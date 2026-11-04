//! Propiedades personalizadas (`--color: #36c`) y `var()` (CSS Custom Properties, nivel 1).
//!
//! - Las `--x` se heredan. Cada elemento comparte el mapa de su padre (un `Rc`) y solo hace uno
//!   nuevo si declara alguna: casi ningún elemento paga memoria por ellas.
//! - Las `var()` dentro de una `--x` se resuelven al declararla (`--a: var(--b)`), con un límite
//!   de profundidad: un ciclo (`--a: var(--b); --b: var(--a)`) deja las dos sin valor, sin colgar.
//! - En las demás propiedades, `var()` se sustituye **antes** de expandir los atajos, así
//!   `margin: var(--m) 0` funciona. Si una variable no existe y no hay valor de reserva
//!   (`var(--x, 4px)`), la propiedad vale `unset`, como manda la especificación.

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::Token;

/// Nombre (`--x`, distingue mayúsculas) → valor ya resuelto. Nombres y valores van en `Rc`:
/// un elemento que declara una variable copia el mapa de su padre, y en YouTube son cientos.
/// Así la copia son punteros, no textos.
pub type Variables = BTreeMap<Rc<str>, Rc<[Token]>>;

/// Cuántas `var()` dentro de `var()` se siguen como mucho.
const PROFUNDIDAD: u8 = 16;

/// Si el valor usa alguna `var()`.
pub(crate) fn tiene_var(v: &[Token]) -> bool {
    v.iter().any(|t| matches!(t, Token::Funcion(f) if f.eq_ignore_ascii_case("var")))
}

/// Dónde se cierra la función que se abrió justo antes de `desde` (el índice de su `)`).
fn cierre(v: &[Token], desde: usize) -> Option<usize> {
    let mut nivel = 1usize;
    for (i, t) in v.iter().enumerate().skip(desde) {
        match t {
            Token::Funcion(_) | Token::Abre('(') => nivel += 1,
            Token::Cierra(')') => {
                nivel -= 1;
                if nivel == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Sin blancos al principio ni al final.
fn recortar(v: &[Token]) -> &[Token] {
    let inicio = v.iter().position(|t| *t != Token::Blanco).unwrap_or(v.len());
    let fin = v.iter().rposition(|t| *t != Token::Blanco).map_or(inicio, |i| i + 1);
    &v[inicio..fin.max(inicio)]
}

/// Sustituye cada `var()` de `v`. `None` si alguna no existe y no tiene valor de reserva (o si
/// hay demasiadas anidadas: un ciclo).
pub(crate) fn sustituir(v: &[Token], vars: &Variables, profundidad: u8) -> Option<Vec<Token>> {
    if profundidad == 0 {
        return None;
    }
    let mut salida = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let Token::Funcion(f) = &v[i] else {
            salida.push(v[i].clone());
            i += 1;
            continue;
        };
        if !f.eq_ignore_ascii_case("var") {
            salida.push(v[i].clone());
            i += 1;
            continue;
        }
        let fin = cierre(v, i + 1)?;
        let dentro = &v[i + 1..fin];
        let coma = dentro.iter().position(|t| *t == Token::Coma);
        let (nombre, reserva) = match coma {
            Some(c) => (recortar(&dentro[..c]), Some(recortar(&dentro[c + 1..]))),
            None => (recortar(dentro), None),
        };
        let valor: &[Token] = match (nombre, reserva) {
            ([Token::Ident(n)], _) if vars.contains_key(n.as_str()) => vars.get(n.as_str()).map(|v| &v[..])?,
            (_, Some(r)) => r,
            _ => return None,
        };
        salida.extend(sustituir(valor, vars, profundidad - 1)?);
        i = fin + 1;
    }
    Some(salida)
}

/// Las variables de un elemento: las heredadas de su padre más las que declara, en orden de
/// cascada (la última gana) y ya resueltas. Si no declara ninguna, comparte el mapa del padre.
pub(crate) fn del_elemento(padre: &Rc<Variables>, propias: Vec<(String, Vec<Token>)>) -> Rc<Variables> {
    if propias.is_empty() {
        return padre.clone();
    }
    // Una sola copia (de punteros): primero con los valores tal cual, luego se resuelven las
    // propias que usan `var()` contra ese mapa (como si se resolvieran a la vez).
    let mut mapa: Variables = (**padre).clone();
    let propias: Vec<(Rc<str>, Rc<[Token]>)> = propias.into_iter().map(|(n, v)| (Rc::from(n), Rc::from(v))).collect();
    for (nombre, valor) in &propias {
        mapa.insert(nombre.clone(), valor.clone());
    }
    let resueltas: Vec<(Rc<str>, Option<Vec<Token>>)> = propias
        .iter()
        .filter(|(n, _)| mapa.get(n).is_some_and(|v| tiene_var(v)))
        .map(|(n, _)| (n.clone(), mapa.get(n).and_then(|v| sustituir(v, &mapa, PROFUNDIDAD))))
        .collect();
    for (nombre, valor) in resueltas {
        // Inválida (no existe lo que usa, o un ciclo): la variable queda sin valor.
        match valor {
            Some(v) => _ = mapa.insert(nombre, Rc::from(v)),
            None => _ = mapa.remove(&nombre),
        }
    }
    Rc::new(mapa)
}

/// El valor de una declaración con sus `var()` sustituidas (`unset` si alguna no tiene valor).
pub(crate) fn valor_de(v: &[Token], vars: &Variables) -> Vec<Token> {
    sustituir(v, vars, PROFUNDIDAD).unwrap_or_else(|| alloc::vec![Token::Ident(String::from("unset"))])
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tokens(css: &str) -> Vec<Token> {
        quimera_css::declaraciones(&alloc::format!("x:{css}")).pop().map(|d| d.valor).unwrap_or_default()
    }

    #[test]
    fn sustituye_con_reserva_y_anidadas() {
        let padre = Rc::new(Variables::new());
        let vars = del_elemento(
            &padre,
            alloc::vec![
                (String::from("--a"), tokens("4px")),
                (String::from("--b"), tokens("var(--a) 2px")),
                (String::from("--ciclo1"), tokens("var(--ciclo2)")),
                (String::from("--ciclo2"), tokens("var(--ciclo1)")),
            ],
        );
        assert_eq!(valor_de(&tokens("var(--b)"), &vars), tokens("4px 2px"));
        assert_eq!(valor_de(&tokens("calc(var(--a) + 1px)"), &vars), tokens("calc(4px + 1px)"));
        assert_eq!(valor_de(&tokens("var(--nada, var(--a))"), &vars), tokens("4px"));
        assert_eq!(valor_de(&tokens("var(--nada)"), &vars), tokens("unset"));
        assert!(!vars.contains_key("--ciclo1") && !vars.contains_key("--ciclo2"), "un ciclo no cuelga");
        let hijo = del_elemento(&vars, Vec::new());
        assert!(Rc::ptr_eq(&hijo, &vars), "sin variables propias se comparte el mapa del padre");
    }
}
