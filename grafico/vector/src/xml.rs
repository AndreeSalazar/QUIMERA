//! Un lector de XML mínimo para SVG: elementos y atributos. Se salta el texto, los comentarios,
//! `<?xml ...?>`, `<!DOCTYPE>` y `CDATA`. No valida: lo que no entiende lo deja pasar.

use alloc::string::String;
use alloc::vec::Vec;

/// Un elemento con sus atributos y los números de sus hijos.
#[derive(Debug, Default)]
pub struct Elemento {
    /// El nombre sin prefijo (`svg:path` → `path`).
    pub nombre: String,
    pub atributos: Vec<(String, String)>,
    pub hijos: Vec<usize>,
}

impl Elemento {
    /// El valor de un atributo, sin distinguir mayúsculas (el HTML guarda `viewbox`); `xlink:href`
    /// también vale como `href`.
    pub fn atributo(&self, n: &str) -> Option<&str> {
        let igual =
            |k: &str| k.eq_ignore_ascii_case(n) || k.rsplit(':').next().is_some_and(|k| k.eq_ignore_ascii_case(n));
        self.atributos.iter().find(|(k, _)| igual(k)).map(|(_, v)| v.as_str())
    }
}

/// Lee `texto`; el elemento 0 es una raíz vacía cuyos hijos son los elementos de arriba.
pub fn leer(texto: &str) -> Vec<Elemento> {
    let b = texto.as_bytes();
    let mut els = alloc::vec![Elemento::default()];
    let mut pila = alloc::vec![0usize];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if texto[i..].starts_with("<!--") {
            i = texto[i..].find("-->").map_or(b.len(), |f| i + f + 3);
        } else if texto[i..].starts_with("<![CDATA[") {
            i = texto[i..].find("]]>").map_or(b.len(), |f| i + f + 3);
        } else if b.get(i + 1).is_some_and(|c| *c == b'?' || *c == b'!') {
            i = texto[i..].find('>').map_or(b.len(), |f| i + f + 1);
        } else if b.get(i + 1) == Some(&b'/') {
            if pila.len() > 1 {
                pila.pop();
            }
            i = texto[i..].find('>').map_or(b.len(), |f| i + f + 1);
        } else {
            let (el, fin, solo) = etiqueta(texto, i + 1);
            let n = els.len();
            els.push(el);
            if let Some(&padre) = pila.last() {
                els[padre].hijos.push(n);
            }
            if !solo {
                pila.push(n);
            }
            i = fin;
        }
    }
    els
}

/// Una etiqueta de apertura desde `i` (tras `<`): el elemento, dónde acaba y si se cierra sola.
fn etiqueta(texto: &str, mut i: usize) -> (Elemento, usize, bool) {
    let b = texto.as_bytes();
    let es_fin = |c: u8| c.is_ascii_whitespace() || c == b'>' || c == b'/';
    let inicio = i;
    while i < b.len() && !es_fin(b[i]) {
        i += 1;
    }
    let nombre = texto[inicio..i].rsplit(':').next().unwrap_or("");
    let mut el = Elemento { nombre: String::from(nombre), ..Elemento::default() };
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        match b.get(i) {
            None => return (el, i, true),
            Some(b'>') => return (el, i + 1, false),
            Some(b'/') => {
                let fin = texto[i..].find('>').map_or(b.len(), |f| i + f + 1);
                return (el, fin, true);
            }
            _ => {}
        }
        let k0 = i;
        while i < b.len() && b[i] != b'=' && !es_fin(b[i]) {
            i += 1;
        }
        let clave = String::from(&texto[k0..i]);
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut valor = String::new();
        if b.get(i) == Some(&b'=') {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if let Some(&q) = b.get(i).filter(|c| **c == b'"' || **c == b'\'') {
                let fin = texto[i + 1..].find(q as char).map_or(b.len(), |f| i + 1 + f);
                valor = entidades(&texto[i + 1..fin]);
                i = fin + 1;
            }
        }
        if clave.is_empty() {
            i += 1;
        } else {
            el.atributos.push((clave, valor));
        }
    }
}

/// Las entidades más comunes en atributos.
fn entidades(s: &str) -> String {
    if !s.contains('&') {
        return String::from(s);
    }
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn elementos_y_atributos() {
        let e = leer(
            r##"<?xml version="1.0"?><!-- c --><svg viewBox="0 0 24 24" xmlns:xlink="x"><g fill='red'><path d="M0 0L1 1"/></g><use xlink:href="#a"/></svg>"##,
        );
        let svg = &e[e[0].hijos[0]];
        assert_eq!(svg.nombre, "svg");
        assert_eq!(svg.atributo("viewBox"), Some("0 0 24 24"));
        let g = &e[svg.hijos[0]];
        assert_eq!((g.nombre.as_str(), g.atributo("fill")), ("g", Some("red")));
        assert_eq!(e[g.hijos[0]].atributo("d"), Some("M0 0L1 1"));
        assert_eq!(e[svg.hijos[1]].atributo("href"), Some("#a"));
    }
}
