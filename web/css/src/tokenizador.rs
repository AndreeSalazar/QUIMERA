//! Tokenizador de CSS (CSS Syntax Level 3): texto → identificadores, números, dimensiones,
//! cadenas, `#hash`, funciones, signos... Los comentarios desaparecen.

use alloc::string::String;
use alloc::vec::Vec;

/// Un token de CSS.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// `color`, `-webkit-box`.
    Ident(String),
    /// `rgb(` (el nombre, sin el paréntesis).
    Funcion(String),
    /// `@media` (sin la arroba).
    Arroba(String),
    /// `#fff`, `#principal`.
    Almohadilla(String),
    /// `"texto"` ya sin comillas ni escapes.
    Cadena(String),
    /// `url(imagen.png)` sin comillas.
    Url(String),
    /// `12`, `-1.5`.
    Numero(f64),
    /// `50%`.
    Porcentaje(f64),
    /// `12px`, `1.5em` (la unidad en minúsculas).
    Dimension(f64, String),
    /// Uno o más blancos.
    Blanco,
    /// Cualquier otro carácter suelto (`.`, `>`, `+`, `*`, `!`...).
    Delim(char),
    /// `:`
    DosPuntos,
    /// `;`
    PuntoYComa,
    /// `,`
    Coma,
    /// `[`, `]`, `(`, `)`, `{`, `}`.
    Abre(char),
    /// El que cierra.
    Cierra(char),
}

struct Lector {
    c: Vec<char>,
    i: usize,
}

/// Todos los tokens de un texto.
pub fn tokens(texto: &str) -> Vec<Token> {
    let mut l = Lector { c: texto.chars().collect(), i: 0 };
    let mut v = Vec::new();
    while let Some(t) = l.token() {
        v.push(t);
    }
    v
}

fn empieza_ident(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn parte_ident(c: char) -> bool {
    empieza_ident(c) || c.is_ascii_digit() || c == '-'
}

impl Lector {
    fn ver(&self, k: usize) -> Option<char> {
        self.c.get(self.i + k).copied()
    }

    /// Si aquí empieza un identificador (`a`, `-a`, `--a`, `\31 a`).
    fn hay_ident(&self, k: usize) -> bool {
        match self.ver(k) {
            Some('-') => self.ver(k + 1).is_some_and(|c| empieza_ident(c) || c == '-') || self.hay_escape(k + 1),
            Some('\\') => self.hay_escape(k),
            Some(c) => empieza_ident(c),
            None => false,
        }
    }

    fn hay_escape(&self, k: usize) -> bool {
        self.ver(k) == Some('\\') && self.ver(k + 1).is_some_and(|c| c != '\n')
    }

    /// Si aquí empieza un número (`1`, `.5`, `-2`, `+.3`).
    fn hay_numero(&self) -> bool {
        let digito = |k: usize| self.ver(k).is_some_and(|c| c.is_ascii_digit());
        match self.ver(0) {
            Some('+' | '-') => digito(1) || (self.ver(1) == Some('.') && digito(2)),
            Some('.') => digito(1),
            _ => digito(0),
        }
    }

    fn token(&mut self) -> Option<Token> {
        let c = self.ver(0)?;
        if c == '/' && self.ver(1) == Some('*') {
            self.i += 2;
            while self.i < self.c.len() && !(self.ver(0) == Some('*') && self.ver(1) == Some('/')) {
                self.i += 1;
            }
            self.i = (self.i + 2).min(self.c.len());
            return self.token();
        }
        if c.is_whitespace() {
            while self.ver(0).is_some_and(char::is_whitespace) {
                self.i += 1;
            }
            return Some(Token::Blanco);
        }
        if self.hay_numero() {
            return Some(self.numerico());
        }
        if self.hay_ident(0) {
            return Some(self.ident_o_funcion());
        }
        self.i += 1;
        Some(match c {
            '"' | '\'' => Token::Cadena(self.cadena(c)),
            '#' if self.ver(0).is_some_and(parte_ident) || self.hay_escape(0) => Token::Almohadilla(self.nombre()),
            '@' if self.hay_ident(0) => Token::Arroba(self.nombre()),
            ':' => Token::DosPuntos,
            ';' => Token::PuntoYComa,
            ',' => Token::Coma,
            '[' | '(' | '{' => Token::Abre(c),
            ']' | ')' | '}' => Token::Cierra(c),
            c => Token::Delim(c),
        })
    }

    /// Un nombre (identificador sin interpretar), con escapes resueltos.
    fn nombre(&mut self) -> String {
        let mut s = String::new();
        loop {
            match self.ver(0) {
                Some(c) if parte_ident(c) => {
                    s.push(c);
                    self.i += 1;
                }
                Some('\\') if self.hay_escape(0) => {
                    self.i += 1;
                    s.push(self.escape());
                }
                _ => return s,
            }
        }
    }

    /// Tras la `\`: hasta 6 cifras hexadecimales (y un blanco) o el carácter tal cual.
    fn escape(&mut self) -> char {
        let inicio = self.i;
        while self.i - inicio < 6 && self.ver(0).is_some_and(|c| c.is_ascii_hexdigit()) {
            self.i += 1;
        }
        if self.i == inicio {
            let c = self.ver(0).unwrap_or('\u{FFFD}');
            self.i += 1;
            return c;
        }
        let hex: String = self.c[inicio..self.i].iter().collect();
        if self.ver(0).is_some_and(char::is_whitespace) {
            self.i += 1;
        }
        let n = u32::from_str_radix(&hex, 16).unwrap_or(0);
        match char::from_u32(n) {
            Some(c) if n != 0 => c,
            _ => '\u{FFFD}',
        }
    }

    fn ident_o_funcion(&mut self) -> Token {
        let nombre = self.nombre();
        if self.ver(0) != Some('(') {
            return Token::Ident(nombre);
        }
        self.i += 1;
        if !nombre.eq_ignore_ascii_case("url") {
            return Token::Funcion(nombre.to_ascii_lowercase());
        }
        while self.ver(0).is_some_and(char::is_whitespace) {
            self.i += 1;
        }
        if matches!(self.ver(0), Some('"' | '\'')) {
            return Token::Funcion(String::from("url")); // url("...") es una función normal
        }
        let mut s = String::new();
        while let Some(c) = self.ver(0) {
            self.i += 1;
            match c {
                ')' => break,
                '\\' => s.push(self.escape()),
                c if c.is_whitespace() => {}
                c => s.push(c),
            }
        }
        Token::Url(s)
    }

    fn cadena(&mut self, comilla: char) -> String {
        let mut s = String::new();
        while let Some(c) = self.ver(0) {
            self.i += 1;
            match c {
                c if c == comilla => break,
                '\n' => break, // cadena mal cerrada: termina en el salto
                '\\' => match self.ver(0) {
                    Some('\n') => self.i += 1,
                    Some(_) => s.push(self.escape()),
                    None => {}
                },
                c => s.push(c),
            }
        }
        s
    }

    fn numerico(&mut self) -> Token {
        let inicio = self.i;
        if matches!(self.ver(0), Some('+' | '-')) {
            self.i += 1;
        }
        let cifras = |l: &mut Self| {
            while l.ver(0).is_some_and(|c| c.is_ascii_digit()) {
                l.i += 1;
            }
        };
        cifras(self);
        if self.ver(0) == Some('.') && self.ver(1).is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
            cifras(self);
        }
        let con_signo =
            |k: usize| matches!(self.ver(k), Some('+' | '-')) && self.ver(k + 1).is_some_and(|c| c.is_ascii_digit());
        if matches!(self.ver(0), Some('e' | 'E')) && (self.ver(1).is_some_and(|c| c.is_ascii_digit()) || con_signo(1)) {
            self.i += 2;
            cifras(self);
        }
        let texto: String = self.c[inicio..self.i].iter().collect();
        let n: f64 = texto.parse().unwrap_or(0.0);
        if self.ver(0) == Some('%') {
            self.i += 1;
            return Token::Porcentaje(n);
        }
        if self.hay_ident(0) {
            return Token::Dimension(n, self.nombre().to_ascii_lowercase());
        }
        Token::Numero(n)
    }
}

#[cfg(test)]
mod pruebas {
    use super::Token::*;
    use super::*;

    #[test]
    fn tokens_basicos() {
        let t = tokens("a.b>#c{margin:-1.5em 50% .5px!important}/* nada */@media url( x.png ) rgb(");
        assert_eq!(
            t,
            [
                Ident("a".into()),
                Delim('.'),
                Ident("b".into()),
                Delim('>'),
                Almohadilla("c".into()),
                Abre('{'),
                Ident("margin".into()),
                DosPuntos,
                Dimension(-1.5, "em".into()),
                Blanco,
                Porcentaje(50.0),
                Blanco,
                Dimension(0.5, "px".into()),
                Delim('!'),
                Ident("important".into()),
                Cierra('}'),
                Arroba("media".into()),
                Blanco,
                Url("x.png".into()),
                Blanco,
                Funcion("rgb".into()),
            ]
        );
    }

    #[test]
    fn cadenas_escapes_y_numeros() {
        assert_eq!(tokens("'a\\'b' \"c"), [Cadena("a'b".into()), Blanco, Cadena("c".into())]);
        assert_eq!(tokens(".\\31 a"), [Delim('.'), Ident("1a".into())]);
        assert_eq!(
            tokens("1e3 2E-1 -.5 +3"),
            [Numero(1000.0), Blanco, Numero(0.2), Blanco, Numero(-0.5), Blanco, Numero(3.0)]
        );
        assert_eq!(tokens("--var -x"), [Ident("--var".into()), Blanco, Ident("-x".into())]);
    }
}
