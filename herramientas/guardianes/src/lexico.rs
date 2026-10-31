//! Lexer de Rust suficiente para los guardianes: separa código de cadenas y comentarios,
//! distingue vidas (`'a`) de caracteres y sabe qué partes son pruebas.

/// Clase de un token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clase {
    /// Identificador o palabra clave.
    Ident,
    /// Número.
    Numero,
    /// Cadena (de cualquier tipo).
    Cadena,
    /// Carácter o vida.
    Caracter,
    /// Signo de puntuación.
    Signo,
}

/// Un token con su línea (desde 1).
#[derive(Debug, Clone, Copy)]
pub struct Tok<'a> {
    /// Clase.
    pub clase: Clase,
    /// Texto.
    pub t: &'a str,
    /// Línea donde empieza.
    pub linea: usize,
}

fn es_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// Corta `texto[a..b]` sin partir un carácter UTF-8.
fn corte(texto: &str, mut a: usize, mut b: usize) -> &str {
    b = b.min(texto.len());
    while b < texto.len() && !texto.is_char_boundary(b) {
        b += 1;
    }
    a = a.min(b);
    while a > 0 && !texto.is_char_boundary(a) {
        a -= 1;
    }
    &texto[a..b]
}

/// Fin de un comentario de bloque que empieza en `i` (admite comentarios anidados).
fn fin_comentario_bloque(b: &[u8], mut i: usize) -> usize {
    let mut nivel = 0usize;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            nivel += 1;
            i += 2;
        } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
            nivel = nivel.saturating_sub(1);
            i += 2;
            if nivel == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    b.len()
}

/// Si en `i` empieza una cadena cruda (`r"..."`, `r#"..."#`, `br#"..."#`), dónde acaba.
fn fin_cadena_cruda(b: &[u8], i: usize) -> Option<usize> {
    let mut j = match (b[i], b.get(i + 1)) {
        (b'r', Some(b'"' | b'#')) => i + 1,
        (b'b' | b'c', Some(b'r')) if matches!(b.get(i + 2), Some(b'"' | b'#')) => i + 2,
        _ => return None,
    };
    let h0 = j;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    let almohadillas = j - h0;
    if b.get(j) != Some(&b'"') {
        return None; // r#identificador
    }
    let mut k = j + 1;
    while k < b.len()
        && !(b[k] == b'"' && b.len() - k > almohadillas && b[k + 1..=k + almohadillas].iter().all(|x| *x == b'#'))
    {
        k += 1;
    }
    Some((k + 1 + almohadillas).min(b.len()))
}

/// Si en `i` empieza una cadena normal (`"..."`, `b"..."`, `c"..."`), dónde acaba.
fn fin_cadena(b: &[u8], i: usize) -> Option<usize> {
    let mut j = match (b[i], b.get(i + 1)) {
        (b'"', _) => i + 1,
        (b'b' | b'c', Some(b'"')) => i + 2,
        _ => return None,
    };
    while j < b.len() && b[j] != b'"' {
        j += if b[j] == b'\\' { 2 } else { 1 };
    }
    Some((j + 1).min(b.len()))
}

/// Si en `i` empieza un carácter (`'x'`, `'\n'`, `b'x'`) o una vida (`'a`), dónde acaba.
fn fin_caracter(texto: &str, b: &[u8], i: usize) -> Option<usize> {
    let q = match (b[i], b.get(i + 1)) {
        (b'\'', _) => i,
        (b'b', Some(b'\'')) => i + 1,
        _ => return None,
    };
    if b.get(q + 1) == Some(&b'\\') {
        // Saltar la barra y el carácter escapado ('\'' también es un carácter).
        let mut j = (q + 3).min(b.len());
        while j < b.len() && b[j] != b'\'' && b[j] != b'\n' {
            j += 1;
        }
        return Some((j + 1).min(b.len()));
    }
    let largo = texto.get(q + 1..).and_then(|r| r.chars().next()).map_or(1, char::len_utf8);
    if b.get(q + 1 + largo) == Some(&b'\'') {
        return Some(q + 2 + largo);
    }
    let mut j = q + 1;
    while j < b.len() && es_ident(b[j]) {
        j += 1;
    }
    Some(j)
}

/// Fin de un número o de un identificador que empieza en `i`.
fn fin_palabra(b: &[u8], mut i: usize, numero: bool) -> usize {
    while i < b.len() && (es_ident(b[i]) || (numero && b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))) {
        i += 1;
    }
    i
}

/// Tokens de código y, aparte, los comentarios (cada uno como un token con su línea de inicio).
pub fn tokens_y_comentarios(texto: &str) -> (Vec<Tok<'_>>, Vec<Tok<'_>>) {
    let b = texto.as_bytes();
    let mut v = Vec::new();
    let mut comentarios = Vec::new();
    let (mut i, mut linea) = (0usize, 1usize);
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            linea += usize::from(c == b'\n');
            i += 1;
            continue;
        }
        let a = i;
        let (fin, clase) = if c == b'/' && b.get(i + 1) == Some(&b'/') {
            (b[i..].iter().position(|x| *x == b'\n').map_or(b.len(), |p| i + p), None)
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            (fin_comentario_bloque(b, i), None)
        } else if let Some(f) = fin_cadena_cruda(b, i).or_else(|| fin_cadena(b, i)) {
            (f, Some(Clase::Cadena))
        } else if let Some(f) = fin_caracter(texto, b, i) {
            (f, Some(Clase::Caracter))
        } else if c.is_ascii_digit() {
            (fin_palabra(b, i, true), Some(Clase::Numero))
        } else if es_ident(c) {
            (fin_palabra(b, i, false), Some(Clase::Ident))
        } else {
            let largo = if [b"::", b"->", b"=>"].iter().any(|s| b[i..].starts_with(*s)) { 2 } else { 1 };
            (i + largo, Some(Clase::Signo))
        };
        match clase {
            Some(clase) => v.push(Tok { clase, t: corte(texto, a, fin), linea }),
            None => comentarios.push(Tok { clase: Clase::Signo, t: corte(texto, a, fin), linea }),
        }
        linea += b[a..fin].iter().filter(|x| **x == b'\n').count();
        i = fin;
    }
    (v, comentarios)
}

/// Texto de un token que es código (las cadenas y caracteres cuentan como `""`).
pub fn t<'a>(k: &[Tok<'a>], i: usize) -> &'a str {
    match k.get(i) {
        Some(x) if x.clase != Clase::Cadena && x.clase != Clase::Caracter => x.t,
        _ => "",
    }
}

/// Índice del `}` que cierra la llave abierta en `abre`.
pub fn cierre(k: &[Tok], abre: usize) -> Option<usize> {
    let mut nivel = 0i32;
    for (j, _) in k.iter().enumerate().skip(abre) {
        match t(k, j) {
            "{" => nivel += 1,
            "}" => {
                nivel -= 1;
                if nivel == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// Un archivo entero de pruebas: en `tests/`, llamado `pruebas.rs` o `tests.rs`, o con `#![cfg(test)]`.
pub fn archivo_de_prueba(ruta: &str, k: &[Tok]) -> bool {
    let nombre = ruta.rsplit('/').next().unwrap_or(ruta);
    ruta.contains("/tests/")
        || matches!(nombre, "pruebas.rs" | "tests.rs")
        || (0..k.len().min(64)).any(|i| {
            [t(k, i), t(k, i + 1), t(k, i + 2), t(k, i + 3), t(k, i + 4), t(k, i + 5)]
                == ["#", "!", "[", "cfg", "(", "test"]
        })
}

/// Para cada token: si está dentro de `#[cfg(test)] mod ... { }` o de `#[test] fn ... { }`.
pub fn regiones_de_prueba(k: &[Tok]) -> Vec<bool> {
    let mut prueba = vec![false; k.len()];
    let mut i = 0;
    while i < k.len() {
        let es_test = t(k, i) == "#"
            && t(k, i + 1) == "["
            && ((t(k, i + 2) == "test" && t(k, i + 3) == "]")
                || (t(k, i + 2) == "cfg" && t(k, i + 3) == "(" && t(k, i + 4) == "test" && t(k, i + 5) == ")"));
        if es_test {
            // El elemento marcado: hasta su `{` (si antes hay `;`, no tiene cuerpo).
            if let Some(abre) = (i + 3..k.len()).find(|j| matches!(t(k, *j), "{" | ";")).filter(|j| t(k, *j) == "{") {
                let fin = cierre(k, abre).unwrap_or(k.len() - 1);
                prueba[i..=fin].iter_mut().for_each(|p| *p = true);
                i = fin;
            }
        }
        i += 1;
    }
    prueba
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn cadenas_comentarios_y_vidas_no_se_confunden() {
        let fuente = "fn f<'a>(x: &'a str) {\n  let s = r#\"unsafe { }\"#; // unsafe\n  /* unsafe /* anidado */ */\n  let c = '{';\n  unsafe { g() }\n}\n";
        let k = tokens_y_comentarios(fuente).0;
        let unsafes: Vec<usize> =
            k.iter().filter(|x| x.clase == Clase::Ident && x.t == "unsafe").map(|x| x.linea).collect();
        assert_eq!(unsafes, [5], "solo el unsafe de verdad, en su línea");
        assert_eq!(cierre(&k, k.iter().position(|x| x.t == "{").unwrap()), Some(k.len() - 1));
    }

    #[test]
    fn sabe_que_es_prueba() {
        let fuente =
            "fn a() { x.unwrap(); }\n#[cfg(test)]\nmod p { fn b() { y.unwrap(); } }\n#[test]\nfn c() { z.unwrap(); }\n";
        let k = tokens_y_comentarios(fuente).0;
        let p = regiones_de_prueba(&k);
        let fuera: Vec<usize> =
            k.iter().enumerate().filter(|(i, x)| x.t == "unwrap" && !p[*i]).map(|(_, x)| x.linea).collect();
        assert_eq!(fuera, [1]);
    }
}
