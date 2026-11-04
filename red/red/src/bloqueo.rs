//! El bloqueador de anuncios y rastreo, como el escudo de Brave: listas de reglas de EasyList,
//! incluidas en el binario (no se descarga nada), y un filtro que dice si una URL se pide.
//!
//! Reglas de red que se entienden (la sintaxis de Adblock Plus, casi entera):
//!
//! - `||dominio^` — el dominio y sus subdominios (`||doubleclick.net^` vale para
//!   `googleads.g.doubleclick.net`). Con ruta detrás (`||youtube.com/pagead/`), la URL tiene que
//!   empezar por ese anfitrión y esa ruta.
//! - cualquier otro texto (`/pagead/*`) — se busca en la URL entera.
//! - `^` es un separador: cualquier carácter que no sea letra, número, `_ - . %`, o el final;
//!   `*` es cualquier trozo; `|` al principio o al final ancla ahí la URL.
//! - `/…/` es una expresión regular de JavaScript (con `quimera-regex`) sobre la URL entera.
//! - `@@regla` — excepción: lo que coincide se pide aunque otra regla lo bloquee. Con
//!   `$document`, la página entera queda sin bloqueo.
//! - **Opciones** tras `$`, separadas por comas: `third-party` (`3p`) y `~third-party` (`1p`),
//!   los tipos (`script`, `image`, `stylesheet`, `xmlhttprequest`, `font`, `media`,
//!   `subdocument`, `other`, y su contrario con `~`), `domain=a.com|~b.com` y `match-case`.
//!   Una regla con una opción que no se entiende (`$popup`, `$csp=`, `$redirect=`) se ignora
//!   entera: mejor no bloquear de más.
//! - `!` empieza un comentario. Las reglas de ocultar elementos (`##`) y los *scriptlets*
//!   (`##+js(...)`) los lee [`crate::cosmetico`].
//!
//! Las listas: la propia de QUIMERA ([`LISTA`]) y EasyList y EasyPrivacy enteras ([`EASYLIST`],
//! [`EASYPRIVACY`]), tal como se publican. Para no gastar RAM, las reglas apuntan a ese texto en
//! vez de copiarlo, y se buscan con un **índice por trozos**, como el motor de Brave: cada regla
//! se guarda bajo su trozo más largo (`doubleclick`, `pagead`...) y una URL solo se compara con
//! las reglas de los trozos que tiene.
//!
//! Lo bloqueado no sale a la red: el `Cliente` lo responde como un fallo de red
//! ([`crate::Error::Bloqueado`]), igual que Brave, para que la página siga funcionando.

use crate::Url;
use crate::cosmetico::Cosmeticas;
use crate::regla::{Ancla, Pedido, Regla, picadillo};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// La lista propia de QUIMERA: lo que EasyList no cubre o cubre tarde (YouTube, sobre todo).
pub const LISTA: &str = include_str!("bloqueo.txt");
/// EasyList (anuncios), de <https://easylist.to/>. Licencia GPLv3 o CC BY-SA 3.0, a elegir
/// (ver `red/red/listas/LEEME.md`).
pub const EASYLIST: &str = include_str!("../listas/easylist.txt");
/// EasyPrivacy (rastreo), de <https://easylist.to/>, con la misma licencia.
pub const EASYPRIVACY: &str = include_str!("../listas/easyprivacy.txt");

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
    pub(crate) fn bit(self) -> u16 {
        1 << self as u16
    }

    pub(crate) fn de_nombre(n: &str) -> Option<Tipo> {
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

/// Reglas con su índice por trozos.
#[derive(Debug, Clone, Default)]
struct Grupo {
    reglas: Vec<Regla>,
    /// Trozo → reglas que lo tienen.
    indice: HashMap<u64, Vec<u32>>,
    /// Las que no tienen un trozo seguro (se miran con todas las URLs).
    sueltas: Vec<u32>,
}

impl Grupo {
    fn meter(&mut self, r: Regla) {
        let n = self.reglas.len() as u32;
        match r.trozo() {
            Some(t) => self.indice.entry(t).or_default().push(n),
            None => self.sueltas.push(n),
        }
        self.reglas.push(r);
    }

    fn alguna(&self, p: &Pedido, trozos: &[u64], c: &Contexto) -> bool {
        let por_trozo = trozos.iter().filter_map(|t| self.indice.get(t)).flatten();
        self.sueltas.iter().chain(por_trozo).any(|&i| self.reglas[i as usize].aplica(p, c))
    }
}

/// El filtro: las reglas ya leídas, listas para preguntar por cada URL.
#[derive(Debug, Clone, Default)]
pub struct Filtro {
    /// Dominios bloqueados enteros y sin opciones (`||dominio^`), ordenados: se busca cada
    /// sufijo del anfitrión.
    dominios: Vec<&'static str>,
    bloqueos: Grupo,
    excepciones: Grupo,
    /// `@@…$document`: páginas enteras sin bloqueo.
    documentos: Vec<Regla>,
    /// Las de ocultar elementos (`##`) y los *scriptlets*.
    pub cosmeticas: Cosmeticas,
}

impl Filtro {
    /// El filtro con las listas incluidas (la propia, EasyList y EasyPrivacy). Se lee una vez
    /// por proceso y se comparte.
    pub fn de_serie() -> Arc<Filtro> {
        static FILTRO: OnceLock<Arc<Filtro>> = OnceLock::new();
        FILTRO.get_or_init(|| Arc::new(Filtro::leer_todas(&[LISTA, EASYLIST, EASYPRIVACY]))).clone()
    }

    /// Lee una lista de reglas (una por línea).
    pub fn leer(lista: &'static str) -> Filtro {
        Filtro::leer_todas(&[lista])
    }

    /// Lee varias listas.
    pub fn leer_todas(listas: &[&'static str]) -> Filtro {
        let mut f = Filtro::default();
        let lineas = listas.iter().enumerate().flat_map(|(i, l)| l.lines().map(move |x| (i == 0, x.trim())));
        for (propia, linea) in lineas {
            if linea.is_empty() || linea.starts_with('!') || linea.starts_with('[') || f.cosmeticas.leer(linea, propia)
            {
                continue;
            }
            let (excepcion, texto) = linea.strip_prefix("@@").map_or((false, linea), |r| (true, r));
            if let Some(regla) = Regla::leer(texto) {
                f.meter(excepcion, regla);
            }
        }
        f.dominios.sort_unstable();
        f.dominios.dedup();
        f
    }

    fn meter(&mut self, excepcion: bool, regla: Regla) {
        let solo_dominio = regla.regex.is_none()
            && matches!(regla.texto, "" | "^")
            && !regla.fin
            && regla.opciones == Default::default();
        match regla.ancla {
            Ancla::Dominio(d) if !excepcion && solo_dominio && !d.bytes().any(|c| c.is_ascii_uppercase()) => {
                self.dominios.push(d);
            }
            _ if excepcion && regla.opciones.documento => self.documentos.push(regla),
            _ if excepcion => self.excepciones.meter(regla),
            _ => self.bloqueos.meter(regla),
        }
    }

    /// Cuántas reglas tiene (de red y de ocultar).
    pub fn reglas(&self) -> usize {
        self.dominios.len()
            + self.bloqueos.reglas.len()
            + self.excepciones.reglas.len()
            + self.documentos.len()
            + self.cosmeticas.total()
    }

    /// Si una excepción `$document` deja la página `anfitrion` entera sin bloqueo.
    pub fn pagina_permitida(&self, anfitrion: &str) -> bool {
        if self.documentos.is_empty() {
            return false;
        }
        let a = anfitrion.to_ascii_lowercase();
        let entera = format!("https://{a}/");
        let p = Pedido { original: &entera, entera: &entera, anfitrion: &a, resto: "/" };
        let c = Contexto { tipo: Tipo::Otro, pagina: Some(&a) };
        self.documentos.iter().any(|r| r.aplica(&p, &c))
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
        let mut sin_fragmento = url.clone();
        sin_fragmento.fragmento = None;
        let original = sin_fragmento.to_string();
        let entera = original.to_ascii_lowercase();
        let anfitrion = url.anfitrion.to_ascii_lowercase();
        let mut resto = url.ruta.to_ascii_lowercase();
        if let Some(q) = &url.consulta {
            resto.push('?');
            resto.push_str(&q.to_ascii_lowercase());
        }
        let p = Pedido { original: &original, entera: &entera, anfitrion: &anfitrion, resto: &resto };
        let mut trozos: Vec<u64> =
            entera.split(|ch: char| !ch.is_ascii_alphanumeric()).filter(|t| t.len() >= 2).map(picadillo).collect();
        trozos.sort_unstable();
        trozos.dedup();
        let por_dominio = anfitrion
            .char_indices()
            .filter(|(i, ch)| *i == 0 || *ch == '.')
            .any(|(i, _)| self.dominios.binary_search(&anfitrion[i..].trim_start_matches('.')).is_ok());
        let bloquea = por_dominio || self.bloqueos.alguna(&p, &trozos, c);
        bloquea && !self.excepciones.alguna(&p, &trozos, c)
    }
}

#[cfg(test)]
#[path = "bloqueo_pruebas.rs"]
mod pruebas;
