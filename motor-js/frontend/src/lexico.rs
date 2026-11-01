//! Léxico de JavaScript: convierte el texto en tokens.
//!
//! Sigue ECMAScript 2024 para identificadores, números (con separadores `_`), cadenas con
//! escapes, signos y comentarios. Anota si hubo un salto de línea antes de cada token,
//! que es lo que necesita la inserción automática de `;`. Un `/` es una expresión regular si
//! el token anterior no puede terminar una expresión (como hacen los navegadores con su
//! regla del token previo). Las plantillas (`` ` ``) todavía no están: dan un error claro.

use crate::fuente::{Pos, Resultado, error};
use alloc::string::String;
use alloc::vec::Vec;

/// Qué es un token.
#[derive(Debug, Clone, PartialEq)]
pub enum Tipo {
    /// Un identificador (también las palabras contextuales: `of`, `async`, `get`...).
    Ident(String),
    /// Una palabra reservada.
    Clave(&'static str),
    /// Un número.
    Numero(f64),
    /// Una cadena, ya sin comillas ni escapes.
    Cadena(String),
    /// Un signo de puntuación.
    Signo(&'static str),
    /// Una expresión regular `/patrón/banderas`, sin validar todavía.
    Regex(String, String),
    /// El final del texto.
    Fin,
}

/// Un token con su posición.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Qué es.
    pub tipo: Tipo,
    /// Dónde empieza.
    pub pos: Pos,
    /// Hubo un salto de línea entre el token anterior y este.
    pub salto_antes: bool,
}

/// Palabras reservadas.
pub const CLAVES: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Signos, del más largo al más corto (se toma siempre el más largo posible).
const SIGNOS: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=", "<=", ">=", "&&",
    "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "**", "{", "}", "(", ")",
    "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "?", ":", "=", ".", "@", "#",
];

struct Lexico<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
    linea: u32,
    inicio_linea: usize,
}

/// Convierte todo el texto en tokens (el último es [`Tipo::Fin`]).
pub fn tokens(fuente: &str) -> Resultado<Vec<Token>> {
    let mut l = Lexico { s: fuente, b: fuente.as_bytes(), i: 0, linea: 1, inicio_linea: 0 };
    let mut v = Vec::with_capacity(fuente.len() / 4);
    loop {
        let salto_antes = l.saltar_blancos()?;
        let pos = l.pos();
        let regex_posible = v.last().is_none_or(|t: &Token| puede_preceder_regex(&t.tipo));
        let tipo = if l.i >= l.b.len() {
            Tipo::Fin
        } else if regex_posible && l.byte(0) == b'/' {
            l.regex()?
        } else {
            l.token()?
        };
        let fin = tipo == Tipo::Fin;
        v.push(Token { tipo, pos, salto_antes });
        if fin {
            return Ok(v);
        }
    }
}

/// Si tras este token un `/` empieza una expresión regular (y no es una división): no lo es
/// tras algo que termina una expresión (`x /`, `) /`, `] /`, `this /`...).
fn puede_preceder_regex(t: &Tipo) -> bool {
    match t {
        Tipo::Signo(s) => !matches!(*s, ")" | "]" | "}" | "++" | "--"),
        Tipo::Clave(k) => !matches!(*k, "this" | "super" | "null" | "true" | "false"),
        _ => false,
    }
}

fn es_inicio_ident(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || (!c.is_ascii() && c.is_alphabetic())
}

fn es_parte_ident(c: char) -> bool {
    es_inicio_ident(c) || c.is_ascii_digit() || c == '\u{200C}' || c == '\u{200D}' || (!c.is_ascii() && c.is_numeric())
}

fn es_salto(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

impl Lexico<'_> {
    fn pos(&self) -> Pos {
        Pos { linea: self.linea, columna: (self.i - self.inicio_linea) as u32 + 1 }
    }

    fn actual(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn byte(&self, desplazamiento: usize) -> u8 {
        self.b.get(self.i + desplazamiento).copied().unwrap_or(0)
    }

    fn nueva_linea(&mut self) {
        self.linea += 1;
        self.inicio_linea = self.i;
    }

    /// Salta espacios y comentarios; devuelve si cruzó un salto de línea.
    fn saltar_blancos(&mut self) -> Resultado<bool> {
        let mut salto = false;
        while let Some(c) = self.actual() {
            if es_salto(c) {
                self.i += c.len_utf8();
                if !(c == '\r' && self.byte(0) == b'\n') {
                    self.nueva_linea();
                }
                salto = true;
            } else if c.is_whitespace() || c == '\u{FEFF}' {
                self.i += c.len_utf8();
            } else if c == '/' && self.byte(1) == b'/' {
                while self.actual().is_some_and(|c| !es_salto(c)) {
                    self.i += self.actual().map_or(1, char::len_utf8);
                }
            } else if c == '/' && self.byte(1) == b'*' {
                salto |= self.comentario_bloque()?;
            } else {
                break;
            }
        }
        Ok(salto)
    }

    fn comentario_bloque(&mut self) -> Resultado<bool> {
        let pos = self.pos();
        self.i += 2;
        let mut salto = false;
        loop {
            match self.actual() {
                None => return error(pos, "comentario /* sin cerrar"),
                Some('*') if self.byte(1) == b'/' => {
                    self.i += 2;
                    return Ok(salto);
                }
                Some(c) => {
                    self.i += c.len_utf8();
                    if es_salto(c) && !(c == '\r' && self.byte(0) == b'\n') {
                        self.nueva_linea();
                        salto = true;
                    }
                }
            }
        }
    }

    fn token(&mut self) -> Resultado<Tipo> {
        let pos = self.pos();
        let c = self.actual().unwrap_or('\0');
        if es_inicio_ident(c) {
            return Ok(self.identificador());
        }
        if c.is_ascii_digit() || (c == '.' && self.byte(1).is_ascii_digit()) {
            return self.numero();
        }
        match c {
            '"' | '\'' => self.cadena(),
            '`' => error(pos, "las plantillas (`...`) todavía no están en QUIMERA"),
            '\\' => error(pos, "los escapes \\u en identificadores todavía no están en QUIMERA"),
            _ => self.signo(),
        }
    }

    /// `/patrón/banderas`, con el cursor en el primer `/`.
    fn regex(&mut self) -> Resultado<Tipo> {
        let pos = self.pos();
        self.i += 1;
        let inicio = self.i;
        let mut en_clase = false;
        loop {
            let Some(c) = self.actual().filter(|c| !es_salto(*c)) else {
                return error(pos, "expresión regular sin cerrar");
            };
            self.i += c.len_utf8();
            match c {
                '\\' => match self.actual().filter(|c| !es_salto(*c)) {
                    Some(e) => self.i += e.len_utf8(),
                    None => return error(pos, "expresión regular sin cerrar"),
                },
                '[' => en_clase = true,
                ']' => en_clase = false,
                '/' if !en_clase => break,
                _ => {}
            }
        }
        let patron = String::from(&self.s[inicio..self.i - 1]);
        let desde = self.i;
        while let Some(c) = self.actual().filter(|c| es_parte_ident(*c)) {
            self.i += c.len_utf8();
        }
        Ok(Tipo::Regex(patron, String::from(&self.s[desde..self.i])))
    }

    fn identificador(&mut self) -> Tipo {
        let inicio = self.i;
        while let Some(c) = self.actual().filter(|c| es_parte_ident(*c)) {
            self.i += c.len_utf8();
        }
        let palabra = &self.s[inicio..self.i];
        match CLAVES.iter().find(|k| **k == palabra) {
            Some(k) => Tipo::Clave(k),
            None => Tipo::Ident(String::from(palabra)),
        }
    }

    fn signo(&mut self) -> Resultado<Tipo> {
        let resto = &self.s[self.i..];
        for s in SIGNOS {
            // `?.` seguido de un dígito es `?` y un número (a?.5:b).
            if resto.starts_with(s) && !(*s == "?." && self.byte(2).is_ascii_digit()) {
                self.i += s.len();
                return Ok(Tipo::Signo(s));
            }
        }
        let c = self.actual().unwrap_or('\0');
        error(self.pos(), alloc::format!("carácter inesperado {c:?}"))
    }

    fn digitos(&mut self, base: u32, texto: &mut String) -> Resultado<()> {
        while let Some(c) = self.actual() {
            if c == '_' {
                if !self.s[self.i + 1..].chars().next().is_some_and(|d| d.is_digit(base)) {
                    return error(self.pos(), "un separador _ debe ir entre dos dígitos");
                }
                self.i += 1;
            } else if c.is_digit(base) {
                texto.push(c);
                self.i += 1;
            } else {
                return Ok(());
            }
        }
        Ok(())
    }

    fn numero(&mut self) -> Resultado<Tipo> {
        let pos = self.pos();
        let mut texto = String::new();
        let base = match (self.byte(0), self.byte(1) | 0x20) {
            (b'0', b'x') => 16,
            (b'0', b'o') => 8,
            (b'0', b'b') => 2,
            _ => 10,
        };
        let valor = if base != 10 {
            self.i += 2;
            self.digitos(base, &mut texto)?;
            if texto.is_empty() {
                return error(pos, "faltan los dígitos del número");
            }
            texto.chars().fold(0f64, |n, d| n * base as f64 + d.to_digit(base).unwrap_or(0) as f64)
        } else {
            self.decimal(&mut texto)?;
            texto.parse::<f64>().or_else(|_| error(pos, "número mal escrito"))?
        };
        if self.byte(0) == b'n' {
            return error(pos, "los BigInt (123n) todavía no están en QUIMERA");
        }
        if self.actual().is_some_and(es_inicio_ident) {
            return error(self.pos(), "un número no puede ir pegado a un identificador");
        }
        Ok(Tipo::Numero(valor))
    }

    fn decimal(&mut self, texto: &mut String) -> Resultado<()> {
        self.digitos(10, texto)?;
        if self.byte(0) == b'.' {
            texto.push('.');
            self.i += 1;
            self.digitos(10, texto)?;
        }
        if self.byte(0) | 0x20 == b'e' {
            texto.push('e');
            self.i += 1;
            if matches!(self.byte(0), b'+' | b'-') {
                texto.push(self.byte(0) as char);
                self.i += 1;
            }
            let antes = texto.len();
            self.digitos(10, texto)?;
            if texto.len() == antes {
                return error(self.pos(), "falta el exponente");
            }
        }
        Ok(())
    }

    fn cadena(&mut self) -> Resultado<Tipo> {
        let pos = self.pos();
        let comilla = self.actual().unwrap_or('"');
        self.i += 1;
        let mut texto = String::new();
        loop {
            match self.actual() {
                None => return error(pos, "cadena sin cerrar"),
                Some(c) if c == comilla => {
                    self.i += 1;
                    return Ok(Tipo::Cadena(texto));
                }
                Some('\n' | '\r') => return error(pos, "cadena sin cerrar antes del salto de línea"),
                Some('\\') => {
                    self.i += 1;
                    self.escape(&mut texto)?;
                }
                Some(c) => {
                    texto.push(c);
                    self.i += c.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self, texto: &mut String) -> Resultado<()> {
        let pos = self.pos();
        let Some(c) = self.actual() else { return error(pos, "cadena sin cerrar") };
        self.i += c.len_utf8();
        let simple = match c {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{C}'),
            'v' => Some('\u{B}'),
            '0' if !self.byte(0).is_ascii_digit() => Some('\0'),
            _ => None,
        };
        if let Some(s) = simple {
            texto.push(s);
            return Ok(());
        }
        match c {
            'x' => {
                let n = self.hexadecimal(2)?;
                texto.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
            }
            'u' => self.escape_unicode(texto)?,
            '\r' => {
                if self.byte(0) == b'\n' {
                    self.i += 1;
                }
                self.nueva_linea();
            }
            '\n' | '\u{2028}' | '\u{2029}' => self.nueva_linea(),
            '1'..='9' => return error(pos, "los escapes octales no están permitidos"),
            otro => texto.push(otro),
        }
        Ok(())
    }

    fn hexadecimal(&mut self, n: usize) -> Resultado<u32> {
        let pos = self.pos();
        let trozo = self.s.get(self.i..self.i + n).filter(|t| t.bytes().all(|b| b.is_ascii_hexdigit()));
        let Some(trozo) = trozo else { return error(pos, "escape hexadecimal mal escrito") };
        self.i += n;
        u32::from_str_radix(trozo, 16).or_else(|_| error(pos, "escape hexadecimal mal escrito"))
    }

    /// `\uXXXX`, `\u{X...}` y pares sustitutos (`😀`). Uno suelto se cambia por U+FFFD:
    /// las cadenas de Rust son UTF-8; las de JavaScript (UTF-16) llegarán con `quimera-cadenas`.
    fn escape_unicode(&mut self, texto: &mut String) -> Resultado<()> {
        let pos = self.pos();
        let n = if self.byte(0) == b'{' {
            self.i += 1;
            let fin =
                self.s[self.i..].find('}').ok_or_else(|| crate::fuente::Error { pos, mensaje: "falta }".into() })?;
            let n = u32::from_str_radix(&self.s[self.i..self.i + fin], 16)
                .or_else(|_| error(pos, "escape \\u{...} mal escrito"))?;
            self.i += fin + 1;
            n
        } else {
            self.hexadecimal(4)?
        };
        let n = if (0xD800..0xDC00).contains(&n) && self.s[self.i..].starts_with("\\u") {
            let guardado = self.i;
            self.i += 2;
            let bajo = self.hexadecimal(4)?;
            if (0xDC00..0xE000).contains(&bajo) {
                0x10000 + ((n - 0xD800) << 10) + (bajo - 0xDC00)
            } else {
                self.i = guardado;
                n
            }
        } else {
            n
        };
        texto.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
        Ok(())
    }
}
