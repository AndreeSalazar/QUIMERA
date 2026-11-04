//! Ocultar elementos: las reglas `##` de EasyList. No bloquean peticiones; esconden los huecos y
//! banners de anuncios que vienen en la propia página (en YouTube, `ytd-ad-slot-renderer`).
//!
//! - `##selector` — en todas las páginas.
//! - `youtube.com,m.youtube.com##selector` — solo en esos sitios (y sus subdominios);
//!   `~sitio` lo quita de ese sitio.
//! - `sitio#@#selector` — excepción: ese selector no se oculta en ese sitio.
//!
//! El resultado es una hoja de estilo (`selector{display:none!important}`, una regla por
//! selector, para que uno que no se entienda no anule a los demás) que la ventana añade a la
//! página, como hace Brave. Las variantes con guiones (`#?#`, `#$#`, `##+js(...)`) todavía se
//! ignoran.

use crate::bloqueo::es_del_dominio;

/// Una regla de ocultar.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Regla {
    en: Vec<String>,
    no_en: Vec<String>,
    selector: String,
}

impl Regla {
    fn aplica(&self, anfitrion: &str) -> bool {
        let en = |l: &[String]| l.iter().any(|d| es_del_dominio(anfitrion, d));
        (self.en.is_empty() || en(&self.en)) && !en(&self.no_en)
    }
}

/// Las reglas de ocultar de una lista.
#[derive(Debug, Clone, Default)]
pub struct Cosmeticas {
    reglas: Vec<Regla>,
    excepciones: Vec<Regla>,
}

impl Cosmeticas {
    /// Si la línea es de ocultar elementos, la guarda (o la descarta si es de una variante que
    /// no se entiende) y devuelve `true`; si es de red, `false`.
    pub fn leer(&mut self, linea: &str) -> bool {
        for raro in ["#?#", "#$#", "#%#", "#@?#", "#@$#"] {
            if linea.contains(raro) {
                return true;
            }
        }
        let (excepcion, (sitios, selector)) = match (linea.split_once("#@#"), linea.split_once("##")) {
            (Some(p), _) => (true, p),
            (None, Some(p)) => (false, p),
            (None, None) => return false,
        };
        let selector = selector.trim();
        if selector.is_empty() || selector.starts_with('+') || selector.contains(['{', '}']) {
            return true;
        }
        let mut r = Regla { en: Vec::new(), no_en: Vec::new(), selector: String::from(selector) };
        for s in sitios.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()) {
            match s.strip_prefix('~') {
                Some(s) => r.no_en.push(String::from(s)),
                None => r.en.push(s),
            }
        }
        if excepcion {
            self.excepciones.push(r)
        } else {
            self.reglas.push(r)
        }
        true
    }

    /// Cuántas reglas hay.
    pub fn total(&self) -> usize {
        self.reglas.len() + self.excepciones.len()
    }

    /// Los selectores que se ocultan en `anfitrion`.
    pub fn selectores(&self, anfitrion: &str) -> Vec<&str> {
        let a = anfitrion.to_ascii_lowercase();
        let quitado = |s: &str| self.excepciones.iter().any(|e| e.selector == s && e.aplica(&a));
        let mut v: Vec<&str> = Vec::new();
        for r in self.reglas.iter().filter(|r| r.aplica(&a) && !quitado(&r.selector)) {
            if !v.contains(&r.selector.as_str()) {
                v.push(&r.selector);
            }
        }
        v
    }

    /// La hoja de estilo que oculta lo de `anfitrion` (vacía si no hay nada).
    pub fn hoja(&self, anfitrion: &str) -> String {
        self.selectores(anfitrion).iter().map(|s| format!("{s}{{display:none!important}}\n")).collect()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn por_sitio_y_excepciones() {
        let mut c = Cosmeticas::default();
        for l in [
            "##.adsbygoogle",
            "youtube.com##ytd-ad-slot-renderer",
            "youtube.com,~music.youtube.com###masthead-ad",
            "ejemplo.com#@#.adsbygoogle",
            "ejemplo.com#?#div:-abp-has(.ad)",
            "##+js(json-prune, adPlacements)",
            "||doubleclick.net^",
        ] {
            let es = c.leer(l);
            assert_eq!(es, !l.starts_with("||"), "{l}");
        }
        assert_eq!(c.total(), 4);
        assert_eq!(c.selectores("www.youtube.com"), [".adsbygoogle", "ytd-ad-slot-renderer", "#masthead-ad"]);
        assert_eq!(c.selectores("music.youtube.com"), [".adsbygoogle", "ytd-ad-slot-renderer"]);
        assert!(c.selectores("ejemplo.com").is_empty(), "la excepción la quita");
        assert_eq!(c.hoja("otro.com"), ".adsbygoogle{display:none!important}\n");
    }
}
