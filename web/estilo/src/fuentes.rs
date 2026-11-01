//! Las fuentes de la página (`@font-face`): qué familias define y de dónde se descargan.
//!
//! - De cada `src` se elige la primera que QUIMERA sabe leer: WOFF, TrueType u OpenType (WOFF2
//!   todavía no). `local()` se salta.
//! - Con `unicode-range`, las webs parten cada familia por alfabetos. Solo se piden los trozos
//!   que cubren el latino básico: los demás serían red y memoria para letras que casi nunca se
//!   usan (las de reserva del sistema siguen ahí para ellas).
//! - El estilo no lee red ni archivos: devuelve las direcciones tal como vienen y la hoja de la
//!   que salen (las relativas se resuelven contra ella), y quien llama las descarga.

use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::{Declaracion, Token, piezas};
use quimera_html::{Documento, NodoId};

/// Una fuente que pide la página.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuenteWeb {
    /// Su nombre de familia (`Fira Sans`).
    pub familia: String,
    /// `font-weight` de 600 o más.
    pub negrita: bool,
    /// `font-style: italic | oblique`.
    pub cursiva: bool,
    /// De dónde se descarga, tal como viene en `src`.
    pub url: String,
    /// El `href` de la hoja `<link>` que la define (contra ella se resuelve `url`); `None` si
    /// viene de un `<style>` de la página.
    pub hoja: Option<String>,
}

/// Las fuentes que piden las hojas de la página. `externa` da el CSS de cada `<link>` por su `href`.
pub fn fuentes_web(doc: &Documento, externa: &dyn Fn(&str) -> Option<String>) -> Vec<FuenteWeb> {
    let mut v = Vec::new();
    for id in doc.descendientes(Documento::RAIZ) {
        let Some((css, hoja)) = hoja_de(doc, id, externa) else { continue };
        for cara in quimera_css::analizar(&css).caras {
            if let Some(f) = leer_cara(&cara, hoja.clone()) {
                v.push(f);
            }
        }
    }
    v
}

/// El CSS de un `<style>` o de un `<link rel="stylesheet">`, con el `href` del segundo.
fn hoja_de(doc: &Documento, id: NodoId, externa: &dyn Fn(&str) -> Option<String>) -> Option<(String, Option<String>)> {
    let e = doc.elemento(id)?;
    if e.nombre == "style" {
        return Some((doc.texto(id), None));
    }
    let rel = e.atributo("rel")?;
    let es_hoja =
        rel.split_ascii_whitespace().any(|x| x.eq_ignore_ascii_case("stylesheet")) && !rel.contains("alternate");
    let href = e.atributo("href").filter(|_| e.nombre == "link" && es_hoja)?;
    Some((externa(href)?, Some(String::from(href))))
}

fn ident(v: &[Token]) -> Option<String> {
    match v {
        [Token::Ident(x)] => Some(x.to_ascii_lowercase()),
        _ => None,
    }
}

/// Una regla `@font-face`; `None` si no tiene familia, ni una fuente que se sepa leer, o si su
/// `unicode-range` no cubre el latino.
fn leer_cara(decl: &[Declaracion], hoja: Option<String>) -> Option<FuenteWeb> {
    let valor = |p: &str| decl.iter().rev().find(|d| d.propiedad == p).map(|d| d.valor.as_slice());
    let familia = nombre_de_familia(valor("font-family")?)?;
    if valor("unicode-range").is_some_and(|r| !incluye_latino(r)) {
        return None;
    }
    let url = elegir_src(valor("src")?)?;
    let negrita = match valor("font-weight") {
        Some([Token::Numero(n)]) => *n >= 600.0,
        Some(v) => matches!(ident(v).as_deref(), Some("bold" | "bolder")),
        None => false,
    };
    let cursiva = valor("font-style").is_some_and(|v| {
        matches!(v.first(), Some(Token::Ident(x)) if x.eq_ignore_ascii_case("italic") || x.eq_ignore_ascii_case("oblique"))
    });
    Some(FuenteWeb { familia, negrita, cursiva, url, hoja })
}

/// `"Fira Sans"` o `Fira Sans` (palabras sueltas).
fn nombre_de_familia(v: &[Token]) -> Option<String> {
    let palabras: Vec<&str> = v
        .iter()
        .filter_map(|t| match t {
            Token::Cadena(s) | Token::Ident(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    let nombre = palabras.join(" ");
    (!nombre.trim().is_empty()).then(|| String::from(nombre.trim()))
}

/// La primera fuente de `src` que QUIMERA sabe leer: WOFF, TrueType u OpenType. Sin `format()`,
/// se mira la extensión de la dirección.
fn elegir_src(v: &[Token]) -> Option<String> {
    for parte in v.split(|t| *t == Token::Coma) {
        let mut url = None;
        let mut formato = None;
        let mut i = 0;
        while i < parte.len() {
            match (&parte[i], parte.get(i + 1)) {
                (Token::Url(u), _) => url = Some(u.clone()),
                (Token::Funcion(f), Some(Token::Cadena(u))) if f.eq_ignore_ascii_case("url") => url = Some(u.clone()),
                (Token::Funcion(f), Some(Token::Cadena(x) | Token::Ident(x))) if f.eq_ignore_ascii_case("format") => {
                    formato = Some(x.to_ascii_lowercase());
                }
                _ => {}
            }
            i += 1;
        }
        let Some(url) = url else { continue };
        let ruta = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
        let sabe = match formato.as_deref() {
            Some(f) => matches!(f, "woff" | "truetype" | "opentype"),
            None => ruta.ends_with(".woff") || ruta.ends_with(".ttf") || ruta.ends_with(".otf"),
        };
        if sabe {
            return Some(url);
        }
    }
    None
}

/// Si el `unicode-range` es el del latino básico: su primer rango (el bloque principal de cada
/// trozo: `U+0-FF` latino, `U+100-24F` latino extendido, `U+400-45F` cirílico) empieza antes de
/// `U+80`. El tokenizador parte `U+20A0-20AB` en `U`, `+20` con la «unidad» `a0`, y `-20AB`
/// (como manda CSS, que solo lo lee entero dentro de este descriptor): el número y las letras
/// que le siguen se vuelven a juntar y se leen en hexadecimal.
fn incluye_latino(v: &[Token]) -> bool {
    let Some(primero) = piezas(v).into_iter().next() else { return false };
    let hex = match primero {
        [Token::Ident(u), Token::Numero(n), ..] if u.eq_ignore_ascii_case("u") => alloc::format!("{}", *n as u64),
        [Token::Ident(u), Token::Dimension(n, unidad), ..] if u.eq_ignore_ascii_case("u") => {
            let letras: String = unidad.chars().take_while(char::is_ascii_hexdigit).collect();
            alloc::format!("{}{letras}", *n as u64)
        }
        _ => return false,
    };
    u32::from_str_radix(&hex, 16).is_ok_and(|inicio| inicio < 0x80)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn elige_lo_que_sabe_leer_y_el_latino() {
        let doc = quimera_html::analizar(
            "<style>
             @font-face { font-family: 'Fira Sans'; font-weight: 400;
               src: local('Fira'), url(f.woff2) format('woff2'), url('f.woff') format('woff');
               unicode-range: U+0000-00FF, U+0131 }
             @font-face { font-family: 'Fira Sans'; src: url(c.woff) format('woff'); unicode-range: U+0400-045F }
             @font-face { font-family: 'Fira Sans'; src: url(e.woff) format('woff'); unicode-range: U+100-24F, U+20A0-20AB }
             @font-face { font-family: Titular Uno; font-weight: bold; font-style: italic; src: url(t.ttf) }
             @font-face { font-family: Solo; src: url(s.woff2) format('woff2') }
             </style>",
        );
        let v = fuentes_web(&doc, &|_| None);
        let resumen: Vec<(&str, bool, bool, &str)> =
            v.iter().map(|f| (f.familia.as_str(), f.negrita, f.cursiva, f.url.as_str())).collect();
        assert_eq!(resumen, [("Fira Sans", false, false, "f.woff"), ("Titular Uno", true, true, "t.ttf")]);
    }
}
