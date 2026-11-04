//! El bloqueador de anuncios y rastreo, como el escudo de Brave: una lista de reglas al estilo
//! EasyList, incluida en el binario (no se descarga nada), y un filtro que dice si una URL se pide.
//!
//! Reglas que se entienden (un subconjunto de la sintaxis de Adblock Plus):
//!
//! - `||dominio^` — el dominio y sus subdominios (`||doubleclick.net^` vale para
//!   `googleads.g.doubleclick.net`). Con ruta detrás (`||youtube.com/pagead/`), la URL tiene que
//!   empezar por ese anfitrión y esa ruta.
//! - `@@||dominio^` o `@@texto` — excepción: lo que coincide se pide aunque otra regla lo bloquee.
//! - cualquier otro texto (`/pagead/`) — se busca en la URL entera, sin el esquema.
//! - `^` es un separador: cualquier carácter que no sea letra, número, `_ - . %`, o el final.
//! - `!` empieza un comentario. Las opciones (`$third-party`...) y las reglas de ocultar
//!   elementos (`##`) todavía no se entienden: esas líneas se ignoran.
//!
//! Lo bloqueado no sale a la red: el `Cliente` lo responde como un fallo de red
//! ([`crate::Error::Bloqueado`]), igual que Brave, para que la página siga funcionando.

use crate::Url;
use std::collections::HashSet;

/// La lista incluida: anuncios y rastreo, elegida para no romper los sitios que los sirven.
pub const LISTA: &str = include_str!("bloqueo.txt");

/// Una regla de texto: un trozo que tiene que aparecer en la URL (`^` es un separador).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Patron {
    /// `Some(dominio)` si va anclado a un anfitrión (`||dominio/ruta`).
    dominio: Option<String>,
    /// Lo que sigue (la ruta), o el texto entero si no va anclado.
    texto: String,
}

impl Patron {
    fn coincide(&self, anfitrion: &str, resto: &str) -> bool {
        match &self.dominio {
            Some(d) => es_del_dominio(anfitrion, d) && empieza_con(resto, &self.texto),
            None => {
                let entera = format!("{anfitrion}{resto}");
                (0..=entera.len()).any(|i| entera.is_char_boundary(i) && empieza_con(&entera[i..], &self.texto))
            }
        }
    }
}

/// Si `anfitrion` es `dominio` o uno de sus subdominios.
fn es_del_dominio(anfitrion: &str, dominio: &str) -> bool {
    anfitrion == dominio || anfitrion.strip_suffix(dominio).is_some_and(|r| r.ends_with('.'))
}

/// Si `texto` empieza con `patron`, donde `^` vale por un separador o por el final.
fn empieza_con(texto: &str, patron: &str) -> bool {
    let (mut t, mut p) = (texto.as_bytes(), patron.as_bytes());
    while let Some((&c, resto)) = p.split_first() {
        match (c, t.split_first()) {
            (b'^', None) => return resto.is_empty(),
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
    /// Dominios bloqueados enteros (`||dominio^`): se busca cada sufijo del anfitrión.
    dominios: HashSet<String>,
    patrones: Vec<Patron>,
    excepciones: Vec<Patron>,
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
            if linea.is_empty() || linea.starts_with('!') || linea.starts_with('[') || linea.contains('#') {
                continue;
            }
            // Las opciones todavía no se entienden: mejor no bloquear de más.
            if linea.contains('$') {
                continue;
            }
            let (excepcion, regla) = match linea.strip_prefix("@@") {
                Some(r) => (true, r),
                None => (false, linea),
            };
            let patron = match regla.strip_prefix("||") {
                Some(r) => {
                    let fin = r.find(['^', '/']).unwrap_or(r.len());
                    let (dominio, texto) = r.split_at(fin);
                    if !excepcion && (texto == "^" || texto.is_empty()) {
                        f.dominios.insert(dominio.to_ascii_lowercase());
                        continue;
                    }
                    Patron { dominio: Some(dominio.to_ascii_lowercase()), texto: String::from(texto) }
                }
                None => Patron { dominio: None, texto: String::from(regla.trim_start_matches('|')) },
            };
            if patron.texto.is_empty() && patron.dominio.is_none() {
                continue;
            }
            if excepcion { f.excepciones.push(patron) } else { f.patrones.push(patron) }
        }
        f
    }

    /// Cuántas reglas tiene.
    pub fn reglas(&self) -> usize {
        self.dominios.len() + self.patrones.len() + self.excepciones.len()
    }

    /// Si la URL se bloquea. Solo se miran `http` y `https`.
    pub fn bloquea(&self, url: &Url) -> bool {
        if !matches!(url.esquema.as_str(), "http" | "https") {
            return false;
        }
        let anfitrion = url.anfitrion.to_ascii_lowercase();
        let mut resto = url.ruta.clone();
        if let Some(c) = &url.consulta {
            resto.push('?');
            resto.push_str(c);
        }
        let por_dominio = anfitrion
            .char_indices()
            .filter(|(i, c)| *i == 0 || *c == '.')
            .map(|(i, _)| anfitrion[i..].trim_start_matches('.'))
            .any(|sufijo| self.dominios.contains(sufijo));
        let bloquea = por_dominio || self.patrones.iter().any(|p| p.coincide(&anfitrion, &resto));
        bloquea && !self.excepciones.iter().any(|p| p.coincide(&anfitrion, &resto))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn url(u: &str) -> Url {
        Url::analizar(u).expect("url")
    }

    #[test]
    fn dominios_y_subdominios() {
        let f = Filtro::leer("||doubleclick.net^\n||google-analytics.com^\n! comentario");
        assert!(f.bloquea(&url("https://doubleclick.net/x")));
        assert!(f.bloquea(&url("https://googleads.g.doubleclick.net/pagead/id")));
        assert!(f.bloquea(&url("https://www.google-analytics.com/analytics.js")));
        assert!(!f.bloquea(&url("https://notdoubleclick.net/")), "no es un subdominio");
        assert!(!f.bloquea(&url("https://doubleclick.net.ejemplo.com/")));
        assert!(!f.bloquea(&url("https://ejemplo.com/?doubleclick.net")));
        assert_eq!(f.reglas(), 2);
    }

    #[test]
    fn rutas_y_separadores() {
        let f = Filtro::leer("/pagead/\n/api/stats/ads\n||youtube.com/ptracking\n/get_midroll_\n/ads.js^");
        assert!(f.bloquea(&url("https://www.youtube.com/pagead/viewthroughconversion/1")));
        assert!(f.bloquea(&url("https://www.youtube.com/api/stats/ads?ver=2")));
        assert!(f.bloquea(&url("https://www.youtube.com/ptracking?pltype=content")));
        assert!(f.bloquea(&url("https://m.youtube.com/ptracking")));
        assert!(!f.bloquea(&url("https://ejemplo.com/ptracking")), "anclada a youtube.com");
        assert!(f.bloquea(&url("https://ejemplo.com/js/ads.js")));
        assert!(f.bloquea(&url("https://ejemplo.com/js/ads.js?v=1")));
        assert!(!f.bloquea(&url("https://ejemplo.com/js/ads.json")), "^ no es una letra");
        assert!(!f.bloquea(&url("https://www.youtube.com/youtubei/v1/player")));
        assert!(!f.bloquea(&url("https://www.youtube.com/api/stats/qoe")));
    }

    #[test]
    fn excepciones_y_lineas_ignoradas() {
        let f = Filtro::leer("||anuncios.com^\n@@||anuncios.com/necesario\nejemplo.com##.banner\n/x$third-party");
        assert!(f.bloquea(&url("https://anuncios.com/banner.png")));
        assert!(!f.bloquea(&url("https://cdn.anuncios.com/necesario.js")));
        assert!(!f.bloquea(&url("https://ejemplo.com/x")), "con opciones se ignora");
        assert!(!f.bloquea(&url("data:text/plain,pagead")), "solo http(s)");
    }

    #[test]
    fn la_lista_de_serie() {
        let f = Filtro::de_serie();
        assert!(f.reglas() > 20);
        for u in [
            "https://googleads.g.doubleclick.net/pagead/id",
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://www.google-analytics.com/analytics.js",
            "https://www.googletagmanager.com/gtag/js?id=G-1",
            "https://www.youtube.com/pagead/adview",
            "https://www.youtube.com/api/stats/ads?x=1",
            "https://www.youtube.com/ptracking?x=1",
            "https://static.doubleclick.net/instream/ad_status.js",
        ] {
            assert!(f.bloquea(&url(u)), "{u}");
        }
        for u in [
            "https://www.youtube.com/watch?v=GWprpnIG-w4",
            "https://www.youtube.com/youtubei/v1/player?prettyPrint=false",
            "https://i.ytimg.com/vi/GWprpnIG-w4/hqdefault.jpg",
            "https://www.youtube.com/s/player/abc/base.js",
            "https://store.steampowered.com/",
            "https://www.google.com/search?q=pagead",
        ] {
            assert!(!f.bloquea(&url(u)), "{u}");
        }
    }
}
