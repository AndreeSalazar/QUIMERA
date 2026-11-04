//! Los *scriptlets* (`sitio##+js(nombre, args)`): un poco de JavaScript que corre antes que los
//! scripts de la página y le quita lo que pone anuncios. Es como los de uBlock Origin y Brave,
//! escritos de nuevo en `scriptlets.js`:
//!
//! | Nombre (y alias) | Qué hace |
//! |---|---|
//! | `set-constant` (`set`) | fija `a.b.c` a un valor permitido (`undefined`, `true`, `noopFunc`, un número...) |
//! | `json-prune` | quita rutas (`playerAds`, `a.[].b`) de lo que lee `JSON.parse` y `Response.json` |
//! | `abort-on-property-read` (`aopr`), `abort-on-property-write` (`aopw`) | un error al leer o escribir esa propiedad |
//! | `abort-current-script` (`acs`) | un error al leerla desde un script que contiene un texto |
//! | `addEventListener-defuser` (`aeld`) | no registra los manejadores de ese tipo con ese texto |
//! | `no-fetch-if`, `no-xhr-if` | esas peticiones responden vacío sin salir |
//! | `no-setTimeout-if` (`nostif`), `no-setInterval-if` (`nosiif`) | no programa esos temporizadores |
//! | `set-local-storage-item`, `remove-cookie` | lo que dicen |
//!
//! Los que no se conocen no hacen nada. La biblioteca se manda una vez por página, solo si
//! algún *scriptlet* vale para ella.

/// La biblioteca de JavaScript.
const BIBLIOTECA: &str = include_str!("scriptlets.js");

/// Parte `nombre, a, b` en sus argumentos: comas sin `\`, sin blancos alrededor y sin las
/// comillas que los rodean.
pub fn argumentos(texto: &str) -> Vec<String> {
    let mut v = vec![String::new()];
    let mut escapado = false;
    for c in texto.chars() {
        match c {
            '\\' if !escapado => escapado = true,
            ',' if !escapado => v.push(String::new()),
            c => {
                if escapado
                    && c != ','
                    && let Some(u) = v.last_mut()
                {
                    u.push('\\');
                }
                escapado = false;
                if let Some(u) = v.last_mut() {
                    u.push(c);
                }
            }
        }
    }
    v.into_iter()
        .map(|a| {
            let a = a.trim();
            let sin = ['"', '\''].iter().find_map(|q| a.strip_prefix(*q).and_then(|r| r.strip_suffix(*q)));
            String::from(sin.unwrap_or(a))
        })
        .collect()
}

/// Una cadena de JavaScript entre comillas simples (sin `</script>` posible).
fn cadena_js(s: &str) -> String {
    let mut r = String::from("'");
    for c in s.chars() {
        match c {
            '\\' => r.push_str("\\\\"),
            '\'' => r.push_str("\\'"),
            '\n' => r.push_str("\\n"),
            '\r' => r.push_str("\\r"),
            '<' => r.push_str("\\x3c"),
            '\u{2028}' => r.push_str("\\u2028"),
            '\u{2029}' => r.push_str("\\u2029"),
            c => r.push(c),
        }
    }
    r.push('\'');
    r
}

/// El código que pone estos *scriptlets* (`nombre, args` cada uno), o `None` si no hay.
pub fn guion(llamadas: &[&str]) -> Option<String> {
    if llamadas.is_empty() {
        return None;
    }
    let mut js = String::from("(function () {\n");
    js.push_str(BIBLIOTECA);
    for l in llamadas {
        let mut a = argumentos(l);
        if a.first().is_none_or(String::is_empty) {
            continue;
        }
        let nombre = a.remove(0);
        let args: Vec<String> = a.iter().map(|x| cadena_js(x)).collect();
        js.push_str(&format!("q({}, [{}]);\n", cadena_js(&nombre), args.join(", ")));
    }
    js.push_str("})();\n");
    Some(js)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn argumentos_como_ublock() {
        assert_eq!(argumentos("set, a.b, undefined"), ["set", "a.b", "undefined"]);
        assert_eq!(argumentos(r"json-prune, a b, 'x\, y'"), ["json-prune", "a b", "x, y"]);
        assert_eq!(argumentos(r"aeld, click, /a\.b/"), ["aeld", "click", r"/a\.b/"]);
    }

    #[test]
    fn el_guion() {
        assert!(guion(&[]).is_none());
        let js = guion(&["set, ytInitialPlayerResponse.adPlacements, undefined", "nostif, </script>"]).expect("js");
        assert!(js.contains("q('set', ['ytInitialPlayerResponse.adPlacements', 'undefined']);"));
        assert!(js.contains(r"q('nostif', ['\x3c/script>']);"), "sin cerrar el <script>");
        assert!(!js.contains("</script>"));
    }
}
