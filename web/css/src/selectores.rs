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
    /// El primer hijo elemento (para `:has()`; sin él, `:has()` no casa).
    fn primer_hijo(&self) -> Option<Self> {
        None
    }
    /// Si es el mismo elemento que `otro` (para `:has()`).
    fn mismo(&self, _otro: &Self) -> bool {
        false
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Lo que depende de la interacción o no se soporta (`:hover`, `:has()`): no casa.
    Nunca,
    /// `:not(lista)`: no casa ninguno.
    No(Vec<Selector>),
    /// `:has(> a, + b, c)`: algún elemento con esa relación casa (ver `tiene.rs`).
    Tiene(Vec<crate::tiene::Relativo>),
    /// `:is(lista)` y `:where(lista)` (este sin especificidad: `false`): casa alguno.
    Alguno(Vec<Selector>, bool),
    /// `:nth-child(an+b)` y familia: su puesto entre los hermanos (de su `tipo`, si lo dice;
    /// contando desde el `final`, si lo dice) es `a·n + b` para algún `n ≥ 0`.
    Enesimo {
        /// El paso (`a` de `an+b`).
        a: i32,
        /// El desplazamiento (`b`).
        b: i32,
        /// Solo cuentan los hermanos de su misma etiqueta (`-of-type`).
        tipo: bool,
        /// Se cuenta desde el último (`nth-last-`).
        desde_el_final: bool,
    },
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
    /// Cuál (`before`, `after`...), en minúsculas.
    pub pseudoelemento: Option<String>,
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
            let b2 = (p.clases.len() + p.atributos.len()) as u32;
            // `:not()` e `:is()` valen lo que el más específico de su lista; `:where()`, nada.
            let (pa, pb, pc) = p.pseudos.iter().fold((0, 0, 0), |(x, y, z), ps| {
                let (i, j, k) = match ps {
                    Pseudo::No(l) | Pseudo::Alguno(l, true) => {
                        l.iter().map(Selector::especificidad).max().unwrap_or((0, 0, 0))
                    }
                    Pseudo::Alguno(_, false) => (0, 0, 0),
                    Pseudo::Tiene(l) => l.iter().map(|r| r.selector.especificidad()).max().unwrap_or((0, 0, 0)),
                    _ => (0, 1, 0),
                };
                (x + i, y + j, z + k)
            });
            (a + p.ids.len() as u32 + pa, b + b2 + pb, c + u32::from(p.etiqueta.is_some()) + p.pseudoelementos + pc)
        })
    }

    /// Si el selector casa con el elemento. Los de `::before`/`::after` casan con el hijo
    /// sintético `::before`/`::after` que el estilo mete en su elemento (ver `quimera-estilo`).
    pub fn casa<E: ElementoCss>(&self, e: E) -> bool {
        let Some(ultimo) = self.partes.last() else { return false };
        match (ultimo.pseudoelemento.as_deref(), e.nombre().strip_prefix("::")) {
            (Some(p), Some(n)) if p == n => {
                e.padre().is_some_and(|origen| self.casa_desde(self.partes.len() - 1, origen))
            }
            (None, None) => self.casa_desde(self.partes.len() - 1, e),
            _ => false,
        }
    }

    /// El pseudoelemento del selector (`before`, `after`...), si lo tiene.
    pub fn seudoelemento(&self) -> Option<&str> {
        self.partes.last()?.pseudoelemento.as_deref()
    }

    /// Si casa con `e` como elemento de origen de su pseudoelemento (`p::before` con un `<p>`).
    pub fn casa_origen<E: ElementoCss>(&self, e: E) -> bool {
        !self.partes.is_empty() && self.casa_desde(self.partes.len() - 1, e)
    }

    /// Si casa con `e` y su primera parte está en relación `c` con `ancla` (`:has()`).
    pub(crate) fn casa_anclado<E: ElementoCss>(&self, e: E, ancla: E, c: Combinador) -> bool {
        !self.partes.is_empty() && self.casa_con(self.partes.len() - 1, e, Some((ancla, c)))
    }

    fn casa_desde<E: ElementoCss>(&self, i: usize, e: E) -> bool {
        self.casa_con(i, e, None)
    }

    fn casa_con<E: ElementoCss>(&self, i: usize, e: E, ancla: Option<(E, Combinador)>) -> bool {
        if !casa_compuesto(&self.partes[i], e) {
            return false;
        }
        if i == 0 {
            return ancla.is_none_or(|(a, c)| crate::tiene::relacion(a, e, c));
        }
        let siguiente_a_probar = |x: E| self.casa_con(i - 1, x, ancla);
        match self.combinadores[i - 1] {
            Combinador::Hijo => e.padre().is_some_and(siguiente_a_probar),
            Combinador::Adyacente => e.anterior().is_some_and(siguiente_a_probar),
            Combinador::Descendiente => core::iter::successors(e.padre(), |x| x.padre()).any(siguiente_a_probar),
            Combinador::Hermano => core::iter::successors(e.anterior(), |x| x.anterior()).any(siguiente_a_probar),
        }
    }
}

fn casa_compuesto<E: ElementoCss>(c: &Compuesto, e: E) -> bool {
    if c.pseudoelementos > 0 && c.pseudoelemento.is_none() {
        return false; // `::first-line` y demás: todavía no
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
    c.atributos.iter().all(|a| casa_atributo(a, e)) && c.pseudos.iter().all(|p| casa_pseudo(p, e))
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

fn casa_pseudo<E: ElementoCss>(p: &Pseudo, e: E) -> bool {
    match p {
        Pseudo::PrimerHijo => e.anterior().is_none(),
        Pseudo::UltimoHijo => e.siguiente().is_none(),
        Pseudo::UnicoHijo => e.anterior().is_none() && e.siguiente().is_none(),
        Pseudo::Raiz => e.padre().is_none(),
        Pseudo::Vacio => e.vacio(),
        Pseudo::Enlace => e.nombre() == "a" && e.atributo("href").is_some(),
        Pseudo::Nunca => false,
        Pseudo::No(l) => !l.iter().any(|s| s.casa_origen(e)),
        Pseudo::Alguno(l, _) => l.iter().any(|s| s.casa_origen(e)),
        Pseudo::Tiene(l) => crate::tiene::casa(l, e),
        &Pseudo::Enesimo { a, b, tipo, desde_el_final } => {
            let mismo = |x: &E| !tipo || x.nombre() == e.nombre();
            let antes = if desde_el_final {
                core::iter::successors(e.siguiente(), |x| x.siguiente()).filter(mismo).count()
            } else {
                core::iter::successors(e.anterior(), |x| x.anterior()).filter(mismo).count()
            };
            let puesto = antes as i32 + 1;
            if a == 0 { puesto == b } else { (puesto - b) % a == 0 && (puesto - b) / a >= 0 }
        }
    }
}

/// `an+b` (`odd`, `even`, `3`, `-n+2`, `2n + 1`...) como (a, b).
fn an_mas_b(tokens: &[Token]) -> Option<(i32, i32)> {
    let mut s = String::new();
    for t in tokens {
        match t {
            Token::Ident(x) => s.push_str(x),
            Token::Numero(n) => s.push_str(&alloc::format!("{:+}", *n as i64)),
            Token::Dimension(n, u) => s.push_str(&alloc::format!("{}{u}", *n as i64)),
            Token::Delim(c) => s.push(*c),
            Token::Blanco => {}
            _ => return None,
        }
    }
    let s = s.to_ascii_lowercase().replace("++", "+").replace("+-", "-").replace("-+", "-");
    match s.as_str() {
        "odd" => return Some((2, 1)),
        "even" => return Some((2, 0)),
        _ => {}
    }
    let Some((a, b)) = s.split_once('n') else { return Some((0, s.parse().ok()?)) };
    let a = match a {
        "" | "+" => 1,
        "-" => -1,
        x => x.parse().ok()?,
    };
    let b = if b.is_empty() { 0 } else { b.parse().ok()? };
    Some((a, b))
}

/// `:nombre(dentro)`. `None` si no es válido (un `:not()` o un `an+b` mal escritos).
fn pseudo_funcion(nombre: &str, dentro: &[Token]) -> Option<Pseudo> {
    let enesimo = |tipo: bool, desde_el_final: bool| {
        // `:nth-child(2n of .x)`: todavía no.
        if dentro.iter().any(|t| matches!(t, Token::Ident(i) if i.eq_ignore_ascii_case("of"))) {
            return Some(Pseudo::Nunca);
        }
        an_mas_b(dentro).map(|(a, b)| Pseudo::Enesimo { a, b, tipo, desde_el_final })
    };
    match nombre {
        "not" => lista(dentro).map(Pseudo::No),
        // Estas perdonan: con una lista que no se entiende, no casan.
        "is" | "matches" | "-webkit-any" | "any" => {
            Some(lista(dentro).map_or(Pseudo::Nunca, |l| Pseudo::Alguno(l, true)))
        }
        "has" => crate::tiene::leer(dentro).map(Pseudo::Tiene),
        "where" => Some(lista(dentro).map_or(Pseudo::Nunca, |l| Pseudo::Alguno(l, false))),
        "nth-child" => enesimo(false, false),
        "nth-last-child" => enesimo(false, true),
        "nth-of-type" => enesimo(true, false),
        "nth-last-of-type" => enesimo(true, true),
        _ => Some(Pseudo::Nunca),
    }
}

/// Analiza una lista de selectores (`a, b > c`). `None` si alguno no es válido: la regla
/// entera se descarta, como manda CSS.
pub fn lista(tokens: &[Token]) -> Option<Vec<Selector>> {
    // Las comas de dentro de `:not(.a, .b)` no separan.
    let mut nivel = 0i32;
    tokens
        .split(|t| {
            match t {
                Token::Funcion(_) | Token::Abre('(') => nivel += 1,
                Token::Cierra(')') => nivel -= 1,
                _ => {}
            }
            nivel == 0 && *t == Token::Coma
        })
        .map(complejo)
        .collect()
}

pub(crate) fn complejo(tokens: &[Token]) -> Option<Selector> {
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
            let nombre = self.ident()?.to_ascii_lowercase();
            c.pseudoelementos += 1;
            if nombre == "before" || nombre == "after" {
                c.pseudoelemento = Some(nombre);
            }
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
                "first-of-type" => Pseudo::Enesimo { a: 0, b: 1, tipo: true, desde_el_final: false },
                "last-of-type" => Pseudo::Enesimo { a: 0, b: 1, tipo: true, desde_el_final: true },
                "only-of-type" => {
                    c.pseudos.push(Pseudo::Enesimo { a: 0, b: 1, tipo: true, desde_el_final: false });
                    Pseudo::Enesimo { a: 0, b: 1, tipo: true, desde_el_final: true }
                }
                // CSS 2: `:before` y `:after` con un solo `:` son pseudoelementos.
                "before" | "after" | "first-line" | "first-letter" => {
                    let nombre = n.to_ascii_lowercase();
                    self.i += 1;
                    c.pseudoelementos += 1;
                    if nombre == "before" || nombre == "after" {
                        c.pseudoelemento = Some(nombre);
                    }
                    return Some(());
                }
                _ => Pseudo::Nunca,
            },
            Token::Funcion(nombre) => {
                let nombre = nombre.to_ascii_lowercase();
                let inicio = self.i + 1;
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
                let dentro = &self.t[inicio..self.i - 1];
                c.pseudos.push(pseudo_funcion(&nombre, dentro)?);
                return Some(());
            }
            _ => return None,
        };
        self.i += 1;
        c.pseudos.push(p);
        Some(())
    }
}
