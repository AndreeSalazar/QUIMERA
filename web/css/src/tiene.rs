//! `:has()`: un elemento casa si alguno de su alrededor casa con el selector relativo.
//!
//! - `:has(img)` o `:has(a b)` — algún descendiente (y en `a b`, también `a` está dentro).
//! - `:has(> img)` — algún hijo.
//! - `:has(+ p)` — el hermano siguiente; `:has(~ p)`, alguno de los siguientes.
//!
//! Como en Chrome, la lista no perdona: con un selector que no se entiende, la regla entera
//! se descarta. Un `:has()` dentro de otro no vale. Mirar los descendientes cuesta lo que
//! mide el subárbol, así que el resto del compuesto se mira antes (la etiqueta, el id...).

use crate::selectores::{Combinador, ElementoCss, Pseudo, Selector, complejo};
use crate::tokenizador::Token;
use alloc::vec::Vec;

/// Un selector relativo: `> img` es (`Hijo`, `img`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relativo {
    /// Cómo se relaciona con el elemento del `:has()`.
    pub combinador: Combinador,
    /// Lo que tiene que casar.
    pub selector: Selector,
}

/// Lee la lista de `:has(...)`. `None` si algo no vale.
pub(crate) fn leer(tokens: &[Token]) -> Option<Vec<Relativo>> {
    let mut nivel = 0i32;
    let mut v = Vec::new();
    for trozo in tokens.split(|t| {
        match t {
            Token::Funcion(_) | Token::Abre('(') => nivel += 1,
            Token::Cierra(')') => nivel -= 1,
            _ => {}
        }
        nivel == 0 && *t == Token::Coma
    }) {
        let inicio = trozo.iter().position(|t| *t != Token::Blanco)?;
        let trozo = &trozo[inicio..];
        let (combinador, resto) = match trozo.first() {
            Some(Token::Delim('>')) => (Combinador::Hijo, &trozo[1..]),
            Some(Token::Delim('+')) => (Combinador::Adyacente, &trozo[1..]),
            Some(Token::Delim('~')) => (Combinador::Hermano, &trozo[1..]),
            _ => (Combinador::Descendiente, trozo),
        };
        let selector = complejo(resto)?;
        let anidado = selector.partes.iter().flat_map(|p| &p.pseudos).any(|p| matches!(p, Pseudo::Tiene(_)));
        if anidado || selector.seudoelemento().is_some() {
            return None;
        }
        v.push(Relativo { combinador, selector });
    }
    (!v.is_empty()).then_some(v)
}

/// Si `x` está en relación `c` con `ancla` (`ancla > x`, `ancla x`, `ancla + x`, `ancla ~ x`).
pub(crate) fn relacion<E: ElementoCss>(ancla: E, x: E, c: Combinador) -> bool {
    let es = |y: E| y.mismo(&ancla);
    match c {
        Combinador::Hijo => x.padre().is_some_and(es),
        Combinador::Descendiente => core::iter::successors(x.padre(), |y| y.padre()).any(es),
        Combinador::Adyacente => x.anterior().is_some_and(es),
        Combinador::Hermano => core::iter::successors(x.anterior(), |y| y.anterior()).any(es),
    }
}

/// Los descendientes de `e`, en orden del documento.
fn descendientes<E: ElementoCss>(e: E) -> impl Iterator<Item = E> {
    let mut actual = e.primer_hijo();
    core::iter::from_fn(move || {
        let x = actual?;
        actual = x.primer_hijo().or_else(|| {
            let mut y = x;
            loop {
                if let Some(s) = y.siguiente() {
                    return Some(s);
                }
                y = y.padre().filter(|p| !p.mismo(&e))?;
            }
        });
        Some(x)
    })
}

/// Si `e` casa con `:has(l)`.
pub(crate) fn casa<E: ElementoCss>(l: &[Relativo], e: E) -> bool {
    l.iter().any(|r| match r.combinador {
        Combinador::Hijo | Combinador::Descendiente => {
            descendientes(e).any(|d| r.selector.casa_anclado(d, e, r.combinador))
        }
        // Lo de después del hermano también puede estar dentro de él (`:has(+ a b)`).
        Combinador::Adyacente | Combinador::Hermano => core::iter::successors(e.siguiente(), |s| s.siguiente())
            .flat_map(|s| core::iter::once(s).chain(descendientes(s)))
            .any(|d| r.selector.casa_anclado(d, e, r.combinador)),
    })
}
