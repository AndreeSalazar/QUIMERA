//! Ocultar elementos: las reglas `##` de EasyList, y los *scriptlets* `##+js(...)`.
//!
//! - `sitio1,sitio2##selector` — solo en esos sitios (y sus subdominios); `~sitio` lo quita.
//! - `##selector` — en todas las páginas. EasyList tiene unas 13.600; meterlas todas en cada
//!   página haría lenta la cascada. Como Brave, solo se usan las que dependen de una clase o
//!   un id (`##.anuncio`, `###banner-ad`, `##div.ad-box`), y solo si la página tiene esa clase o
//!   ese id. Las demás genéricas (`##a[href*="ad."]`) solo valen si son de la lista propia.
//! - `sitio#@#selector` — excepción: ese selector no se oculta en ese sitio.
//! - `sitio##+js(nombre, argumentos)` — un *scriptlet*: un poco de JavaScript que corre antes
//!   que los scripts de la página (ver [`crate::scriptlets`]). `#@#+js(...)` lo quita.
//!
//! El resultado es una hoja de estilo (`selector{display:none!important}`, una regla por
//! selector, para que uno que no se entienda no anule a los demás) que la ventana añade a la
//! página, como hace Brave. Las variantes `#?#`, `#$#` y `#%#` (de Adblock Plus) se ignoran.

use crate::bloqueo::es_del_dominio;
use crate::regla::{en_dominios, picadillo};
use std::collections::HashMap;

/// Una regla: para qué sitios (tal cual, `a.com,~b.com`) y qué.
type DeSitio = (&'static str, &'static str);

/// Las reglas de ocultar y los *scriptlets* de las listas.
#[derive(Debug, Clone, Default)]
pub struct Cosmeticas {
    de_sitio: Vec<DeSitio>,
    excepciones: Vec<DeSitio>,
    /// Genéricas por su clase o id (`.anuncio`, `#banner`) → selectores.
    por_clave: HashMap<u64, Vec<&'static str>>,
    /// Genéricas sin clave (solo de la lista propia): van en todas las páginas.
    siempre: Vec<&'static str>,
    scriptlets: Vec<DeSitio>,
    sin_scriptlets: Vec<DeSitio>,
}

/// La clase o el id del que depende un selector genérico (`div.ad > p` → `.ad`), si empieza
/// por uno (con su etiqueta delante o no).
pub fn clave(selector: &str) -> Option<&str> {
    let s = selector.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
    s.chars().next().filter(|c| matches!(c, '.' | '#'))?;
    let nombre = &s[1..];
    let fin = nombre.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).unwrap_or(nombre.len());
    (fin > 0).then(|| &s[..=fin])
}

impl Cosmeticas {
    /// Si la línea es de ocultar elementos o un *scriptlet*, la guarda (o la descarta si es de
    /// una variante que no se entiende) y devuelve `true`; si es de red, `false`. `propia`: es
    /// de la lista de QUIMERA.
    pub fn leer(&mut self, linea: &'static str, propia: bool) -> bool {
        if ["#?#", "#$#", "#%#", "#@?#", "#@$#", "#@%#"].iter().any(|m| linea.contains(m)) {
            return true;
        }
        let (excepcion, (sitios, selector)) = match (linea.split_once("#@#"), linea.split_once("##")) {
            (Some(p), _) => (true, p),
            (None, Some(p)) => (false, p),
            (None, None) => return false,
        };
        let (sitios, selector) = (sitios.trim(), selector.trim());
        if let Some(args) = selector.strip_prefix("+js(").and_then(|r| r.strip_suffix(')')) {
            if !sitios.is_empty() {
                let lista = if excepcion { &mut self.sin_scriptlets } else { &mut self.scriptlets };
                lista.push((sitios, args));
            }
            return true;
        }
        if selector.is_empty() || selector.starts_with('+') || selector.contains(['{', '}']) || sitios.contains('*') {
            return true;
        }
        match (excepcion, sitios.is_empty(), clave(selector)) {
            (true, _, _) => self.excepciones.push((sitios, selector)),
            (false, false, _) => self.de_sitio.push((sitios, selector)),
            (false, true, Some(k)) => self.por_clave.entry(picadillo(k)).or_default().push(selector),
            (false, true, None) if propia => self.siempre.push(selector),
            (false, true, None) => {}
        }
        true
    }

    /// Cuántas reglas hay.
    pub fn total(&self) -> usize {
        self.de_sitio.len()
            + self.excepciones.len()
            + self.por_clave.values().map(Vec::len).sum::<usize>()
            + self.siempre.len()
            + self.scriptlets.len()
            + self.sin_scriptlets.len()
    }

    /// Los selectores que se ocultan en `anfitrion`. `tiene(".x")` / `tiene("#y")` dice si la
    /// página tiene esa clase o ese id (para las genéricas).
    pub fn selectores(&self, anfitrion: &str, tiene: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
        let a = anfitrion.to_ascii_lowercase();
        let excepciones: Vec<&str> =
            self.excepciones.iter().filter(|(s, _)| s.is_empty() || en_sitios(s, &a)).map(|(_, x)| *x).collect();
        let genericas = self.por_clave.values().flatten().filter(|s| clave(s).is_some_and(tiene));
        let de_sitio = self.de_sitio.iter().filter(|(s, _)| en_sitios(s, &a)).map(|(_, x)| x);
        let mut v: Vec<&'static str> = Vec::new();
        for s in self.siempre.iter().chain(genericas).chain(de_sitio) {
            if !excepciones.contains(s) && !v.contains(s) {
                v.push(s);
            }
        }
        v
    }

    /// La hoja de estilo que oculta lo de `anfitrion` (vacía si no hay nada).
    pub fn hoja(&self, anfitrion: &str, tiene: &dyn Fn(&str) -> bool) -> String {
        self.selectores(anfitrion, tiene).iter().map(|s| format!("{s}{{display:none!important}}\n")).collect()
    }

    /// Los *scriptlets* de `anfitrion`, como `nombre, arg1, arg2` (lo de dentro de `+js(...)`).
    pub fn scriptlets(&self, anfitrion: &str) -> Vec<&'static str> {
        let a = anfitrion.to_ascii_lowercase();
        let quitados: Vec<&str> =
            self.sin_scriptlets.iter().filter(|(s, _)| en_sitios(s, &a)).map(|(_, x)| *x).collect();
        let mut v: Vec<&'static str> = Vec::new();
        for (_, x) in self.scriptlets.iter().filter(|(s, _)| en_sitios(s, &a)) {
            if !quitados.contains(x) && !quitados.contains(&"") && !v.contains(x) {
                v.push(x);
            }
        }
        v
    }
}

/// Si `anfitrion` está en la lista `a.com,~b.com` de una regla de sitio (que tiene alguno).
fn en_sitios(sitios: &str, anfitrion: &str) -> bool {
    let positivo = sitios.split(',').map(str::trim).any(|d| !d.starts_with('~') && es_del_dominio(anfitrion, d));
    let solo_negativos = sitios.split(',').all(|d| d.trim().starts_with('~'));
    (positivo || solo_negativos) && en_dominios(sitios, anfitrion)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn leer(lineas: &[&'static str]) -> Cosmeticas {
        let mut c = Cosmeticas::default();
        for l in lineas {
            assert_eq!(c.leer(l, true), !l.starts_with("||"), "{l}");
        }
        c
    }

    #[test]
    fn por_sitio_y_excepciones() {
        let c = leer(&[
            "##.adsbygoogle",
            "youtube.com##ytd-ad-slot-renderer",
            "youtube.com,~music.youtube.com###masthead-ad",
            "ejemplo.com#@#.adsbygoogle",
            "ejemplo.com#?#div:-abp-has(.ad)",
            "||doubleclick.net^",
        ]);
        let todo = |_: &str| true;
        assert_eq!(c.selectores("www.youtube.com", &todo), [".adsbygoogle", "ytd-ad-slot-renderer", "#masthead-ad"]);
        assert_eq!(c.selectores("music.youtube.com", &todo), [".adsbygoogle", "ytd-ad-slot-renderer"]);
        assert!(c.selectores("ejemplo.com", &todo).is_empty(), "la excepción la quita");
        assert_eq!(c.hoja("otro.com", &todo), ".adsbygoogle{display:none!important}\n");
        assert!(c.selectores("otro.com", &|_| false).is_empty(), "la página no tiene esa clase");
    }

    #[test]
    fn genericas_por_clave() {
        let mut c = Cosmeticas::default();
        for l in ["##.anuncio", "###banner-ad", "##div.ad-box > p", "##a[href*=\"ad.\"]"] {
            c.leer(l, false);
        }
        assert_eq!(clave("div.ad-box > p"), Some(".ad-box"));
        assert_eq!(clave("#banner-ad"), Some("#banner-ad"));
        assert_eq!(clave("[id^=ad]"), None);
        let tiene = |k: &str| k == ".anuncio" || k == "#banner-ad";
        let mut v = c.selectores("ejemplo.com", &tiene);
        v.sort_unstable();
        assert_eq!(v, ["#banner-ad", ".anuncio"], "la de atributos no es de la lista propia");
    }

    #[test]
    fn scriptlets() {
        let c = leer(&[
            "youtube.com##+js(set, ytInitialPlayerResponse.adPlacements, undefined)",
            "youtube.com##+js(json-prune, playerAds)",
            "music.youtube.com#@#+js(json-prune, playerAds)",
            "##+js(set, generico, 1)",
        ]);
        assert_eq!(
            c.scriptlets("www.youtube.com"),
            ["set, ytInitialPlayerResponse.adPlacements, undefined", "json-prune, playerAds"]
        );
        assert_eq!(c.scriptlets("music.youtube.com"), ["set, ytInitialPlayerResponse.adPlacements, undefined"]);
        assert!(c.scriptlets("otro.com").is_empty(), "sin sitio no vale");
    }
}
