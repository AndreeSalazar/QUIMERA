//! Parser de JavaScript: tokens → árbol de sintaxis.
//!
//! Descenso recursivo para las sentencias y precedencia por niveles para las expresiones,
//! con la inserción automática de `;` de ECMAScript. La estructura sigue la gramática de
//! la especificación, como el parser de Boa, pero sin depender de ningún crate.

mod expresiones;
mod sentencias;

use crate::ast::Sent;
use crate::fuente::{Pos, Resultado, error};
use crate::lexico::{self, Tipo, Token};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Analiza un programa completo.
pub fn analizar(fuente: &str) -> Resultado<Vec<Sent>> {
    let mut p = Parser { toks: lexico::tokens(fuente)?, i: 0, sin_in: false };
    let mut programa = Vec::new();
    while p.ver() != &Tipo::Fin {
        programa.push(p.sentencia()?);
    }
    Ok(programa)
}

pub(crate) struct Parser {
    toks: Vec<Token>,
    i: usize,
    /// Dentro del inicio de un `for`, `in` no es un operador (`for (x in o)`).
    sin_in: bool,
}

impl Parser {
    fn token(&self, n: usize) -> &Token {
        let i = (self.i + n).min(self.toks.len() - 1);
        &self.toks[i]
    }

    fn ver(&self) -> &Tipo {
        &self.token(0).tipo
    }

    fn pos(&self) -> Pos {
        self.token(0).pos
    }

    fn avanzar(&mut self) -> Token {
        let t = self.token(0).clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }

    /// El token actual es este signo o esta palabra reservada.
    fn es(&self, s: &str) -> bool {
        self.es_en(0, s)
    }

    fn es_en(&self, n: usize, s: &str) -> bool {
        matches!(&self.token(n).tipo, Tipo::Signo(x) | Tipo::Clave(x) if *x == s)
    }

    /// El token actual es este identificador contextual (`of`, `get`, `async`...).
    fn es_palabra(&self, s: &str) -> bool {
        matches!(self.ver(), Tipo::Ident(x) if x == s)
    }

    fn tomar(&mut self, s: &str) -> bool {
        if self.es(s) {
            self.avanzar();
            true
        } else {
            false
        }
    }

    fn esperar(&mut self, s: &str) -> Resultado<()> {
        if self.tomar(s) {
            return Ok(());
        }
        let encontrado = describir(self.ver());
        error(self.pos(), format!("se esperaba `{s}` y hay {encontrado}"))
    }

    fn ident(&mut self) -> Resultado<String> {
        match self.ver().clone() {
            Tipo::Ident(n) => {
                self.avanzar();
                Ok(n)
            }
            otro => error(self.pos(), format!("se esperaba un nombre y hay {}", describir(&otro))),
        }
    }

    /// Fin de sentencia: `;`, o se inserta uno antes de `}`, del final o tras un salto de línea.
    fn fin_sentencia(&mut self) -> Resultado<()> {
        if self.tomar(";") || self.es("}") || self.ver() == &Tipo::Fin || self.token(0).salto_antes {
            return Ok(());
        }
        let encontrado = describir(self.ver());
        error(self.pos(), format!("se esperaba `;` y hay {encontrado}"))
    }

    /// Índice del token que cierra el `(`, `[` o `{` de la posición `n`.
    fn cierre_desde(&self, n: usize) -> Option<usize> {
        let mut nivel = 0i32;
        for j in self.i + n..self.toks.len() {
            match &self.toks[j].tipo {
                Tipo::Signo("(" | "[" | "{") => nivel += 1,
                Tipo::Signo(")" | "]" | "}") => {
                    nivel -= 1;
                    if nivel == 0 {
                        return Some(j - self.i);
                    }
                }
                Tipo::Fin => return None,
                _ => {}
            }
        }
        None
    }
}

/// Cómo se nombra un token en un mensaje de error.
fn describir(t: &Tipo) -> String {
    match t {
        Tipo::Ident(n) => format!("el nombre `{n}`"),
        Tipo::Clave(k) => format!("la palabra `{k}`"),
        Tipo::Numero(n) => format!("el número {n}"),
        Tipo::Cadena(_) => String::from("una cadena"),
        Tipo::Signo(s) => format!("`{s}`"),
        Tipo::Regex(p, b) => format!("la expresión regular /{p}/{b}"),
        Tipo::Fin => String::from("el final del texto"),
    }
}

/// Un número como lo escribe JavaScript al usarlo de clave (`1` y no `1.0`).
pub(crate) fn numero_a_clave(n: f64) -> String {
    // Sin `std` no hay `fract()`: un entero exacto vuelve igual al convertirlo de ida y vuelta.
    if n > -1e15 && n < 1e15 && (n as i64) as f64 == n { format!("{}", n as i64) } else { format!("{n}") }
}
