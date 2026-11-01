//! Selectores de CSS para `querySelector`, `querySelectorAll`, `matches` y `closest`.
//!
//! Lo que más usan los scripts: tipo, `*`, `#id`, `.clase`, `[atributo]`, `[atributo=valor]`
//! (también `^=`, `$=`, `*=`, `~=`), los combinadores descendiente (espacio) y `>`, y listas con
//! comas. Un selector con algo que no se entiende (`:hover`, `+`) no coincide con nada.

use alloc::string::String;
use alloc::vec::Vec;
use quimera_html::{Documento, NodoId};

/// `[nombre op valor]`.
#[derive(Debug, Clone)]
struct Atributo {
    nombre: String,
    /// `None`: basta con que exista. `Some((op, valor))`, con `op` `=`, `^`, `$`, `*` o `~`.
    valor: Option<(char, String)>,
}

/// Un selector compuesto: `div#a.b[c]`.
#[derive(Debug, Clone, Default)]
struct Compuesto {
    etiqueta: Option<String>,
    id: Option<String>,
    clases: Vec<String>,
    atributos: Vec<Atributo>,
}

/// Una cadena de compuestos; `true` en `hijo` es `>` con el anterior, `false` un espacio.
#[derive(Debug, Clone)]
struct Complejo {
    partes: Vec<(bool, Compuesto)>,
}

/// Una lista de selectores (`a, b`). `None` si alguno no se entiende.
#[derive(Debug, Clone)]
pub struct Selector {
    lista: Vec<Complejo>,
}

fn nombre_valido(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

fn leer_nombre(s: &[char], i: &mut usize) -> String {
    let mut n = String::new();
    while *i < s.len() && nombre_valido(s[*i]) {
        n.push(s[*i]);
        *i += 1;
    }
    n
}

fn leer_atributo(s: &[char], i: &mut usize) -> Option<Atributo> {
    let nombre = leer_nombre(s, i).to_ascii_lowercase();
    if nombre.is_empty() {
        return None;
    }
    if s.get(*i) == Some(&']') {
        *i += 1;
        return Some(Atributo { nombre, valor: None });
    }
    let op = match s.get(*i)? {
        '=' => '=',
        c @ ('^' | '$' | '*' | '~') if s.get(*i + 1) == Some(&'=') => {
            *i += 1;
            *c
        }
        _ => return None,
    };
    *i += 1;
    let mut valor = String::new();
    if let Some(&q @ ('"' | '\'')) = s.get(*i) {
        *i += 1;
        while *i < s.len() && s[*i] != q {
            valor.push(s[*i]);
            *i += 1;
        }
        *i += 1;
    } else {
        valor = leer_nombre(s, i);
    }
    (s.get(*i) == Some(&']')).then(|| *i += 1)?;
    Some(Atributo { nombre, valor: Some((op, valor)) })
}

fn leer_compuesto(s: &[char], i: &mut usize) -> Option<Compuesto> {
    let mut c = Compuesto::default();
    let inicio = *i;
    if s.get(*i) == Some(&'*') {
        *i += 1;
    } else if s.get(*i).is_some_and(|&x| nombre_valido(x)) {
        c.etiqueta = Some(leer_nombre(s, i).to_ascii_lowercase());
    }
    loop {
        match s.get(*i) {
            Some('#') => {
                *i += 1;
                c.id = Some(leer_nombre(s, i));
            }
            Some('.') => {
                *i += 1;
                c.clases.push(leer_nombre(s, i));
            }
            Some('[') => {
                *i += 1;
                c.atributos.push(leer_atributo(s, i)?);
            }
            _ => break,
        }
    }
    (*i > inicio).then_some(c)
}

impl Selector {
    /// Analiza un selector.
    pub fn analizar(texto: &str) -> Option<Selector> {
        let mut lista = Vec::new();
        for parte in texto.split(',') {
            let s: Vec<char> = parte.trim().chars().collect();
            let (mut i, mut partes, mut hijo) = (0, Vec::new(), false);
            while i < s.len() {
                partes.push((hijo, leer_compuesto(&s, &mut i)?));
                let mut espacio = false;
                while s.get(i).is_some_and(|c| c.is_whitespace()) {
                    (i, espacio) = (i + 1, true);
                }
                hijo = s.get(i) == Some(&'>');
                if hijo {
                    i += 1;
                    while s.get(i).is_some_and(|c| c.is_whitespace()) {
                        i += 1;
                    }
                } else if !espacio && i < s.len() {
                    return None; // `:hover`, `+`, `~`...
                }
            }
            if partes.is_empty() {
                return None;
            }
            lista.push(Complejo { partes });
        }
        Some(Selector { lista })
    }

    /// Si el elemento `id` coincide.
    pub fn coincide(&self, doc: &Documento, id: NodoId) -> bool {
        self.lista.iter().any(|c| coincide_desde(doc, id, &c.partes))
    }
}

fn coincide_compuesto(doc: &Documento, id: NodoId, c: &Compuesto) -> bool {
    let Some(e) = doc.elemento(id) else { return false };
    c.etiqueta.as_ref().is_none_or(|t| *t == e.nombre)
        && c.id.as_ref().is_none_or(|x| e.atributo("id") == Some(x.as_str()))
        && c.clases.iter().all(|x| e.tiene_clase(x))
        && c.atributos.iter().all(|a| {
            let Some(v) = e.atributo(&a.nombre) else { return false };
            match &a.valor {
                None => true,
                Some(('=', x)) => v == x,
                Some(('^', x)) => !x.is_empty() && v.starts_with(x.as_str()),
                Some(('$', x)) => !x.is_empty() && v.ends_with(x.as_str()),
                Some(('*', x)) => !x.is_empty() && v.contains(x.as_str()),
                Some((_, x)) => v.split_ascii_whitespace().any(|p| p == x),
            }
        })
}

/// De derecha a izquierda: el último compuesto es `id`; los anteriores, sus antepasados.
fn coincide_desde(doc: &Documento, id: NodoId, partes: &[(bool, Compuesto)]) -> bool {
    let Some(((hijo, ultimo), resto)) = partes.split_last() else { return true };
    if !coincide_compuesto(doc, id, ultimo) {
        return false;
    }
    if resto.is_empty() {
        return true;
    }
    let mut padre = doc.nodo(id).padre;
    while let Some(p) = padre {
        if coincide_desde(doc, p, resto) {
            return true;
        }
        if *hijo {
            return false;
        }
        padre = doc.nodo(p).padre;
    }
    false
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use quimera_html::analizar;

    #[test]
    fn selectores() {
        let doc = analizar(
            "<div id=a class='x y'><p class=x><a href='https://e.com/z' data-k=v>1</a></p></div><span class=x></span>",
        );
        let todos = |s: &str| -> Vec<String> {
            let sel = Selector::analizar(s).expect("selector");
            doc.descendientes(Documento::RAIZ)
                .filter(|&d| sel.coincide(&doc, d))
                .filter_map(|d| doc.elemento(d).map(|e| e.nombre.clone()))
                .collect()
        };
        assert_eq!(todos(".x"), ["div", "p", "span"]);
        assert_eq!(todos("div .x"), ["p"]);
        assert_eq!(todos("#a > a"), Vec::<String>::new());
        assert_eq!(todos("#a > p > a[href^='https'][data-k=v]"), ["a"]);
        assert_eq!(todos("div.x.y, span"), ["div", "span"]);
        assert_eq!(todos("*[href$=z]"), ["a"]);
        assert!(Selector::analizar("a:hover").is_none());
        assert!(Selector::analizar("a + b").is_none());
    }
}
