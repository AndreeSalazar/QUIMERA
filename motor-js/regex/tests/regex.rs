//! El motor de expresiones regulares contra resultados conocidos de JavaScript.

use quimera_regex::{Memoria, Regex};

/// Primer encaje desde 0: el texto de cada grupo (`None` si no participó).
fn grupos(patron: &str, banderas: &str, texto: &str) -> Option<Vec<Option<String>>> {
    let re = Regex::nueva(patron, banderas).unwrap_or_else(|e| panic!("/{patron}/: {}", e.mensaje));
    let mut m = Memoria::default();
    if !re.buscar(texto, 0, &mut m) {
        return None;
    }
    Some((0..=re.grupos()).map(|g| m.captura(g).map(|(a, b)| texto[a..b].to_string())).collect())
}

fn encaje(patron: &str, banderas: &str, texto: &str) -> Option<String> {
    grupos(patron, banderas, texto).and_then(|g| g[0].clone())
}

fn s(x: &str) -> Option<String> {
    Some(x.to_string())
}

#[test]
fn literales_clases_y_anclas() {
    assert_eq!(encaje("zbmvyyn", "", "Zbmvyyn/5.0 zbmvyyn"), s("zbmvyyn"));
    assert_eq!(encaje("^ba", "", "abba"), None);
    assert_eq!(encaje("^ba", "", "bad"), s("ba"));
    assert_eq!(encaje("[a-c]+", "", "xxabcabd"), s("abcab"));
    assert_eq!(encaje("[^a-c]+", "", "abcxyzab"), s("xyz"));
    assert_eq!(encaje(r"\d+\.\d*", "", "v528.9 x"), s("528.9"));
    assert_eq!(encaje(r"\bQBZ\b", "", "xQBZ QBZ"), s("QBZ"));
    assert_eq!(encaje(r"\s+$", "", "ab  \t"), s("  \t"));
    assert_eq!(encaje(r"[\s\xa0]+", "", "a\u{a0} b"), s("\u{a0} "));
    // ñ y ú están por debajo de Ĩ: como en JavaScript, no son parte de la clase.
    assert_eq!(encaje(r"[\wĨ-￿*_-]+", "", "#ñandú ŝandŭ-x*"), s("and"));
    assert_eq!(encaje(r"[\wĨ-￿*_-]{4,}", "", "#ñandú ŝandŭ-x*"), s("ŝandŭ-x*"));
    assert_eq!(encaje(".+", "", "ab\ncd"), s("ab"));
    assert_eq!(encaje(".+", "s", "ab\ncd"), s("ab\ncd"));
    assert_eq!(encaje("^c", "m", "ab\ncd"), s("c"));
    assert_eq!(encaje("b$", "m", "ab\ncd"), s("b"));
}

#[test]
fn grupos_alternativas_y_repeticiones() {
    let url = r"(((\w+):\/\/)([^\/:]*)(:(\d+))?)?([^#?]*)(\?([^#]*))?(#(.*))?";
    let g = grupos(url, "", "http://www.x.com:80/a/b?q=1#frag").unwrap();
    assert_eq!(g[3], s("http"));
    assert_eq!(g[4], s("www.x.com"));
    assert_eq!(g[6], s("80"));
    assert_eq!(g[7], s("/a/b"));
    assert_eq!(g[9], s("q=1"));
    assert_eq!(g[11], s("frag"));
    assert_eq!(encaje("qqqq|qqq|qq|q", "", "xqqq"), s("qqq"));
    assert_eq!(encaje("a{2,3}", "", "aaaa"), s("aaa"));
    assert_eq!(encaje("a{2,3}?", "", "aaaa"), s("aa"));
    assert_eq!(encaje("a{2}", "", "a a aa"), s("aa"));
    assert_eq!(encaje("x{,2}", "", "x{,2}"), s("x{,2}"), "anexo B: llaves literales");
    assert_eq!(encaje("<.*?>", "", "<a><b>"), s("<a>"));
    assert_eq!(grupos("(a)|b", "", "b").unwrap(), vec![s("b"), None]);
    // Cada vuelta borra las capturas de la anterior: (z)((a+)?(b+)?(c))* sobre "zaacbbbcac".
    let g = grupos("(z)((a+)?(b+)?(c))*", "", "zaacbbbcac").unwrap();
    assert_eq!(g, vec![s("zaacbbbcac"), s("z"), s("ac"), s("a"), None, s("c")]);
}

#[test]
fn referencias_mirar_y_sin_caso() {
    assert_eq!(encaje(r"(\w)\1", "", "abccd"), s("cc"));
    assert_eq!(encaje(r"(a)\1", "i", "aA"), s("aA"));
    assert_eq!(encaje(r"\w+(?=,)", "", "uno, dos"), s("uno"));
    assert_eq!(encaje(r"\d+(?!px)\b", "", "12px 34em 56"), s("56"));
    assert_eq!(encaje("SEVRAQVQ", "i", "x sevraqvq"), s("sevraqvq"));
    assert_eq!(encaje("[a-z]+", "i", "123ABC"), s("ABC"));
    assert_eq!(encaje("(-[a-z])", "i", "x-Y"), s("-Y"));
}

#[test]
fn vacios_que_no_giran_para_siempre() {
    assert_eq!(encaje("(a*)*b", "", "aaab"), s("aaab"));
    assert_eq!(encaje("(a|)*", "", "aa"), s("aa"));
    assert_eq!(encaje(r"^\s*|\s*$", "", "  x  "), s("  "));
    assert_eq!(encaje(r"(\d*)(\D*)", "", "abc"), s("abc"));
}

#[test]
fn errores_de_sintaxis() {
    for (p, b) in [("(", ""), ("a)", ""), ("[a", ""), ("*a", ""), ("a{3,2}", ""), ("a", "gg"), ("a", "q"), ("\\", "")] {
        assert!(Regex::nueva(p, b).is_err(), "/{p}/{b} debería fallar");
    }
}

#[test]
fn textos_largos_sin_desbordar_la_pila() {
    let largo = "a".repeat(200_000);
    assert_eq!(encaje("(a|b)*c?$", "", &largo).map(|x| x.len()), Some(200_000));
    let mut m = Memoria::default();
    let re = Regex::nueva("b", "g").unwrap();
    assert!(!re.buscar(&largo, 10, &mut m));
}
