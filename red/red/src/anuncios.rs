//! Los anuncios de vídeo de YouTube no vienen de otro dominio: llegan dentro de la respuesta del
//! reproductor (`ytInitialPlayerResponse` en el HTML de `/watch` y el JSON de
//! `/youtubei/v1/player`), en las claves `adPlacements`, `playerAds` y `adSlots`. Si no están,
//! el reproductor no pone anuncios. Es lo que hace Brave con su *scriptlet* `json-prune`; aquí se
//! quitan del texto antes de que la página lo lea.

/// Las claves que se quitan.
pub const CLAVES: [&str; 3] = ["adPlacements", "playerAds", "adSlots"];

/// Si la respuesta de esta dirección puede llevar anuncios de vídeo de YouTube.
pub fn es_de_youtube(anfitrion: &str) -> bool {
    let a = anfitrion.to_ascii_lowercase();
    a == "youtube.com"
        || a.ends_with(".youtube.com")
        || a == "youtube-nocookie.com"
        || a.ends_with(".youtube-nocookie.com")
}

/// Quita del texto (HTML o JSON) cada `"clave": valor` de [`CLAVES`], con su coma. Devuelve el
/// texto y cuántas quitó. Si una no se puede leer bien (JSON cortado), se deja como está.
pub fn quitar(texto: &str) -> (String, usize) {
    let mut s = String::from(texto);
    let mut quitadas = 0;
    for clave in CLAVES {
        let aguja = format!("\"{clave}\"");
        let mut desde = 0;
        while let Some(i) = s[desde..].find(&aguja).map(|i| i + desde) {
            match rango(&s, i, aguja.len()) {
                Some((a, b)) => {
                    s.replace_range(a..b, "");
                    quitadas += 1;
                    desde = a;
                }
                None => desde = i + aguja.len(),
            }
        }
    }
    (s, quitadas)
}

/// El trozo a quitar para la clave que empieza en `i` (de largo `n`): `"clave": valor` y una
/// coma de al lado. `None` si no es una clave de un objeto o el valor no se termina.
fn rango(s: &str, i: usize, n: usize) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let antes = s[..i].trim_end();
    let previo = antes.as_bytes().last().copied()?;
    if !matches!(previo, b'{' | b',') {
        return None;
    }
    let mut j = i + n;
    j += espacios(&b[j..]);
    if b.get(j) != Some(&b':') {
        return None;
    }
    j += 1;
    j += espacios(&b[j..]);
    let fin = j + valor(&b[j..])?;
    let tras = fin + espacios(&b[fin..]);
    if b.get(tras) == Some(&b',') {
        Some((i, tras + 1))
    } else if previo == b',' {
        // La última del objeto: se va la coma de antes.
        Some((antes.len() - 1, fin))
    } else {
        Some((i, fin))
    }
}

fn espacios(b: &[u8]) -> usize {
    b.iter().take_while(|c| c.is_ascii_whitespace()).count()
}

/// Lo que mide el valor JSON del principio de `b` (objeto, lista, cadena o literal).
fn valor(b: &[u8]) -> Option<usize> {
    match b.first()? {
        b'"' => cadena(b),
        b'[' | b'{' => {
            let mut nivel = 0usize;
            let mut k = 0;
            while k < b.len() {
                match b[k] {
                    b'"' => k += cadena(&b[k..])? - 1,
                    b'[' | b'{' => nivel += 1,
                    b']' | b'}' => {
                        nivel -= 1;
                        if nivel == 0 {
                            return Some(k + 1);
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            None
        }
        _ => {
            let n = b.iter().take_while(|c| !matches!(c, b',' | b'}' | b']')).count();
            (n < b.len()).then_some(n)
        }
    }
}

/// Lo que mide la cadena JSON del principio de `b` (con sus comillas).
fn cadena(b: &[u8]) -> Option<usize> {
    let mut k = 1;
    while k < b.len() {
        match b[k] {
            b'\\' => k += 2,
            b'"' => return Some(k + 1),
            _ => k += 1,
        }
    }
    None
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn quita_las_claves_de_anuncios() {
        let j =
            r#"{"a":1,"adPlacements":[{"x":"]}\""},{}],"playerAds":[],"videoDetails":{"t":"v"},"adSlots":[{"s":1}]}"#;
        let (s, n) = quitar(j);
        assert_eq!(s, r#"{"a":1,"videoDetails":{"t":"v"}}"#);
        assert_eq!(n, 3);
        let (s, n) = quitar(r#"{ "adSlots" : [1, 2] , "b": 2}"#);
        assert_eq!((s.as_str(), n), (r#"{  "b": 2}"#, 1));
        let (s, _) = quitar(r#"{"playerAds":true}"#);
        assert_eq!(s, "{}");
    }

    #[test]
    fn en_el_html_de_watch() {
        let html = r#"<script>var ytInitialPlayerResponse = {"streamingData":{},"playerAds":[{"p":1}],"adPlacements":[]};var meta = 1;</script><p>"adSlots" en un texto</p>"#;
        let (s, n) = quitar(html);
        assert_eq!(n, 2);
        assert!(s.contains(r#"{"streamingData":{}};var meta"#), "{s}");
        assert!(s.contains(r#"<p>"adSlots" en un texto</p>"#), "lo que no es una clave se queda");
    }

    #[test]
    fn cortado_o_sin_anuncios() {
        let roto = r#"{"adPlacements":[{"x":1}"#;
        assert_eq!(quitar(roto), (String::from(roto), 0));
        let limpio = r#"{"videoDetails":{"title":"adPlacements"}}"#;
        assert_eq!(quitar(limpio), (String::from(limpio), 0));
        assert!(es_de_youtube("www.YouTube.com") && es_de_youtube("youtube.com") && !es_de_youtube("notyoutube.com"));
    }
}
