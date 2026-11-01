//! Tokenizador de HTML: texto → etiquetas de apertura y cierre, texto, comentarios y doctype.
//!
//! Sigue las reglas del estándar donde importan en la práctica: nombres en minúsculas,
//! atributos con o sin comillas, `/>`, comentarios mal cerrados, `<` sueltos que son texto, y
//! el contenido crudo de `<script>`/`<style>` (y `<title>`/`<textarea>`, con referencias).

use crate::entidades;
use alloc::string::String;
use alloc::vec::Vec;

/// Un token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Doctype(String),
    Inicio { nombre: String, atributos: Vec<(String, String)>, autocierre: bool },
    Fin(String),
    Texto(String),
    Comentario(String),
}

/// Elementos cuyo contenido no es HTML sino texto hasta su etiqueta de cierre. `noscript` lo
/// es porque QUIMERA ejecuta JavaScript (con JavaScript activo, su contenido no se analiza).
const CRUDOS: &[&str] = &["script", "style", "xmp", "iframe", "noembed", "noframes", "noscript"];
/// Como los crudos, pero con referencias de caracteres (`&amp;`).
const RCDATA: &[&str] = &["title", "textarea"];

pub(crate) struct Tokenizador<'a> {
    s: &'a str,
    i: usize,
    /// Dentro de un elemento crudo: su nombre y si decodifica referencias.
    crudo: Option<(String, bool)>,
}

fn es_blanco(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r')
}

impl<'a> Tokenizador<'a> {
    pub(crate) fn nuevo(s: &'a str) -> Tokenizador<'a> {
        Tokenizador { s, i: 0, crudo: None }
    }

    fn resto(&self) -> &'a str {
        &self.s[self.i..]
    }

    fn byte(&self, k: usize) -> Option<u8> {
        self.s.as_bytes().get(self.i + k).copied()
    }

    /// El siguiente token, o `None` al final.
    pub(crate) fn siguiente(&mut self) -> Option<Token> {
        if let Some((nombre, rcdata)) = self.crudo.take() {
            let fin = self.cierre_crudo(&nombre);
            let texto = &self.s[self.i..fin];
            self.i = fin;
            if !texto.is_empty() {
                return Some(Token::Texto(if rcdata {
                    entidades::decodificar(texto, false)
                } else {
                    String::from(texto)
                }));
            }
        }
        if self.i >= self.s.len() {
            return None;
        }
        if self.byte(0) == Some(b'<')
            && let Some(t) = self.etiqueta()
        {
            return Some(t);
        }
        // Texto hasta el siguiente `<` que pueda abrir algo (un `<` suelto se queda en el texto).
        let desde = self.i;
        self.i = self.fin_de_texto();
        Some(Token::Texto(entidades::decodificar(&self.s[desde..self.i], false)))
    }

    /// Dónde termina el texto que empieza aquí: en el siguiente `<` seguido de letra, `/`, `!` o `?`.
    fn fin_de_texto(&self) -> usize {
        let b = self.s.as_bytes();
        let mut p = self.i + 1;
        while let Some(k) = b.get(p..).and_then(|r| r.iter().position(|&c| c == b'<')) {
            let pos = p + k;
            if b.get(pos + 1).is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, b'/' | b'!' | b'?')) {
                return pos;
            }
            p = pos + 1;
        }
        self.s.len()
    }

    /// Dónde empieza `</nombre` (sin distinguir mayúsculas) seguido de blanco, `/` o `>`.
    fn cierre_crudo(&self, nombre: &str) -> usize {
        let b = self.s.as_bytes();
        let mut p = self.i;
        while let Some(k) = self.s[p..].find("</") {
            let inicio = p + k;
            let fin_nombre = inicio + 2 + nombre.len();
            let coincide = b.get(inicio + 2..fin_nombre).is_some_and(|x| x.eq_ignore_ascii_case(nombre.as_bytes()));
            if coincide && b.get(fin_nombre).is_none_or(|&c| es_blanco(c) || c == b'/' || c == b'>') {
                return inicio;
            }
            p = inicio + 2;
        }
        self.s.len()
    }

    /// Lo que empieza por `<`; `None` si el `<` es texto.
    fn etiqueta(&mut self) -> Option<Token> {
        let r = self.resto();
        if r.starts_with("<!--") {
            return Some(self.comentario());
        }
        if r.starts_with("<!") || r.starts_with("<?") {
            return Some(self.declaracion());
        }
        match self.byte(1) {
            Some(b'/') => match self.byte(2) {
                Some(c) if c.is_ascii_alphabetic() => Some(self.cierre()),
                Some(b'>') => {
                    self.i += 3; // `</>` no es nada
                    self.siguiente()
                }
                None => None,
                Some(_) => Some(self.comentario_falso(2)),
            },
            Some(c) if c.is_ascii_alphabetic() => Some(self.apertura()),
            _ => None,
        }
    }

    fn comentario(&mut self) -> Token {
        let cuerpo = &self.s[self.i + 4..];
        // `<!-->` y `<!--->` son comentarios vacíos.
        if cuerpo.starts_with('>') || cuerpo.starts_with("->") {
            self.i += 4 + if cuerpo.starts_with('>') { 1 } else { 2 };
            return Token::Comentario(String::new());
        }
        let (texto, consumido) = match cuerpo.find("-->") {
            Some(p) => (&cuerpo[..p], p + 3),
            None => (cuerpo, cuerpo.len()),
        };
        self.i += 4 + consumido;
        Token::Comentario(String::from(texto))
    }

    /// `<!DOCTYPE html>`, o cualquier otro `<!...>` / `<?...>` (un comentario falso).
    fn declaracion(&mut self) -> Token {
        let r = self.resto();
        if r.len() >= 9 && r[2..9].eq_ignore_ascii_case("doctype") {
            let fin = r.find('>').unwrap_or(r.len());
            let nombre = r[9..fin].split_ascii_whitespace().next().unwrap_or("").to_ascii_lowercase();
            self.i += (fin + 1).min(r.len());
            return Token::Doctype(nombre);
        }
        self.comentario_falso(if r.starts_with("<?") { 1 } else { 2 })
    }

    fn comentario_falso(&mut self, salto: usize) -> Token {
        let r = &self.s[self.i + salto..];
        let fin = r.find('>').unwrap_or(r.len());
        self.i += salto + (fin + 1).min(r.len());
        Token::Comentario(String::from(&r[..fin]))
    }

    fn nombre(&mut self) -> String {
        let desde = self.i;
        while self.byte(0).is_some_and(|b| !es_blanco(b) && b != b'/' && b != b'>') {
            self.i += 1;
        }
        self.s[desde..self.i].to_ascii_lowercase()
    }

    fn cierre(&mut self) -> Token {
        self.i += 2;
        let nombre = self.nombre();
        let fin = self.resto().find('>').map_or(self.s.len(), |p| self.i + p + 1);
        self.i = fin;
        Token::Fin(nombre)
    }

    fn apertura(&mut self) -> Token {
        self.i += 1;
        let nombre = self.nombre();
        let mut atributos: Vec<(String, String)> = Vec::new();
        let mut autocierre = false;
        loop {
            while self.byte(0).is_some_and(es_blanco) {
                self.i += 1;
            }
            match self.byte(0) {
                None => break,
                Some(b'>') => {
                    self.i += 1;
                    break;
                }
                Some(b'/') => {
                    self.i += 1;
                    autocierre = self.byte(0) == Some(b'>');
                }
                Some(_) => {
                    let (n, v) = self.atributo();
                    if !n.is_empty() && !atributos.iter().any(|(x, _)| *x == n) {
                        atributos.push((n, v));
                    }
                }
            }
        }
        if let Some(c) = CRUDOS.iter().chain(RCDATA).find(|c| **c == nombre) {
            self.crudo = Some((String::from(*c), RCDATA.contains(c)));
        }
        Token::Inicio { nombre, atributos, autocierre }
    }

    /// `nombre`, `nombre=valor`, `nombre="valor"` o `nombre='valor'`.
    fn atributo(&mut self) -> (String, String) {
        let desde = self.i;
        self.i += 1; // el primer carácter puede ser `=` (anómalo pero válido)
        while self.byte(0).is_some_and(|b| !es_blanco(b) && b != b'/' && b != b'>' && b != b'=') {
            self.i += 1;
        }
        let nombre = self.s[desde..self.i].to_ascii_lowercase();
        let mut k = 0;
        while self.byte(k).is_some_and(es_blanco) {
            k += 1;
        }
        if self.byte(k) != Some(b'=') {
            return (nombre, String::new());
        }
        self.i += k + 1;
        while self.byte(0).is_some_and(es_blanco) {
            self.i += 1;
        }
        let valor = match self.byte(0) {
            Some(q @ (b'"' | b'\'')) => {
                let r = &self.s[self.i + 1..];
                let fin = r.find(q as char).unwrap_or(r.len());
                self.i += 1 + (fin + 1).min(r.len());
                &r[..fin]
            }
            _ => {
                let desde = self.i;
                while self.byte(0).is_some_and(|b| !es_blanco(b) && b != b'>') {
                    self.i += 1;
                }
                &self.s[desde..self.i]
            }
        };
        (nombre, entidades::decodificar(valor, true))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn tokens(s: &str) -> Vec<Token> {
        let mut t = Tokenizador::nuevo(s);
        core::iter::from_fn(|| t.siguiente()).collect()
    }

    fn inicio(n: &str, a: &[(&str, &str)], autocierre: bool) -> Token {
        let atributos = a.iter().map(|(x, y)| (String::from(*x), String::from(*y))).collect();
        Token::Inicio { nombre: String::from(n), atributos, autocierre }
    }

    #[test]
    fn etiquetas_atributos_y_texto() {
        assert_eq!(
            tokens("<DIV Class=a id='b' data-x=\"1 &amp; 2\" hidden>Hola &lt;3</Div ><br/>"),
            [
                inicio("div", &[("class", "a"), ("id", "b"), ("data-x", "1 & 2"), ("hidden", "")], false),
                Token::Texto(String::from("Hola <3")),
                Token::Fin(String::from("div")),
                inicio("br", &[], true),
            ]
        );
        assert_eq!(tokens("a < b <3"), [Token::Texto(String::from("a < b <3"))], "un < suelto es texto");
    }

    #[test]
    fn comentarios_doctype_y_crudos() {
        assert_eq!(
            tokens("<!DOCTYPE HTML><!-- hola --><!--><?xml x?>"),
            [
                Token::Doctype(String::from("html")),
                Token::Comentario(String::from(" hola ")),
                Token::Comentario(String::new()),
                Token::Comentario(String::from("?xml x?")),
            ]
        );
        assert_eq!(
            tokens("<script>if (a<b) x='</div>'</SCRIPT><title>A &amp; B</title>"),
            [
                inicio("script", &[], false),
                Token::Texto(String::from("if (a<b) x='</div>'")),
                Token::Fin(String::from("script")),
                inicio("title", &[], false),
                Token::Texto(String::from("A & B")),
                Token::Fin(String::from("title")),
            ]
        );
    }
}
