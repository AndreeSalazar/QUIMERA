//! El escudo en la página: lo que el bloqueador cambia del HTML antes de leerlo, si el bloqueo
//! está activo en el sitio (Ctrl+B lo apaga):
//!
//! 1. En YouTube, quita los anuncios de vídeo del texto (`quimera_red::anuncios`).
//! 2. Mete los *scriptlets* del sitio en un `<script>` justo tras `<head>`, para que corran
//!    antes que cualquier script de la página.
//! 3. Añade al final la hoja que oculta los huecos de anuncios (`##`). Para las reglas
//!    genéricas, mira qué clases e ids trae el HTML.

use quimera_red::{Cliente, Url};
use std::collections::HashSet;

/// El HTML con el escudo puesto (o tal cual, sin bloqueo en ese sitio).
pub fn aplicar(cliente: &Cliente, url: &Url, html: String) -> String {
    let Some(filtro) = cliente.filtro.as_ref().filter(|_| cliente.bloqueo_activo(url)) else { return html };
    let mut html = crate::peticiones::limpiar_youtube(url, html);
    let cosmeticas = &filtro.cosmeticas;
    if let Some(js) = quimera_red::scriptlets::guion(&cosmeticas.scriptlets(&url.anfitrion)) {
        let en = tras_head(&html);
        html.insert_str(en, &format!("<script id=\"quimera-scriptlets\">{js}</script>"));
    }
    let claves = clases_e_ids(&html);
    let hoja = cosmeticas.hoja(&url.anfitrion, &|k| claves.contains(k));
    if !hoja.is_empty() {
        html.push_str("<style id=\"quimera-bloqueo\">\n");
        html.push_str(&hoja.replace('<', r"\3c "));
        html.push_str("</style>");
    }
    html
}

/// Dónde meter un script que tiene que ir antes que los de la página: tras `<head ...>`; si no
/// hay, tras `<!doctype ...>` (antes no, o la página pasaría a modo *quirks*); si no, al principio.
fn tras_head(html: &str) -> usize {
    let min = html.get(..html.len().min(1 << 16)).map_or_else(|| html.to_ascii_lowercase(), str::to_ascii_lowercase);
    let tras = |etiqueta: &str| {
        let i = min.find(etiqueta)?;
        let siguiente = min.as_bytes().get(i + etiqueta.len())?;
        let es_etiqueta = matches!(siguiente, b'>' | b' ' | b'\t' | b'\n' | b'\r');
        es_etiqueta.then(|| min[i..].find('>').map(|f| i + f + 1)).flatten()
    };
    tras("<head").or_else(|| tras("<!doctype")).filter(|&i| html.is_char_boundary(i)).unwrap_or(0)
}

/// Las clases (`.x`) y los ids (`#y`) que aparecen en los atributos `class` e `id` del HTML.
/// Es una lectura rápida del texto, sin construir el árbol.
fn clases_e_ids(html: &str) -> HashSet<String> {
    let mut v = HashSet::new();
    let b = html.as_bytes();
    for (nombre, marca) in [("class=", '.'), ("id=", '#')] {
        let mut desde = 0;
        while let Some(i) = html[desde..].find(nombre).map(|i| i + desde) {
            desde = i + nombre.len();
            if i > 0 && !matches!(b[i - 1], b' ' | b'\t' | b'\n' | b'\r') {
                continue; // `data-id=`, `xclass=`...
            }
            let Some(&q) = b.get(desde).filter(|q| matches!(q, b'"' | b'\'')) else { continue };
            let Some(fin) = html[desde + 1..].find(q as char) else { break };
            for palabra in html[desde + 1..desde + 1 + fin].split_ascii_whitespace() {
                v.insert(format!("{marca}{palabra}"));
            }
            desde += 1 + fin;
        }
    }
    v
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn donde_van_los_scriptlets() {
        let h = "<!DOCTYPE html><html><HEAD lang=es><title>x</title>";
        assert_eq!(&h[tras_head(h)..], "<title>x</title>");
        let h = "<!doctype html><p>sin head";
        assert_eq!(&h[tras_head(h)..], "<p>sin head");
        assert_eq!(tras_head("<p>nada"), 0);
        assert_eq!(tras_head("<header>no es head</header>"), 0);
    }

    #[test]
    fn clases_e_ids_del_html() {
        let c = clases_e_ids(r#"<div class="a  ad-box" id='x'><p data-id="no" class=sin-comillas>"#);
        let mut v: Vec<&str> = c.iter().map(String::as_str).collect();
        v.sort_unstable();
        assert_eq!(v, ["#x", ".a", ".ad-box"]);
    }

    #[test]
    fn el_escudo_en_youtube() {
        let c = Cliente::nuevo();
        let yt = Url::analizar("https://www.youtube.com/watch?v=x").expect("url");
        let html =
            String::from(r#"<html><head><script>var ytInitialPlayerResponse = {"playerAds":[1],"a":2};</script>"#);
        let r = aplicar(&c, &yt, html.clone());
        assert!(r.contains(r#"{"a":2}"#) && !r.contains("playerAds\":[1]"));
        let s = r.find("quimera-scriptlets").expect("scriptlets");
        assert!(s < r.find("ytInitialPlayerResponse =").expect("script de la página"), "antes que los de la página");
        assert!(r.contains("ytd-ad-slot-renderer{display:none!important}"));
        c.alternar_bloqueo(&yt);
        assert_eq!(aplicar(&c, &yt, html.clone()), html, "con el bloqueo apagado, nada");
    }
}
