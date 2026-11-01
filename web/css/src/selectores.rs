//! Selectores: `div.nota > p:first-child`, su especificidad y si casan con un elemento.
//!
//! El emparejamiento no conoce el árbol: pregunta lo que necesita a través del rasgo
//! [`ElementoCss`], así sirve igual para el árbol de `quimera-html` que para el DOM futuro.

use crate::tokenizador::Token;
use alloc::string::String;
use alloc::vec::Vec;

/// Lo que el emparejamiento necesita saber de un elemento.
pub trait ElementoCss: Copy {
    /// Nombre en minúsculas.
    fn nombre(&self) -> &str;
    /// Un atributo.
    fn atributo(&self, nombre: &str) -> Option<&str>;
    /// El elemento padre (ninguno para la raíz `<html>`).
    fn padre(&self) -> Option<Self>;
    /// El hermano elemento anterior.
    fn anterior(&self) -> Option<Self>;
    /// El hermano elemento siguiente.
    fn siguiente(&self) -> Option<Self>;
    /// Si no tiene hijos (ni elementos ni texto).
    fn vacio(&self) -> bool;
    /// Si `class` contiene esta clase.
    fn tiene_clase(&self, clase: &str) -> bool {
        self.atributo("class").is_some_and(|c| c.split_ascii_whitespace().any(|x| x == clase))
    }
}

/// Cómo se relacionan dos partes de un selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinador {
    /// `a b`
    Descendiente,
    /// `a > b`
    Hijo,
    /// `a + b`
    Adyacente,
    /// `a ~ b`
    Hermano,
}

/// `[atributo]`, `[atributo=valor]`...
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atributo {
    /// `[a]`
    Existe(String),
    /// `[a=v]`
    Igual(String, String),
    /// `[a~=v]`: una de sus palabras.
    Palabra(String, String),
    /// `[a^=v]`
    Prefijo(String, String),
    /// `[a$=v]`
    Sufijo(String, String),
    /// `[a*=v]`
    Contiene(String, String),
    /// `[a|=v]`: `v` o `v-...`.
    Guion(String, String),
}

/// Pseudoclases que entiende el estilo estático.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pseudo {
    /// `:first-child`
    PrimerHijo,
    /// `:last-child`
    UltimoHijo,
    /// `:only-child`
    UnicoHijo,
    /// `:root`
    Raiz,
    /// `:empty`
    Vacio,
    /// `:link`, `:any-link`: un `<a href>`.
    Enlace,
    /// Lo que depende de la interacción o no se soporta (`:hover`, `::before`): no casa.
    Nunca,
}

/// Una parte sin combinadores: `div#a.b[c]:first-child`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Compuesto {
    /// La etiqueta (`None` para `*` o si no se dice).
    pub etiqueta: Option<String>,
    /// `#id`
    pub ids: Vec<String>,
    /// `.clase`
    pub clases: Vec<String>,
    /// `[...]`
    pub atributos: Vec<Atributo>,
    /// `:...`
    pub pseudos: Vec<Pseudo>,
    /// Pseudoelementos (`::before`): cuentan como etiqueta en la especificidad.
    pub pseudoelementos: u32,
}

/// Un selector complejo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    /// Las partes de izquierda a derecha.
    pub partes: Vec<Compuesto>,
    /// `combinadores[i]` une `partes[i]` con `partes[i + 1]`.
    pub combinadores: Vec<Combinador>,
}

impl Selector {
    /// Especificidad (ids, clases/atributos/pseudoclases, etiquetas/pseudoelementos).
    pub fn especificidad(&self) -> (u32, u32, u32) {
        self.partes.iter().fold((0, 0, 0), |(a, b, c), p| {
            let b2 = (p.clases.len() + p.atributos.len() + p.pseudos.len()) as u32;
            (a + p.ids.len() as u32, b + b2, c + u32::from(p.etiqueta.is_some()) + p.pseudoelementos)
        })
    }

    /// Si el selector casa con el elemento.
    pub fn casa<E: ElementoCss>(&self, e: E) -> bool {
        !self.partes.is_empty() && self.casa_desde(self.partes.len() - 1, e)
    }

    fn casa_desde<E: ElementoCss>(&self, i: usize, e: E) -> bool {
        if !casa_compuesto(&self.partes[i], e) {
            return false;
        }
        if i == 0 {
            return true;
        }
        let siguiente_a_probar = |x: E| self.casa_desde(i - 1, x);
        match self.combinadores[i - 1] {
            Combinador::Hijo => e.padre().is_some_and(siguiente_a_probar),
            Combinador::Adyacente => e.anterior().is_some_and(siguiente_a_probar),
            Combinador::Descendiente => core::iter::successors(e.padre(), |x| x.padre()).any(siguiente_a_probar),
            Combinador::Hermano => core::iter::successors(e.anterior(), |x| x.anterior()).any(siguiente_a_probar),
        }
    }
}

fn casa_compuesto<E: ElementoCss>(c: &Compuesto, e: E) -> bool {
    if c.pseudoelementos > 0 {
        return false; // las reglas de ::before/::after no son del elemento
    }
    if c.etiqueta.as_ref().is_some_and(|t| t != e.nombre()) {
        return false;
    }
    if c.ids.iter().any(|id| e.atributo("id") != Some(id.as_str())) {
        return false;
    }
    if c.clases.iter().any(|k| !e.tiene_clase(k)) {
        return false;
    }
    c.atributos.iter().all(|a| casa_atributo(a, e)) && c.pseudos.iter().all(|p| casa_pseudo(*p, e))
}

fn casa_atributo<E: ElementoCss>(a: &Atributo, e: E) -> bool {
    let valor = |n: &str| e.atributo(n);
    match a {
        Atributo::Existe(n) => valor(n).is_some(),
        Atributo::Igual(n, v) => valor(n) == Some(v.as_str()),
        Atributo::Palabra(n, v) => valor(n).is_some_and(|x| x.split_ascii_whitespace().any(|p| p == v)),
        Atributo::Prefijo(n, v) => !v.is_empty() && valor(n).is_some_and(|x| x.starts_with(v.as_str())),
        Atributo::Sufijo(n, v) => !v.is_empty() && valor(n).is_some_and(|x| x.ends_with(v.as_str())),
        Atributo::Contiene(n, v) => !v.is_empty() && valor(n).is_some_and(|x| x.contains(v.as_str())),
        Atributo::Guion(n, v) => {
            valor(n).is_some_and(|x| x == v || x.strip_prefix(v.as_str()).is_some_and(|r| r.starts_with('-')))
        }
    }
}

fn casa_pseudo<E: ElementoCss>(p: Pseudo, e: E) -> bool {
    match p {
        Pseudo::PrimerHijo => e.anterior().is_none(),
        Pseudo::UltimoHijo => e.siguiente().is_none(),
        Pseudo::UnicoHijo => e.anterior().is_none() && e.siguiente().is_none(),
        Pseudo::Raiz => e.padre().is_none(),
        Pseudo::Vacio => e.vacio(),
        Pseudo::Enlace => e.nombre() == "a" && e.atributo("href").is_some(),
        Pseudo::Nunca => false,
    }
}

/// Analiza una lista de selectores (`a, b > c`). `None` si alguno no es válido: la regla
/// entera se descarta, como manda CSS.
pub fn lista(tokens: &[Token]) -> Option<Vec<Selector>> {
    tokens.split(|t| *t == Token::Coma).map(complejo).collect()
}

fn complejo(tokens: &[Token]) -> Option<Selector> {
    let mut l = LectorSel { t: tokens, i: 0 };
    let mut sel = Selector { partes: Vec::new(), combinadores: Vec::new() };
    l.blancos();
    loop {
        sel.partes.push(l.compuesto()?);
        let hubo_blanco = l.blancos();
        let combinador = match l.ver() {
            None => break,
            Some(Token::Delim('>')) => Combinador::Hijo,
            Some(Token::Delim('+')) => Combinador::Adyacente,
            Some(Token::Delim('~')) => Combinador::Hermano,
            Some(_) if hubo_blanco => Combinador::Descendiente,
            Some(_) => return None,
        };
        if combinador != Combinador::Descendiente {
            l.i += 1;
            l.blancos();
        }
        sel.combinadores.push(combinador);
    }
    Some(sel)
}

struct LectorSel<'a> {
    t: &'a [Token],
    i: usize,
}

impl<'a> LectorSel<'a> {
    fn ver(&self) -> Option<&'a Token> {
        self.t.get(self.i)
    }

    fn blancos(&mut self) -> bool {
        let inicio = self.i;
        while self.ver() == Some(&Token::Blanco) {
            self.i += 1;
        }
        self.i > inicio
    }

    fn ident(&mut self) -> Option<String> {
        match self.ver()? {
            Token::Ident(n) => {
                let n = n.clone();
                self.i += 1;
                Some(n)
            }
            _ => None,
        }
    }

    fn compuesto(&mut self) -> Option<Compuesto> {
        let mut c = Compuesto::default();
        let antes = self.i;
        match self.ver() {
            Some(Token::Ident(n)) => {
                c.etiqueta = Some(n.to_ascii_lowercase());
                self.i += 1;
            }
            Some(Token::Delim('*')) => self.i += 1,
            _ => {}
        }
        loop {
            match self.ver() {
                Some(Token::Almohadilla(id)) => {
                    c.ids.push(id.clone());
                    self.i += 1;
                }
                Some(Token::Delim('.')) => {
                    self.i += 1;
                    c.clases.push(self.ident()?);
                }
                Some(Token::Abre('[')) => {
                    self.i += 1;
                    c.atributos.push(self.atributo()?);
                }
                Some(Token::DosPuntos) => self.pseudo(&mut c)?,
                _ => break,
            }
        }
        // Un compuesto tiene que tener algo (`*` cuenta).
        if self.i == antes { None } else { Some(c) }
    }

    /// `[nombre]`, `[nombre op valor]` (tras el `[`).
    fn atributo(&mut self) -> Option<Atributo> {
        self.blancos();
        let nombre = self.ident()?.to_ascii_lowercase();
        self.blancos();
        let op = match self.ver()? {
            Token::Cierra(']') => {
                self.i += 1;
                return Some(Atributo::Existe(nombre));
            }
            Token::Delim('=') => '=',
            Token::Delim(c @ ('~' | '^' | '$' | '*' | '|')) if self.t.get(self.i + 1) == Some(&Token::Delim('=')) => {
                self.i += 1;
                *c
            }
            _ => return None,
        };
        self.i += 1;
        self.blancos();
        let valor = match self.ver()? {
            Token::Ident(v) | Token::Cadena(v) => v.clone(),
            _ => return None,
        };
        self.i += 1;
        self.blancos();
        if self.ver() == Some(&Token::Ident(String::from("i"))) {
            self.i += 1; // `[a=b i]`: sin distinguir mayúsculas (se acepta, sin efecto)
            self.blancos();
        }
        if self.ver() != Some(&Token::Cierra(']')) {
            return None;
        }
        self.i += 1;
        Some(match op {
            '=' => Atributo::Igual(nombre, valor),
            '~' => Atributo::Palabra(nombre, valor),
            '^' => Atributo::Prefijo(nombre, valor),
            '$' => Atributo::Sufijo(nombre, valor),
            '*' => Atributo::Contiene(nombre, valor),
            _ => Atributo::Guion(nombre, valor),
        })
    }

    /// `:pseudo`, `::elemento` o `:funcion(...)` (tras ver los dos puntos).
    fn pseudo(&mut self, c: &mut Compuesto) -> Option<()> {
        self.i += 1;
        if self.ver() == Some(&Token::DosPuntos) {
            self.i += 1;
            self.ident()?;
            c.pseudoelementos += 1;
            return Some(());
        }
        let p = match self.ver()? {
            Token::Ident(n) => match n.to_ascii_lowercase().as_str() {
                "first-child" => Pseudo::PrimerHijo,
                "last-child" => Pseudo::UltimoHijo,
                "only-child" => Pseudo::UnicoHijo,
                "root" => Pseudo::Raiz,
                "empty" => Pseudo::Vacio,
                "link" | "any-link" => Pseudo::Enlace,
                // CSS 2: `:before` y `:after` con un solo `:` son pseudoelementos.
                "before" | "after" | "first-line" | "first-letter" => {
                    self.i += 1;
                    c.pseudoelementos += 1;
                    return Some(());
                }
                _ => Pseudo::Nunca,
            },
            Token::Funcion(_) => {
                // `:not(...)`, `:nth-child(...)`...: todavía no; se salta su contenido.
                let mut nivel = 0;
                loop {
                    match self.ver()? {
                        Token::Funcion(_) | Token::Abre('(') => nivel += 1,
                        Token::Cierra(')') => nivel -= 1,
                        _ => {}
                    }
                    self.i += 1;
                    if nivel == 0 {
                        break;
                    }
                }
                c.pseudos.push(Pseudo::Nunca);
                return Some(());
            }
            _ => return None,
        };
        self.i += 1;
        c.pseudos.push(p);
        Some(())
    }
}
