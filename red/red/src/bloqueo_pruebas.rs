//! Pruebas del filtro de red.

#![cfg(test)]

use super::*;

fn url(u: &str) -> Url {
    Url::analizar(u).expect("url")
}

fn desde<'a>(tipo: Tipo, pagina: &'a str) -> Contexto<'a> {
    Contexto { tipo, pagina: Some(pagina) }
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
    let f = Filtro::leer("/pagead/*\n/api/stats/ads\n||youtube.com/ptracking\n/get_midroll_\n/ads.js^");
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
    let comodin = Filtro::leer(
        "||ejemplo.com/*/anuncio^
/ad*.gif",
    );
    assert!(comodin.bloquea(&url("https://ejemplo.com/a/b/anuncio?x")));
    assert!(!comodin.bloquea(&url("https://ejemplo.com/anuncios")));
    assert!(comodin.bloquea(&url("https://otro.com/img/ad_300.gif")));
}

#[test]
fn excepciones_y_lineas_ignoradas() {
    let f = Filtro::leer("||anuncios.com^\n@@||anuncios.com/necesario\n/x$popup\n/^ad[0-9]+/\n/y$csp=script-src");
    assert!(f.bloquea(&url("https://anuncios.com/banner.png")));
    assert!(!f.bloquea(&url("https://cdn.anuncios.com/necesario.js")));
    assert!(!f.bloquea(&url("https://ejemplo.com/x")), "una opción que no se entiende");
    assert!(!f.bloquea(&url("https://ejemplo.com/y")));
    assert!(!f.bloquea(&url("https://ejemplo.com/ad1")), "las expresiones regulares se ignoran");
    assert!(!f.bloquea(&url("data:text/plain,pagead")), "solo http(s)");
}

#[test]
fn tercero_y_tipos() {
    let f = Filtro::leer("||facebook.net^$third-party\n/rastreo.js$script\n/pixel.$image,~third-party\n/api/*$~xhr");
    let fb = url("https://connect.facebook.net/es_ES/fbevents.js");
    assert!(f.bloquea_en(&fb, &desde(Tipo::Script, "www.ejemplo.com")));
    assert!(!f.bloquea_en(&fb, &desde(Tipo::Script, "www.facebook.net")), "en su propio sitio");
    assert!(!f.bloquea(&fb), "sin saber la página no se aplica");
    let r = url("https://ejemplo.com/rastreo.js");
    assert!(f.bloquea_en(&r, &desde(Tipo::Script, "ejemplo.com")));
    assert!(!f.bloquea_en(&r, &desde(Tipo::Xhr, "ejemplo.com")));
    let p = url("https://img.ejemplo.com/pixel.gif");
    assert!(f.bloquea_en(&p, &desde(Tipo::Imagen, "www.ejemplo.com")), "mismo sitio");
    assert!(!f.bloquea_en(&p, &desde(Tipo::Imagen, "otro.com")));
    let api = url("https://ejemplo.com/api/x");
    assert!(f.bloquea_en(&api, &desde(Tipo::Script, "ejemplo.com")));
    assert!(!f.bloquea_en(&api, &desde(Tipo::Xhr, "ejemplo.com")));
}

#[test]
fn dominio_y_documento() {
    let f = Filtro::leer(
        "/banner/*$domain=ejemplo.com|~foro.ejemplo.com\n||rastreo.com^\n@@||rastreo.com^$domain=amigo.com\n\
         @@||confiado.org^$document",
    );
    let b = url("https://cdn.com/banner/1.png");
    assert!(f.bloquea_en(&b, &desde(Tipo::Imagen, "www.ejemplo.com")));
    assert!(!f.bloquea_en(&b, &desde(Tipo::Imagen, "foro.ejemplo.com")));
    assert!(!f.bloquea_en(&b, &desde(Tipo::Imagen, "otro.com")));
    let r = url("https://rastreo.com/t.js");
    assert!(f.bloquea_en(&r, &desde(Tipo::Script, "otro.com")));
    assert!(!f.bloquea_en(&r, &desde(Tipo::Script, "amigo.com")), "excepción en ese sitio");
    assert!(f.pagina_permitida("www.confiado.org"));
    assert!(!f.bloquea_en(&r, &desde(Tipo::Script, "www.confiado.org")), "página entera permitida");
}

#[test]
fn sitios() {
    assert_eq!(sitio("www.youtube.com"), "youtube.com");
    assert_eq!(sitio("youtube.com"), "youtube.com");
    assert_eq!(sitio("a.b.bbc.co.uk"), "bbc.co.uk");
    assert_eq!(sitio("www.sunat.gob.pe"), "sunat.gob.pe");
    assert_eq!(sitio("localhost"), "localhost");
    assert_eq!(sitio("store.steampowered.com"), "steampowered.com");
}

#[test]
fn la_lista_de_serie() {
    let f = Filtro::de_serie();
    assert!(f.reglas() > 60);
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
    let tercero = desde(Tipo::Script, "www.ejemplo.com");
    assert!(f.bloquea_en(&url("https://connect.facebook.net/en_US/fbevents.js"), &tercero));
    assert!(!f.bloquea_en(&url("https://static.xx.fbcdn.net/rsrc.php/x.js"), &desde(Tipo::Script, "www.facebook.com")));
    for u in [
        "https://www.youtube.com/watch?v=GWprpnIG-w4",
        "https://www.youtube.com/youtubei/v1/player?prettyPrint=false",
        "https://i.ytimg.com/vi/GWprpnIG-w4/hqdefault.jpg",
        "https://www.youtube.com/s/player/abc/base.js",
        "https://store.steampowered.com/",
        "https://www.google.com/search?q=pagead",
    ] {
        assert!(!f.bloquea_en(&url(u), &desde(Tipo::Xhr, "www.youtube.com")), "{u}");
    }
    assert!(f.cosmeticas.selectores("www.youtube.com").contains(&"ytd-ad-slot-renderer"));
}
