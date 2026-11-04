//! El bloqueador de anuncios y rastreo, como el escudo de Brave: una lista de reglas al estilo
//! EasyList, incluida en el binario (no se descarga nada), y un filtro que dice si una URL se pide.
//!
//! Reglas de red que se entienden (un subconjunto de la sintaxis de Adblock Plus):
//!
//! - `||dominio^` — el dominio y sus subdominios (`||doubleclick.net^` vale para
//!   `googleads.g.doubleclick.net`). Con ruta detrás (`||youtube.com/pagead/`), la URL tiene que
//!   empezar por ese anfitrión y esa ruta.
//! - cualquier otro texto (`/pagead/*`) — se busca en la URL entera, sin el esquema.
//! - `^` es un separador: cualquier carácter que no sea letra, número, `_ - . %`, o el final;
//!   `*` es cualquier trozo. Un texto entre barras (`/…/`) es una expresión regular, por eso las
//!   rutas se escriben con `*` detrás (`/pagead/*`), como en EasyList.
//! - `@@regla` — excepción: lo que coincide se pide aunque otra regla lo bloquee. Con
//!   `$document`, la página entera queda sin bloqueo.
//! - **Opciones** tras `$`, separadas por comas ([`Opciones`]): `third-party` (`3p`) y
//!   `~third-party` (`1p`), los tipos (`script`, `image`, `stylesheet`, `xmlhttprequest`, `font`,
//!   `media`, `subdocument`, `other`, y su contrario con `~`) y `domain=a.com|~b.com`.
//!   Una regla con una opción que no se entiende se ignora entera: mejor no bloquear de más.
//! - `!` empieza un comentario. Las reglas de ocultar elementos (`##`) las lee
//!   [`crate::cosmetico`]; las expresiones regulares (`/…/`) todavía se ignoran.
//!
//! Lo bloqueado no sale a la red: el `Cliente` lo responde como un fallo de red
//! ([`crate::Error::Bloqueado`]), igual que Brave, para que la página siga funcionando.

use crate::Url;
use crate::cosmetico::Cosmeticas;
use std::collections::HashSet;

/// La lista incluida: anuncios y rastreo, elegida para no romper los sitios que los sirven.
pub const LISTA: &str = include_str!("bloqueo.txt");

/// Qué es lo que se pide (para las opciones `$script`, `$image`...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tipo {
    /// `<script src>` y los scripts que añade la página.
    Script,
    /// `<img>`.
    Imagen,
    /// `<link rel=stylesheet>`.
    Hoja,
    /// `@font-face`.
    Fuente,
    /// `fetch` y `XMLHttpRequest`.
    Xhr,
    /// Vídeo y sonido.
    Medio,
    /// `<iframe>`.
    Subdocumento,
    /// Lo que no se sabe.
    #[default]
    Otro,
}

impl Tipo {
    fn bit(self) -> u16 {
        1 << self as u16
    }

    fn de_nombre(n: &str) -> Option<Tipo> {
        Some(match n {
            "script" => Tipo::Script,
            "image" => Tipo::Imagen,
            "stylesheet" | "css" => Tipo::Hoja,
            "font" => Tipo::Fuente,
            "xmlhttprequest" | "xhr" => Tipo::Xhr,
            "media" => Tipo::Medio,
            "subdocument" | "frame" => Tipo::Subdocumento,
            "other" => Tipo::Otro,
            _ => return None,
        })
    }
}

/// Desde dónde se pide: el tipo y el anfitrión de la página (si se sabe).
#[derive(Debug, Clone, Copy, Default)]
pub struct Contexto<'a> {
    /// Qué se pide.
    pub tipo: Tipo,
    /// El anfitrión de la página que lo pide (`None`: no se sabe; las reglas que dependen de
    /// él, como `$third-party` o `$domain=`, entonces no se aplican).
    pub pagina: Option<&'a str>,
}

/// El sitio de un anfitrión: los dos últimos trozos (`www.youtube.com` → `youtube.com`), o tres
/// si el penúltimo es un segundo nivel conocido de un país (`bbc.co.uk`, `gob.pe`). Es una
/// aproximación de la lista de sufijos públicos, que QUIMERA todavía no tiene.
pub fn sitio(anfitrion: &str) -> &str {
    let a = anfitrion.trim_end_matches('.');
    let trozos: Vec<&str> = a.rsplitn(4, '.').collect();
    let segundo = ["co", "com", "org", "net", "gob", "gov", "edu", "ac", "ne", "or", "go", "nic"];
    let n = match trozos.as_slice() {
        [pais, nivel, _, ..] if pais.len() == 2 && segundo.contains(nivel) => 3,
        _ => 2,
    };
    let quitar: usize = trozos.iter().take(n).map(|t| t.len() + 1).sum();
    &a[a.len().saturating_sub(quitar.saturating_sub(1))..]
}

/// Si `anfitrion` es `dominio` o uno de sus subdominios.
pub fn es_del_dominio(anfitrion: &str, dominio: &str) -> bool {
    anfitrion == dominio || anfitrion.strip_suffix(dominio).is_some_and(|r| r.ends_with('.'))
}

/// Las opciones de una regla (lo que va tras `$`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Opciones {
    /// `Some(true)`: solo a otro sitio; `Some(false)`: solo al mismo.
    tercero: Option<bool>,
    /// Los tipos a los que se aplica (0: todos) y los que no.
    tipos: u16,
    sin_tipos: u16,
    /// `domain=`: en qué páginas sí y en cuáles no.
    en: Vec<String>,
    no_en: Vec<String>,
    /// `$document` en una excepción: la página entera sin bloqueo.
    documento: bool,
}

impl Opciones {
    /// Lee `third-party,script,domain=a.com|~b.com`. `None` si alguna no se entiende.
    fn leer(texto: &str) -> Option<Opciones> {
        let mut o = Opciones::default();
        for op in texto.split(',').map(|s| s.trim().to_ascii_lowercase()) {
            let (no, nombre) = op.strip_prefix('~').map_or((false, op.as_str()), |r| (true, r));
            match nombre {
                "third-party" | "3p" => o.tercero = Some(!no),
                "first-party" | "1p" => o.tercero = Some(no),
                "document" | "doc" if !no => o.documento = true,
                "match-case" | "important" => {}
                n if n.starts_with("domain=") => {
                    for d in n["domain=".len()..].split('|').filter(|d| !d.is_empty()) {
                        match d.strip_prefix('~') {
                            Some(d) => o.no_en.push(String::from(d)),
                            None => o.en.push(String::from(d)),
                        }
                    }
                }
                n => {
                    let t = Tipo::de_nombre(n)?;
                    if no { o.sin_tipos |= t.bit() } else { o.tipos |= t.bit() }
                }
            }
        }
        Some(o)
    }

    fn aplica(&self, anfitrion: &str, c: &Contexto) -> bool {
        let bit = c.tipo.bit();
        if (self.tipos != 0 && self.tipos & bit == 0) || self.sin_tipos & bit != 0 {
            return false;
        }
        let necesita_pagina = self.tercero.is_some() || !self.en.is_empty() || !self.no_en.is_empty();
        let Some(pagina) = c.pagina else { return !necesita_pagina };
        let pagina = pagina.to_ascii_lowercase();
        if let Some(t) = self.tercero
            && (sitio(anfitrion) != sitio(&pagina)) != t
        {
            return false;
        }
        let en = |l: &[String]| l.iter().any(|d| es_del_dominio(&pagina, d));
        (self.en.is_empty() || en(&self.en)) && !en(&self.no_en)
    }
}

/// Una regla de red: dónde casa (un trozo de URL, `^` es un separador) y sus opciones.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Regla {
    /// `Some(dominio)` si va anclada a un anfitrión (`||dominio/ruta`).
    dominio: Option<String>,
    /// Lo que sigue (la ruta), o el texto entero si no va anclada.
    texto: String,
    opciones: Opciones,
}

impl Regla {
    /// Lee una regla sin `@@`. `None` si no se entiende.
    fn leer(regla: &str) -> Option<Regla> {
        let (patron, opciones) = match regla.rsplit_once('$') {
            Some((p, o)) => (p, Opciones::leer(o)?),
            None => (regla, Opciones::default()),
        };
        if patron.len() > 1 && patron.starts_with('/') && patron.ends_with('/') {
            return None; // una expresión regular
        }
        let r = match patron.strip_prefix("||") {
            Some(r) => {
                let (dominio, texto) = r.split_at(r.find(['^', '/']).unwrap_or(r.len()));
                Regla { dominio: Some(dominio.to_ascii_lowercase()), texto: String::from(texto), opciones }
            }
            None => Regla { dominio: None, texto: String::from(patron.trim_matches('|')), opciones },
        };
        (r.dominio.as_ref().is_some_and(|d| !d.is_empty()) || !r.texto.is_empty()).then_some(r)
    }

    fn casa(&self, anfitrion: &str, resto: &str) -> bool {
        match &self.dominio {
            Some(d) => es_del_dominio(anfitrion, d) && empieza_con(resto, &self.texto),
            None => {
                let entera = format!("{anfitrion}{resto}");
                (0..=entera.len()).any(|i| entera.is_char_boundary(i) && empieza_con(&entera[i..], &self.texto))
            }
        }
    }

    fn aplica(&self, anfitrion: &str, resto: &str, c: &Contexto) -> bool {
        self.opciones.aplica(anfitrion, c) && self.casa(anfitrion, resto)
    }
}

/// Si `texto` empieza con `patron`, donde `^` vale por un separador o por el final, y `*` por
/// cualquier trozo (también vacío).
fn empieza_con(texto: &str, patron: &str) -> bool {
    casa_bytes(texto.as_bytes(), patron.as_bytes())
}

fn casa_bytes(mut t: &[u8], mut p: &[u8]) -> bool {
    while let Some((&c, resto)) = p.split_first() {
        match (c, t.split_first()) {
            (b'*', _) => return (0..=t.len()).any(|i| casa_bytes(&t[i..], resto)),
            (b'^', None) => return resto.iter().all(|&x| x == b'*' || x == b'^'),
            (b'^', Some((&x, r))) if separador(x) => t = r,
            (_, Some((&x, r))) if x.eq_ignore_ascii_case(&c) => t = r,
            _ => return false,
        }
        p = resto;
    }
    true
}

fn separador(c: u8) -> bool {
    !(c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'%'))
}

/// El filtro: las reglas ya leídas, listas para preguntar por cada URL.
#[derive(Debug, Clone, Default)]
pub struct Filtro {
    /// Dominios bloqueados enteros y sin opciones (`||dominio^`): se busca cada sufijo.
    dominios: HashSet<String>,
    reglas: Vec<Regla>,
    excepciones: Vec<Regla>,
    /// Las de ocultar elementos (`##`).
    pub cosmeticas: Cosmeticas,
}

impl Filtro {
    /// El filtro con la lista incluida ([`LISTA`]).
    pub fn de_serie() -> Filtro {
        Filtro::leer(LISTA)
    }

    /// Lee una lista de reglas (una por línea).
    pub fn leer(lista: &str) -> Filtro {
        let mut f = Filtro::default();
        for linea in lista.lines().map(str::trim) {
            if linea.is_empty() || linea.starts_with('!') || linea.starts_with('[') || f.cosmeticas.leer(linea) {
                continue;
            }
            let (excepcion, texto) = linea.strip_prefix("@@").map_or((false, linea), |r| (true, r));
            let Some(regla) = Regla::leer(texto) else { continue };
            let entero = regla.texto.is_empty() || regla.texto == "^";
            if !excepcion
                && entero
                && regla.opciones == Opciones::default()
                && let Some(d) = &regla.dominio
            {
                f.dominios.insert(d.clone());
                continue;
            }
            if excepcion { f.excepciones.push(regla) } else { f.reglas.push(regla) }
        }
        f
    }

    /// Cuántas reglas tiene (de red y de ocultar).
    pub fn reglas(&self) -> usize {
        self.dominios.len() + self.reglas.len() + self.excepciones.len() + self.cosmeticas.total()
    }

    /// Si una excepción `$document` deja la página `anfitrion` entera sin bloqueo.
    pub fn pagina_permitida(&self, anfitrion: &str) -> bool {
        let a = anfitrion.to_ascii_lowercase();
        self.excepciones.iter().any(|r| r.opciones.documento && r.casa(&a, "/"))
    }

    /// Si la URL se bloquea, sin saber desde dónde se pide.
    pub fn bloquea(&self, url: &Url) -> bool {
        self.bloquea_en(url, &Contexto::default())
    }

    /// Si la URL se bloquea pedida en el contexto `c`. Solo se miran `http` y `https`.
    pub fn bloquea_en(&self, url: &Url, c: &Contexto) -> bool {
        if !matches!(url.esquema.as_str(), "http" | "https") || c.pagina.is_some_and(|p| self.pagina_permitida(p)) {
            return false;
        }
        let anfitrion = url.anfitrion.to_ascii_lowercase();
        let mut resto = url.ruta.clone();
        if let Some(q) = &url.consulta {
            resto.push('?');
            resto.push_str(q);
        }
        let por_dominio = anfitrion
            .char_indices()
            .filter(|(i, ch)| *i == 0 || *ch == '.')
            .any(|(i, _)| self.dominios.contains(anfitrion[i..].trim_start_matches('.')));
        let bloquea = por_dominio || self.reglas.iter().any(|r| r.aplica(&anfitrion, &resto, c));
        bloquea && !self.excepciones.iter().any(|r| !r.opciones.documento && r.aplica(&anfitrion, &resto, c))
    }
}

#[cfg(test)]
#[path = "bloqueo_pruebas.rs"]
mod pruebas;
