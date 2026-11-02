//! Análisis del patrón: texto → árbol (`Nodo`), con la sintaxis de JavaScript (incluido el
//! anexo B de los navegadores: `{` suelto es un carácter, `\8` es un `8`...).

use crate::Error;
use crate::clases::{Abreviada, Clase, Constructor};
use alloc::boxed::Box;
use alloc::vec::Vec;

/// Profundidad máxima de paréntesis: el análisis es recursivo y no debe agotar la pila.
const MAX_ANIDADO: usize = 256;

/// Un trozo del patrón.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Nodo {
    Vacio,
    Caracter(char),
    Punto,
    Clase(Clase),
    Inicio,
    Fin,
    /// `\b` (`true`) o `\B` (`false`).
    Limite(bool),
    /// Un grupo; con número si captura.
    Grupo(Box<Nodo>, Option<u32>),
    /// `(?=...)` (positiva) o `(?!...)`.
    Mirar(Box<Nodo>, bool),
    Referencia(u32),
    Secuencia(Vec<Nodo>),
    Alternativa(Vec<Nodo>),
    Repetir {
        nodo: Box<Nodo>,
        min: u32,
        max: Option<u32>,
        codicioso: bool,
    },
}

pub(crate) struct Lector<'a> {
    c: &'a [char],
    i: usize,
    /// Grupos que captura el patrón entero (para saber si `\2` es una referencia).
    pub(crate) total_grupos: u32,
    grupos: u32,
    sin_caso: bool,
    unicode: bool,
    anidado: usize,
}

fn error<T>(m: &str) -> Result<T, Error> {
    Err(Error { mensaje: alloc::string::String::from(m) })
}

impl<'a> Lector<'a> {
    pub(crate) fn nuevo(c: &'a [char], sin_caso: bool, unicode: bool) -> Lector<'a> {
        let mut total = 0;
        let (mut i, mut en_clase) = (0, false);
        while i < c.len() {
            match c[i] {
                '\\' => i += 1,
                '[' => en_clase = true,
                ']' => en_clase = false,
                '(' if !en_clase && c.get(i + 1) != Some(&'?') => total += 1,
                '(' if !en_clase && c.get(i + 2) == Some(&'<') && !matches!(c.get(i + 3), Some('=' | '!')) => {
                    total += 1
                }
                _ => {}
            }
            i += 1;
        }
        Lector { c, i: 0, total_grupos: total, grupos: 0, sin_caso, unicode, anidado: 0 }
    }

    fn ver(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn comer(&mut self, x: char) -> bool {
        let si = self.ver() == Some(x);
        if si {
            self.i += 1;
        }
        si
    }

    /// El patrón entero.
    pub(crate) fn todo(&mut self) -> Result<Nodo, Error> {
        let n = self.disyuncion()?;
        if self.i < self.c.len() {
            return error("paréntesis ')' sin abrir");
        }
        Ok(n)
    }

    fn disyuncion(&mut self) -> Result<Nodo, Error> {
        self.anidado += 1;
        if self.anidado > MAX_ANIDADO {
            return error("patrón demasiado anidado");
        }
        let mut ramas = alloc::vec![self.alternativa()?];
        while self.comer('|') {
            ramas.push(self.alternativa()?);
        }
        self.anidado -= 1;
        Ok(if ramas.len() == 1 { ramas.pop().unwrap_or(Nodo::Vacio) } else { Nodo::Alternativa(ramas) })
    }

    fn alternativa(&mut self) -> Result<Nodo, Error> {
        let mut v = Vec::new();
        while let Some(c) = self.ver() {
            if c == '|' || c == ')' {
                break;
            }
            let atomo = self.termino()?;
            v.push(self.cuantificador(atomo)?);
        }
        Ok(match v.len() {
            0 => Nodo::Vacio,
            1 => v.pop().unwrap_or(Nodo::Vacio),
            _ => Nodo::Secuencia(v),
        })
    }

    fn termino(&mut self) -> Result<Nodo, Error> {
        let Some(c) = self.ver() else { return Ok(Nodo::Vacio) };
        self.i += 1;
        Ok(match c {
            '^' => Nodo::Inicio,
            '$' => Nodo::Fin,
            '.' => Nodo::Punto,
            '(' => return self.grupo(),
            '[' => Nodo::Clase(self.clase()?),
            '\\' => return self.escape(),
            '*' | '+' | '?' => return error("nada que repetir"),
            '{' if self.unicode => return error("'{' suelto con la bandera u"),
            c => Nodo::Caracter(c),
        })
    }

    fn grupo(&mut self) -> Result<Nodo, Error> {
        let (captura, mirar) = if self.comer('?') {
            let r = match self.ver() {
                Some(':') => (false, None),
                Some('=') => (false, Some(true)),
                Some('!') => (false, Some(false)),
                Some('<') if !matches!(self.c.get(self.i + 1), Some('=' | '!')) => {
                    // Grupo con nombre: captura como uno normal (el nombre aún no se usa).
                    while self.ver().is_some_and(|c| c != '>') {
                        self.i += 1;
                    }
                    (true, None)
                }
                _ => return error("grupo '(?' no soportado"),
            };
            self.i += 1; // el `:`, `=`, `!` o el `>` del nombre
            r
        } else {
            (true, None)
        };
        let numero = captura.then(|| {
            self.grupos += 1;
            self.grupos
        });
        let dentro = self.disyuncion()?;
        if !self.comer(')') {
            return error("falta ')'");
        }
        Ok(match mirar {
            Some(positiva) => Nodo::Mirar(Box::new(dentro), positiva),
            None => Nodo::Grupo(Box::new(dentro), numero),
        })
    }

    fn cuantificador(&mut self, atomo: Nodo) -> Result<Nodo, Error> {
        let (min, max) = match self.ver() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            Some('{') => match self.llaves() {
                Some(r) => r,
                None => return Ok(atomo), // anexo B: `{` literal; se lee en el siguiente término
            },
            _ => return Ok(atomo),
        };
        self.i += 1; // el `*`, `+`, `?` o la `}` final
        if matches!(atomo, Nodo::Inicio | Nodo::Fin | Nodo::Limite(_)) {
            return error("nada que repetir");
        }
        if max.is_some_and(|m| m < min) {
            return error("cuantificador con el máximo menor que el mínimo");
        }
        let codicioso = !self.comer('?');
        Ok(Nodo::Repetir { nodo: Box::new(atomo), min, max, codicioso })
    }

    /// `{n}`, `{n,}`, `{n,m}`: deja el cursor sobre la `}`. `None` (sin moverlo) si no es un cuantificador.
    fn llaves(&mut self) -> Option<(u32, Option<u32>)> {
        let inicio = self.i;
        self.i += 1;
        let r = match (self.numero(), self.comer(',')) {
            (Some(n), false) => Some((n, Some(n))),
            (Some(n), true) => Some((n, self.numero())),
            _ => None,
        };
        if r.is_some() && self.ver() == Some('}') {
            return r;
        }
        self.i = inicio;
        None
    }

    fn numero(&mut self) -> Option<u32> {
        let inicio = self.i;
        let mut n: u32 = 0;
        while let Some(d) = self.ver().and_then(|c| c.to_digit(10)) {
            n = n.saturating_mul(10).saturating_add(d);
            self.i += 1;
        }
        (self.i > inicio).then_some(n)
    }

    fn escape(&mut self) -> Result<Nodo, Error> {
        let Some(c) = self.ver() else { return error("'\\' al final del patrón") };
        self.i += 1;
        Ok(match c {
            'b' => Nodo::Limite(true),
            'B' => Nodo::Limite(false),
            'd' | 'D' | 'w' | 'W' | 's' | 'S' => {
                let (a, negada) = abreviada(c);
                let mut k = Constructor::default();
                k.abreviada(a, negada);
                if self.sin_caso {
                    k.sin_caso();
                }
                Nodo::Clase(k.terminar(false))
            }
            'p' | 'P' if self.unicode => {
                let mut k = Constructor::default();
                k.propiedad(self.propiedad()?, c == 'P');
                Nodo::Clase(k.terminar(false))
            }
            '1'..='9' => {
                self.i -= 1;
                let inicio = self.i;
                let n = self.numero().unwrap_or(0);
                if n <= self.total_grupos {
                    return Ok(Nodo::Referencia(n));
                }
                // Anexo B: no hay tantos grupos; es un escape octal o el propio dígito.
                self.i = inicio + 1;
                Nodo::Caracter(c)
            }
            _ => Nodo::Caracter(self.caracter_escapado(c)?),
        })
    }

    /// El carácter de un escape que no es una clase ni una aserción (`\n`, `\x41`, `é`, `\.`).
    fn caracter_escapado(&mut self, c: char) -> Result<char, Error> {
        Ok(match c {
            't' => '\t',
            'n' => '\n',
            'r' => '\r',
            'v' => '\u{b}',
            'f' => '\u{c}',
            '0' if !self.ver().is_some_and(|d| d.is_ascii_digit()) => '\0',
            'c' => match self.ver() {
                Some(l) if l.is_ascii_alphabetic() => {
                    self.i += 1;
                    char::from_u32(l as u32 % 32).unwrap_or(l)
                }
                _ => '\\',
            },
            'x' => self.hexadecimal(2).unwrap_or('x'),
            'u' => self.unicode_escape().unwrap_or('u'),
            c => c,
        })
    }

    /// El `{Nombre}` de `\p{Nombre}` (con el cursor tras la `p`): sus rangos.
    fn propiedad(&mut self) -> Result<Vec<(u32, u32)>, Error> {
        if !self.comer('{') {
            return error(r"falta '{' tras \p");
        }
        let inicio = self.i;
        while self.ver().is_some_and(|c| c != '}') {
            self.i += 1;
        }
        let nombre: alloc::string::String = self.c[inicio..self.i].iter().collect();
        if !self.comer('}') {
            return error(r"falta '}' en \p{...}");
        }
        match crate::propiedades::rangos(&nombre) {
            Some(r) => Ok(r),
            None => error(&alloc::format!("propiedad de Unicode desconocida: {nombre}")),
        }
    }

    fn hexadecimal(&mut self, n: usize) -> Option<char> {
        let digitos = self.c.get(self.i..self.i + n)?;
        let mut v = 0u32;
        for d in digitos {
            v = v * 16 + d.to_digit(16)?;
        }
        self.i += n;
        char::from_u32(v)
    }

    fn unicode_escape(&mut self) -> Option<char> {
        if self.unicode && self.comer('{') {
            let inicio = self.i;
            while self.ver().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.i += 1;
            }
            let n = u32::from_str_radix(&self.c[inicio..self.i].iter().collect::<alloc::string::String>(), 16).ok();
            return n.and_then(char::from_u32).filter(|_| self.comer('}'));
        }
        self.hexadecimal(4)
    }

    /// `[...]` después del `[`.
    fn clase(&mut self) -> Result<Clase, Error> {
        let negada = self.comer('^');
        let mut k = Constructor::default();
        loop {
            let Some(c) = self.ver() else { return error("falta ']'") };
            self.i += 1;
            if c == ']' {
                break;
            }
            let desde = match self.atomo_de_clase(c, &mut k)? {
                Some(d) => d,
                None => continue,
            };
            // Un rango `a-z` (si lo que sigue al `-` no es `]` ni una clase abreviada).
            if self.ver() == Some('-') && !matches!(self.c.get(self.i + 1), Some(']') | None) {
                self.i += 1;
                let h = self.ver().unwrap_or(']');
                self.i += 1;
                match self.atomo_de_clase(h, &mut k)? {
                    Some(hasta) if hasta >= desde => k.rango(desde, hasta),
                    Some(_) => return error("rango de clase al revés"),
                    None => k.rango('-' as u32, '-' as u32),
                }
            }
            k.rango(desde, desde);
        }
        if self.sin_caso {
            k.sin_caso();
        }
        Ok(k.terminar(negada))
    }

    /// Un elemento de `[...]`: su carácter, o `None` si era una clase abreviada (ya añadida).
    fn atomo_de_clase(&mut self, c: char, k: &mut Constructor) -> Result<Option<u32>, Error> {
        if c != '\\' {
            return Ok(Some(c as u32));
        }
        let Some(e) = self.ver() else { return error("'\\' al final del patrón") };
        self.i += 1;
        Ok(match e {
            'd' | 'D' | 'w' | 'W' | 's' | 'S' => {
                let (a, negada) = abreviada(e);
                k.abreviada(a, negada);
                None
            }
            'p' | 'P' if self.unicode => {
                k.propiedad(self.propiedad()?, e == 'P');
                None
            }
            'b' => Some(8),
            '-' => Some('-' as u32),
            e => Some(self.caracter_escapado(e)? as u32),
        })
    }
}

fn abreviada(c: char) -> (Abreviada, bool) {
    let a = match c.to_ascii_lowercase() {
        'd' => Abreviada::Digito,
        'w' => Abreviada::Palabra,
        _ => Abreviada::Espacio,
    };
    (a, c.is_ascii_uppercase())
}
