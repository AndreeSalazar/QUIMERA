//! Una regla de red de EasyList: dónde casa, con qué opciones y por qué trozo se indexa.
//!
//! Las reglas no copian su texto: apuntan a la lista incluida en el binario (`&'static str`).
//! Con unas 20.000 reglas de ruta y 87.000 dominios, copiar cada una costaría megas de RAM.

use crate::bloqueo::{Contexto, Tipo, es_del_dominio, sitio};
use quimera_regex::{Memoria, Regex};

/// La URL que se pregunta, ya preparada una vez para todas las reglas.
pub(crate) struct Pedido<'u> {
    /// La URL entera tal cual (para las expresiones regulares con `match-case`).
    pub original: &'u str,
    /// La URL entera en minúsculas.
    pub entera: &'u str,
    /// El anfitrión en minúsculas.
    pub anfitrion: &'u str,
    /// Lo que sigue al anfitrión (ruta y consulta), en minúsculas.
    pub resto: &'u str,
}

/// Las opciones de una regla (lo que va tras `$`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Opciones {
    /// `Some(true)`: solo a otro sitio; `Some(false)`: solo al mismo.
    tercero: Option<bool>,
    /// Los tipos a los que se aplica (0: todos) y los que no.
    tipos: u16,
    sin_tipos: u16,
    /// `domain=` tal cual (`a.com|~b.com`), o vacío.
    dominios: &'static str,
    /// `$document` en una excepción: la página entera sin bloqueo.
    pub documento: bool,
    /// `$match-case`: distingue mayúsculas (solo cuenta en las expresiones regulares).
    mayusculas: bool,
}

impl Opciones {
    /// Lee `third-party,script,domain=a.com|~b.com`. `None` si alguna no se entiende.
    fn leer(texto: &'static str) -> Option<Opciones> {
        let mut o = Opciones::default();
        for op in texto.split(',').map(str::trim) {
            let (no, nombre) = op.strip_prefix('~').map_or((false, op), |r| (true, r));
            match nombre.to_ascii_lowercase().as_str() {
                "third-party" | "3p" => o.tercero = Some(!no),
                "first-party" | "1p" => o.tercero = Some(no),
                "document" | "doc" if !no => o.documento = true,
                "match-case" => o.mayusculas = true,
                "important" => {}
                n if n.starts_with("domain=") && !no => o.dominios = &nombre["domain=".len()..],
                n => {
                    let t = Tipo::de_nombre(n)?;
                    if no { o.sin_tipos |= t.bit() } else { o.tipos |= t.bit() }
                }
            }
        }
        Some(o)
    }

    /// Si la regla vale para esta petición (tipo, sitio y página).
    fn aplica(&self, anfitrion: &str, c: &Contexto) -> bool {
        let bit = c.tipo.bit();
        if (self.tipos != 0 && self.tipos & bit == 0) || self.sin_tipos & bit != 0 {
            return false;
        }
        let necesita_pagina = self.tercero.is_some() || !self.dominios.is_empty();
        let Some(pagina) = c.pagina else { return !necesita_pagina };
        let pagina = pagina.to_ascii_lowercase();
        if let Some(t) = self.tercero
            && (sitio(anfitrion) != sitio(&pagina)) != t
        {
            return false;
        }
        en_dominios(self.dominios, &pagina)
    }
}

/// Si `pagina` cumple una lista `a.com|~b.com` (vacía: siempre). Con algún dominio sin `~`,
/// tiene que estar en uno de ellos; nunca en uno con `~`.
pub(crate) fn en_dominios(lista: &str, pagina: &str) -> bool {
    let mut hay_positivos = false;
    let mut dentro = false;
    for d in lista.split(['|', ',']).map(str::trim).filter(|d| !d.is_empty()) {
        match d.strip_prefix('~') {
            Some(n) if es_del_dominio(pagina, &n.to_ascii_lowercase()) => return false,
            Some(_) => {}
            None => {
                hay_positivos = true;
                dentro |= es_del_dominio(pagina, &d.to_ascii_lowercase());
            }
        }
    }
    !hay_positivos || dentro
}

/// Dónde empieza a mirar una regla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ancla {
    /// `||dominio`: el anfitrión es ese dominio o un subdominio, y el texto sigue a él.
    Dominio(&'static str),
    /// `|http://`: la URL empieza por el texto.
    Inicio,
    /// En cualquier sitio de la URL.
    Ninguna,
}

/// Una regla de red.
#[derive(Debug, Clone)]
pub(crate) struct Regla {
    pub ancla: Ancla,
    /// El patrón (con `*` y `^`), sin las anclas.
    pub texto: &'static str,
    /// `|` al final: la URL tiene que acabar ahí.
    pub fin: bool,
    /// `/…/`: una expresión regular (la de JavaScript, con `quimera-regex`).
    pub regex: Option<Box<Regex>>,
    pub opciones: Opciones,
}

impl Regla {
    /// Lee una regla sin `@@`. `None` si no se entiende (una opción desconocida, una expresión
    /// regular que no compila...).
    pub fn leer(regla: &'static str) -> Option<Regla> {
        let (patron, opciones) = match regla.rsplit_once('$') {
            // Un `$` dentro de una expresión regular no separa opciones.
            Some((p, o)) if !o.contains('/') || p.ends_with('/') => (p, Opciones::leer(o)?),
            _ => (regla, Opciones::default()),
        };
        if patron.len() > 2 && patron.starts_with('/') && patron.ends_with('/') {
            let banderas = if opciones.mayusculas { "" } else { "i" };
            let r = Regex::nueva(&patron[1..patron.len() - 1], banderas).ok()?;
            return Some(Regla { ancla: Ancla::Ninguna, texto: "", fin: false, regex: Some(Box::new(r)), opciones });
        }
        let (ancla, resto) = match (patron.strip_prefix("||"), patron.strip_prefix('|')) {
            (Some(r), _) => {
                let (d, t) = r.split_at(r.find(['^', '/', '*', '|', ':', '?']).unwrap_or(r.len()));
                (Ancla::Dominio(d), t)
            }
            (None, Some(r)) => (Ancla::Inicio, r),
            (None, None) => (Ancla::Ninguna, patron),
        };
        let (texto, fin) = resto.strip_suffix('|').map_or((resto, false), |t| (t, true));
        let vacia =
            matches!(ancla, Ancla::Dominio("")) || (ancla != Ancla::Inicio && texto.trim_matches('*').is_empty());
        (!vacia).then_some(Regla { ancla, texto, fin, regex: None, opciones })
    }

    /// Si casa con la URL (sin mirar las opciones).
    fn casa(&self, p: &Pedido) -> bool {
        if let Some(r) = &self.regex {
            let texto = if self.opciones.mayusculas { p.original } else { p.entera };
            return r.buscar(texto, 0, &mut Memoria::default());
        }
        match self.ancla {
            Ancla::Dominio(d) => es_del_dominio(p.anfitrion, d) && casa_en(p.resto, self.texto, self.fin),
            Ancla::Inicio => casa_en(p.entera, self.texto, self.fin),
            Ancla::Ninguna => {
                let b = p.entera.as_bytes();
                // Solo se prueba donde está su primer carácter (si no empieza por `*` o `^`).
                let primero = self.texto.bytes().next().filter(|&c| c != b'*' && c != b'^');
                (0..=b.len()).any(|i| {
                    primero.is_none_or(|c| b.get(i).is_some_and(|x| x.eq_ignore_ascii_case(&c)))
                        && casa_bytes(&b[i..], self.texto.as_bytes(), self.fin)
                })
            }
        }
    }

    /// Si la regla vale para la URL en este contexto.
    pub fn aplica(&self, p: &Pedido, c: &Contexto) -> bool {
        self.opciones.aplica(p.anfitrion, c) && self.casa(p)
    }

    /// El trozo por el que se indexa: el más largo que seguro aparece entero en la URL
    /// (rodeado de separadores). `None`: la regla se mira con todas las URLs.
    pub fn trozo(&self) -> Option<u64> {
        if self.regex.is_some() {
            return None;
        }
        let mut mejor: Option<&str> = None;
        let mut elegir = |t: &'static str| {
            if t.len() >= 2 && mejor.is_none_or(|m| t.len() > m.len()) {
                mejor = Some(t);
            }
        };
        if let Ancla::Dominio(d) = self.ancla {
            d.split(|c: char| !c.is_ascii_alphanumeric()).for_each(&mut elegir);
        }
        let b = self.texto.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if !b[i].is_ascii_alphanumeric() {
                i += 1;
                continue;
            }
            let fin = (i..b.len()).find(|&k| !b[k].is_ascii_alphanumeric()).unwrap_or(b.len());
            let antes = if i == 0 { self.ancla != Ancla::Ninguna } else { b[i - 1] != b'*' };
            let despues = if fin == b.len() { self.fin } else { b[fin] != b'*' };
            if antes && despues {
                elegir(&self.texto[i..fin]);
            }
            i = fin;
        }
        mejor.map(picadillo)
    }
}

/// El número de un trozo (FNV-1a en minúsculas): el índice no guarda texto.
pub(crate) fn picadillo(t: &str) -> u64 {
    t.bytes().fold(0xcbf2_9ce4_8422_2325, |h, c| (h ^ u64::from(c.to_ascii_lowercase())).wrapping_mul(0x100_0000_01b3))
}

/// Si `texto` empieza con `patron`, donde `^` vale por un separador o por el final, y `*` por
/// cualquier trozo (también vacío). Con `fin`, el texto tiene que acabar donde acaba el patrón.
pub(crate) fn casa_en(texto: &str, patron: &str, fin: bool) -> bool {
    casa_bytes(texto.as_bytes(), patron.as_bytes(), fin)
}

fn casa_bytes(mut t: &[u8], mut p: &[u8], fin: bool) -> bool {
    while let Some((&c, resto)) = p.split_first() {
        match (c, t.split_first()) {
            (b'*', _) => return (0..=t.len()).any(|i| casa_bytes(&t[i..], resto, fin)),
            (b'^', None) => return resto.iter().all(|&x| x == b'*' || x == b'^'),
            (b'^', Some((&x, r))) if separador(x) => t = r,
            (_, Some((&x, r))) if x.eq_ignore_ascii_case(&c) => t = r,
            _ => return false,
        }
        p = resto;
    }
    !fin || t.is_empty()
}

fn separador(c: u8) -> bool {
    !(c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'%'))
}
