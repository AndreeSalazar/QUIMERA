//! Referencias de caracteres: `&amp;`, `&#233;`, `&#x1F600;`.
//!
//! La tabla de nombres es la de uso común (unas 120 de las 2.231 del estándar): las que de
//! verdad aparecen en las páginas. Las numéricas siguen el estándar entero, incluido el
//! remapeo de Windows-1252 (`&#150;` es `–`).

use alloc::string::String;

/// (nombre, texto). Ordenada por nombre para buscar por bisección.
const NOMBRES: &[(&str, &str)] = &[
    ("AElig", "Æ"),
    ("Aacute", "Á"),
    ("Agrave", "À"),
    ("Alpha", "Α"),
    ("Aring", "Å"),
    ("Atilde", "Ã"),
    ("Auml", "Ä"),
    ("Beta", "Β"),
    ("Ccedil", "Ç"),
    ("Delta", "Δ"),
    ("Eacute", "É"),
    ("Egrave", "È"),
    ("Euml", "Ë"),
    ("Gamma", "Γ"),
    ("Iacute", "Í"),
    ("Lambda", "Λ"),
    ("Ntilde", "Ñ"),
    ("Oacute", "Ó"),
    ("Omega", "Ω"),
    ("Ouml", "Ö"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Uacute", "Ú"),
    ("Uuml", "Ü"),
    ("aacute", "á"),
    ("acirc", "â"),
    ("acute", "´"),
    ("aelig", "æ"),
    ("agrave", "à"),
    ("alpha", "α"),
    ("amp", "&"),
    ("apos", "'"),
    ("aring", "å"),
    ("asymp", "≈"),
    ("atilde", "ã"),
    ("auml", "ä"),
    ("bdquo", "„"),
    ("beta", "β"),
    ("brvbar", "¦"),
    ("bull", "•"),
    ("ccedil", "ç"),
    ("cedil", "¸"),
    ("cent", "¢"),
    ("check", "✓"),
    ("copy", "©"),
    ("cross", "✗"),
    ("curren", "¤"),
    ("dagger", "†"),
    ("darr", "↓"),
    ("deg", "°"),
    ("delta", "δ"),
    ("divide", "÷"),
    ("eacute", "é"),
    ("ecirc", "ê"),
    ("egrave", "è"),
    ("emsp", "\u{2003}"),
    ("ensp", "\u{2002}"),
    ("epsilon", "ε"),
    ("equiv", "≡"),
    ("euml", "ë"),
    ("euro", "€"),
    ("frac12", "½"),
    ("frac14", "¼"),
    ("frac34", "¾"),
    ("gamma", "γ"),
    ("ge", "≥"),
    ("gt", ">"),
    ("harr", "↔"),
    ("hearts", "♥"),
    ("hellip", "…"),
    ("iacute", "í"),
    ("icirc", "î"),
    ("iexcl", "¡"),
    ("igrave", "ì"),
    ("infin", "∞"),
    ("iquest", "¿"),
    ("iuml", "ï"),
    ("lambda", "λ"),
    ("laquo", "«"),
    ("larr", "←"),
    ("ldquo", "“"),
    ("le", "≤"),
    ("lsaquo", "‹"),
    ("lsquo", "‘"),
    ("lt", "<"),
    ("macr", "¯"),
    ("mdash", "—"),
    ("micro", "µ"),
    ("middot", "·"),
    ("minus", "−"),
    ("mu", "μ"),
    ("nbsp", "\u{a0}"),
    ("ndash", "–"),
    ("ne", "≠"),
    ("not", "¬"),
    ("ntilde", "ñ"),
    ("oacute", "ó"),
    ("ocirc", "ô"),
    ("ograve", "ò"),
    ("omega", "ω"),
    ("ordf", "ª"),
    ("ordm", "º"),
    ("otilde", "õ"),
    ("ouml", "ö"),
    ("para", "¶"),
    ("permil", "‰"),
    ("pi", "π"),
    ("plusmn", "±"),
    ("pound", "£"),
    ("prime", "′"),
    ("quot", "\""),
    ("raquo", "»"),
    ("rarr", "→"),
    ("rdquo", "”"),
    ("reg", "®"),
    ("rsaquo", "›"),
    ("rsquo", "’"),
    ("sbquo", "‚"),
    ("sect", "§"),
    ("shy", "\u{ad}"),
    ("sigma", "σ"),
    ("sup1", "¹"),
    ("sup2", "²"),
    ("sup3", "³"),
    ("szlig", "ß"),
    ("theta", "θ"),
    ("thinsp", "\u{2009}"),
    ("times", "×"),
    ("trade", "™"),
    ("uacute", "ú"),
    ("uarr", "↑"),
    ("ucirc", "û"),
    ("ugrave", "ù"),
    ("uml", "¨"),
    ("uuml", "ü"),
    ("yen", "¥"),
    ("yuml", "ÿ"),
    ("zwj", "\u{200d}"),
    ("zwnj", "\u{200c}"),
];

/// Las que los navegadores aceptan sin `;` final (por compatibilidad con HTML antiguo).
const SIN_PUNTO_Y_COMA: &[&str] = &["amp", "lt", "gt", "quot", "nbsp", "copy", "reg"];

/// `&#128;`–`&#159;` no son los controles C1 sino los caracteres de Windows-1252.
const WINDOWS_1252: [u32; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D,
    0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D,
    0x017E, 0x0178,
];

/// Lee una referencia que empieza en `s` justo después del `&`. Devuelve el texto y cuántos
/// bytes consumió, o `None` si no es una referencia (el `&` se queda como está).
/// `en_atributo`: una con nombre sin `;` seguida de letra, cifra o `=` no se decodifica.
pub(crate) fn leer(s: &str, en_atributo: bool) -> Option<(String, usize)> {
    if let Some(resto) = s.strip_prefix('#') {
        return numerica(resto).map(|(c, n)| (String::from(c), n + 1));
    }
    let largo = s.bytes().take_while(u8::is_ascii_alphanumeric).count();
    let nombre = &s[..largo];
    let con_punto = s[largo..].starts_with(';');
    if con_punto && let Ok(i) = NOMBRES.binary_search_by(|(n, _)| (*n).cmp(nombre)) {
        return Some((String::from(NOMBRES[i].1), largo + 1));
    }
    // Sin `;`: el nombre antiguo más largo que sea prefijo (`&ampx` → `&` + `x`).
    let antiguo = SIN_PUNTO_Y_COMA.iter().filter(|n| nombre.starts_with(**n)).max_by_key(|n| n.len())?;
    let despues = s[antiguo.len()..].bytes().next();
    if en_atributo && despues.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'=') {
        return None;
    }
    let i = NOMBRES.binary_search_by(|(n, _)| (*n).cmp(antiguo)).ok()?;
    Some((String::from(NOMBRES[i].1), antiguo.len()))
}

/// `123;` o `x7B;` (tras el `#`): el carácter y los bytes consumidos.
fn numerica(s: &str) -> Option<(char, usize)> {
    let (base, desde) = if s.starts_with(['x', 'X']) { (16, 1) } else { (10, 0) };
    let cifras = s[desde..].chars().take_while(|c| c.is_digit(base)).count();
    if cifras == 0 {
        return None;
    }
    let numero = u32::from_str_radix(&s[desde..desde + cifras.min(8)], base).unwrap_or(u32::MAX);
    let mut n = desde + cifras;
    if s[n..].starts_with(';') {
        n += 1;
    }
    let codigo = match numero {
        0 => 0xFFFD,
        0x80..=0x9F => WINDOWS_1252[(numero - 0x80) as usize],
        x if x > 0x10FFFF => 0xFFFD,
        x => x,
    };
    Some((char::from_u32(codigo).unwrap_or('\u{FFFD}'), n))
}

/// Un texto con sus referencias ya decodificadas.
pub(crate) fn decodificar(s: &str, en_atributo: bool) -> String {
    if !s.contains('&') {
        return String::from(s);
    }
    let mut r = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(p) = s[i..].find('&') {
        r.push_str(&s[i..i + p]);
        i += p + 1;
        match leer(&s[i..], en_atributo) {
            Some((texto, n)) => {
                r.push_str(&texto);
                i += n;
            }
            None => r.push('&'),
        }
    }
    r.push_str(&s[i..]);
    r
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_tabla_esta_ordenada() {
        assert!(NOMBRES.windows(2).all(|w| w[0].0 < w[1].0), "la bisección necesita orden");
    }

    #[test]
    fn referencias() {
        assert_eq!(decodificar("a &amp; b &lt;x&gt; &copy;2026 &hellip;", false), "a & b <x> ©2026 …");
        assert_eq!(decodificar("&#233;&#xE9;&#X1F600;&#150;&#0;", false), "éé😀–\u{FFFD}");
        assert_eq!(decodificar("&amp &ampx &noexiste; & solo", false), "& &x &noexiste; & solo");
        assert_eq!(decodificar("?a=1&copy=2", true), "?a=1&copy=2", "en atributos, &copy= no se toca");
    }
}
