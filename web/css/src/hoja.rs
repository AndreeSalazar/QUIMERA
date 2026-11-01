//! Hojas de estilo: reglas con sus selectores y declaraciones, con la recuperación de errores
//! de CSS (una declaración mala se salta hasta el `;`; un selector malo descarta su regla).

use crate::selectores::{self, Selector};
use crate::tokenizador::{Token, tokens};
use alloc::string::String;
use alloc::vec::Vec;

/// `propiedad: valor !important`.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaracion {
    /// En minúsculas (salvo las variables `--x`, que distinguen mayúsculas).
    pub propiedad: String,
    /// Los tokens del valor, sin blancos al principio ni al final ni el `!important`.
    pub valor: Vec<Token>,
    /// `!important`.
    pub importante: bool,
}

/// `selectores { declaraciones }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Regla {
    /// Uno o más selectores (`a, b`).
    pub selectores: Vec<Selector>,
    /// Las declaraciones en orden.
    pub declaraciones: Vec<Declaracion>,
}

/// `@media condición { reglas }`: la condición queda como texto para quien conozca la pantalla.
#[derive(Debug, Clone, PartialEq)]
pub struct ReglaMedio {
    /// `screen and (max-width: 600px)`.
    pub condicion: String,
    /// Las reglas de dentro.
    pub reglas: Vec<Regla>,
}

/// Una hoja de estilo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hoja {
    /// Las reglas normales, en orden.
    pub reglas: Vec<Regla>,
    /// Las de `@media`, en orden.
    pub medios: Vec<ReglaMedio>,
}

/// Analiza una hoja de estilo entera.
pub fn analizar(texto: &str) -> Hoja {
    let t = tokens(texto);
    let mut l = Lector { t: &t, i: 0 };
    let (reglas, medios) = l.reglas(false);
    Hoja { reglas, medios }
}

/// Analiza una lista de declaraciones sueltas (el atributo `style="..."`).
pub fn declaraciones(texto: &str) -> Vec<Declaracion> {
    let t = tokens(texto);
    lista_de_declaraciones(&t)
}

struct Lector<'a> {
    t: &'a [Token],
    i: usize,
}

impl<'a> Lector<'a> {
    fn ver(&self) -> Option<&'a Token> {
        self.t.get(self.i)
    }

    /// Reglas hasta el final (o hasta la `}` que cierra el bloque si `en_bloque`).
    fn reglas(&mut self, en_bloque: bool) -> (Vec<Regla>, Vec<ReglaMedio>) {
        let (mut reglas, mut medios) = (Vec::new(), Vec::new());
        loop {
            match self.ver() {
                None => break,
                Some(Token::Blanco) => self.i += 1,
                Some(Token::Cierra('}')) if en_bloque => {
                    self.i += 1;
                    break;
                }
                Some(Token::Arroba(nombre)) => {
                    self.i += 1;
                    if let Some(m) = self.regla_arroba(nombre) {
                        medios.push(m);
                    }
                }
                Some(_) => {
                    if let Some(r) = self.regla() {
                        reglas.push(r);
                    }
                }
            }
        }
        (reglas, medios)
    }

    /// Los tokens hasta `{` o `;` del nivel actual (sin consumirlos).
    fn preludio(&mut self) -> &'a [Token] {
        let inicio = self.i;
        let mut nivel = 0i32;
        while let Some(t) = self.ver() {
            match t {
                Token::Abre('{') | Token::PuntoYComa | Token::Cierra('}') if nivel == 0 => break,
                Token::Abre(_) | Token::Funcion(_) => nivel += 1,
                Token::Cierra(_) => nivel -= 1,
                _ => {}
            }
            self.i += 1;
        }
        &self.t[inicio..self.i]
    }

    /// El contenido de un bloque `{...}` (consume las llaves).
    fn bloque(&mut self) -> &'a [Token] {
        self.i += 1; // la `{`
        let inicio = self.i;
        let mut nivel = 0i32;
        while let Some(t) = self.ver() {
            match t {
                Token::Cierra('}') if nivel == 0 => break,
                Token::Abre(_) | Token::Funcion(_) => nivel += 1,
                Token::Cierra(_) => nivel -= 1,
                _ => {}
            }
            self.i += 1;
        }
        let dentro = &self.t[inicio..self.i];
        self.i = (self.i + 1).min(self.t.len()); // la `}`
        dentro
    }

    fn regla_arroba(&mut self, nombre: &str) -> Option<ReglaMedio> {
        let preludio = self.preludio();
        match self.ver() {
            Some(Token::Abre('{')) if nombre.eq_ignore_ascii_case("media") => {
                self.i += 1;
                let (reglas, _) = self.reglas(true);
                Some(ReglaMedio { condicion: texto(preludio), reglas })
            }
            Some(Token::Abre('{')) => {
                self.bloque(); // @font-face, @keyframes...: todavía no
                None
            }
            _ => {
                self.i += 1; // `;` de @import, @charset...
                None
            }
        }
    }

    fn regla(&mut self) -> Option<Regla> {
        let preludio = self.preludio();
        match self.ver() {
            Some(Token::Abre('{')) => {}
            Some(_) => {
                self.i += 1; // un `;` o `}` suelto: se salta
                return None;
            }
            None => return None,
        }
        let cuerpo = self.bloque();
        let selectores = selectores::lista(recortar(preludio))?;
        Some(Regla { selectores, declaraciones: lista_de_declaraciones(cuerpo) })
    }
}

/// Sin blancos al principio ni al final.
fn recortar(t: &[Token]) -> &[Token] {
    let inicio = t.iter().position(|x| *x != Token::Blanco).unwrap_or(t.len());
    let fin = t.iter().rposition(|x| *x != Token::Blanco).map_or(inicio, |p| p + 1);
    &t[inicio..fin]
}

/// Parte por los `;` de nivel superior y lee cada declaración; las malas se saltan.
fn lista_de_declaraciones(t: &[Token]) -> Vec<Declaracion> {
    let mut v = Vec::new();
    let (mut inicio, mut nivel) = (0, 0i32);
    for (i, tok) in t.iter().enumerate() {
        match tok {
            Token::Abre(_) | Token::Funcion(_) => nivel += 1,
            Token::Cierra(_) => nivel -= 1,
            Token::PuntoYComa if nivel == 0 => {
                v.extend(declaracion(&t[inicio..i]));
                inicio = i + 1;
            }
            _ => {}
        }
    }
    v.extend(declaracion(&t[inicio..]));
    v
}

fn declaracion(t: &[Token]) -> Option<Declaracion> {
    let t = recortar(t);
    let Token::Ident(nombre) = t.first()? else { return None };
    let resto = recortar(&t[1..]);
    if resto.first() != Some(&Token::DosPuntos) {
        return None;
    }
    let mut valor = recortar(&resto[1..]);
    let mut importante = false;
    if let [antes @ .., Token::Delim('!'), Token::Ident(i)]
    | [antes @ .., Token::Delim('!'), Token::Blanco, Token::Ident(i)] = valor
        && i.eq_ignore_ascii_case("important")
    {
        importante = true;
        valor = recortar(antes);
    }
    if valor.is_empty() {
        return None; // `color: ;` no vale nada: se descarta, como hacen los navegadores
    }
    let propiedad = if nombre.starts_with("--") { nombre.clone() } else { nombre.to_ascii_lowercase() };
    Some(Declaracion { propiedad, valor: valor.to_vec(), importante })
}

/// Tokens de vuelta a texto (para la condición de `@media`).
pub fn texto(t: &[Token]) -> String {
    let mut s = String::new();
    for tok in recortar(t) {
        match tok {
            Token::Ident(x) => s.push_str(x),
            Token::Funcion(x) => {
                s.push_str(x);
                s.push('(');
            }
            Token::Arroba(x) => {
                s.push('@');
                s.push_str(x);
            }
            Token::Almohadilla(x) => {
                s.push('#');
                s.push_str(x);
            }
            Token::Cadena(x) => s.push_str(&alloc::format!("\"{x}\"")),
            Token::Url(x) => s.push_str(&alloc::format!("url({x})")),
            Token::Numero(n) => s.push_str(&alloc::format!("{n}")),
            Token::Porcentaje(n) => s.push_str(&alloc::format!("{n}%")),
            Token::Dimension(n, u) => s.push_str(&alloc::format!("{n}{u}")),
            Token::Blanco => s.push(' '),
            Token::Delim(c) | Token::Abre(c) | Token::Cierra(c) => s.push(*c),
            Token::DosPuntos => s.push(':'),
            Token::PuntoYComa => s.push(';'),
            Token::Coma => s.push(','),
        }
    }
    s
}
