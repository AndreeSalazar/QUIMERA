//! URLs: analizar `https://ejemplo.com:8443/a/b?q#f` y resolver las relativas (`../c.css`)
//! con el algoritmo del RFC 3986 (sección 5.2), el que usan los enlaces y las hojas de estilo.
//!
//! Como en los navegadores (estándar WHATWG), en `http`, `https` y `file` las `\` cuentan
//! como `/`, el esquema y el anfitrión van en minúsculas, y los espacios y caracteres no ASCII
//! se codifican con `%XX` al pedir la página.

use crate::Error;
use std::fmt;

/// Una URL absoluta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// `http`, `https`, `file`, `data`... (en minúsculas).
    pub esquema: String,
    /// El anfitrión (vacío en `file:///` y en las URL opacas como `data:`).
    pub anfitrion: String,
    /// El puerto si se dijo.
    pub puerto: Option<u16>,
    /// La ruta (con `/` inicial en las jerárquicas; todo lo de después de `:` en las opacas).
    pub ruta: String,
    /// Lo de después de `?`.
    pub consulta: Option<String>,
    /// Lo de después de `#`.
    pub fragmento: Option<String>,
}

fn error(m: impl Into<String>) -> Error {
    Error::Url(m.into())
}

fn jerarquico(esquema: &str) -> bool {
    matches!(esquema, "http" | "https" | "file" | "ws" | "wss" | "ftp")
}

impl Url {
    /// Analiza una URL absoluta.
    pub fn analizar(texto: &str) -> Result<Url, Error> {
        let t = texto.trim();
        let Some((esquema, resto)) = t.split_once(':') else { return Err(error(format!("sin esquema: {t}"))) };
        let valido = esquema.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && esquema.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if !valido {
            return Err(error(format!("esquema no válido: {esquema}")));
        }
        let esquema = esquema.to_ascii_lowercase();
        let (resto, fragmento) = partir(resto, '#');
        if !jerarquico(&esquema) {
            return Ok(Url {
                esquema,
                anfitrion: String::new(),
                puerto: None,
                ruta: String::from(resto),
                consulta: None,
                fragmento,
            });
        }
        let resto = resto.replace('\\', "/");
        let (resto, consulta) = partir(&resto, '?');
        let sin_barras = resto.trim_start_matches('/');
        let (autoridad, ruta) = match sin_barras.find('/') {
            Some(i) => (&sin_barras[..i], &sin_barras[i..]),
            None => (sin_barras, "/"),
        };
        let (anfitrion, puerto) = if esquema == "file" { (String::new(), None) } else { autoridad_de(autoridad)? };
        if esquema != "file" && anfitrion.is_empty() {
            return Err(error(format!("sin anfitrión: {t}")));
        }
        // `file:///C:/x` → ruta `/C:/x`; `file://C:/x` también.
        let ruta =
            if esquema == "file" && !autoridad.is_empty() { format!("/{autoridad}{ruta}") } else { String::from(ruta) };
        Ok(Url { esquema, anfitrion, puerto, ruta: quitar_puntos(&ruta), consulta, fragmento })
    }

    /// El puerto que se usa (el dicho o el del esquema).
    pub fn puerto_efectivo(&self) -> u16 {
        self.puerto.unwrap_or(match self.esquema.as_str() {
            "https" | "wss" => 443,
            "ftp" => 21,
            _ => 80,
        })
    }

    /// Ruta y consulta codificadas para la línea de petición (`/a%20b?x=1`).
    pub fn ruta_y_consulta(&self) -> String {
        let mut s = codificar(&self.ruta);
        if s.is_empty() {
            s.push('/');
        }
        if let Some(q) = &self.consulta {
            s.push('?');
            s.push_str(&codificar(q));
        }
        s
    }

    /// Una URL relativa (`../c.css`, `/x`, `//otro.com/y`, `?q`, `#f`) resuelta contra esta.
    pub fn unir(&self, relativa: &str) -> Result<Url, Error> {
        let r = relativa.trim().replace('\\', "/");
        if r.split_once(':').is_some_and(|(e, _)| {
            !e.is_empty()
                && !e.contains(['/', '?', '#'])
                && e.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }) {
            return Url::analizar(&r);
        }
        if !jerarquico(&self.esquema) {
            return Err(error(format!("no se puede resolver {r} contra {self}")));
        }
        if let Some(sin) = r.strip_prefix("//") {
            return Url::analizar(&format!("{}://{sin}", self.esquema));
        }
        let (r, fragmento) = partir(&r, '#');
        let (camino, consulta) = partir(r, '?');
        let mut u = Url { fragmento, ..self.clone() };
        if camino.is_empty() {
            if consulta.is_some() {
                u.consulta = consulta;
            }
            return Ok(u);
        }
        u.consulta = consulta;
        u.ruta = if camino.starts_with('/') {
            quitar_puntos(camino)
        } else {
            // Se sustituye el último segmento de la ruta base.
            let base = &self.ruta[..self.ruta.rfind('/').map_or(0, |i| i + 1)];
            quitar_puntos(&format!("{base}{camino}"))
        };
        Ok(u)
    }

    /// La ruta local de una URL `file:` (`/C:/x/y.html` → `C:/x/y.html` en Windows).
    pub fn ruta_local(&self) -> Option<String> {
        if self.esquema != "file" {
            return None;
        }
        let ruta = decodificar(&self.ruta);
        let b = ruta.as_bytes();
        let unidad = b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':';
        Some(if unidad { ruta[1..].to_string() } else { ruta })
    }

    /// Una URL `file:` para una ruta local.
    pub fn de_archivo(ruta: &str) -> Url {
        let r = ruta.replace('\\', "/");
        let r = if r.starts_with('/') { r } else { format!("/{r}") };
        Url {
            esquema: String::from("file"),
            anfitrion: String::new(),
            puerto: None,
            ruta: quitar_puntos(&r),
            consulta: None,
            fragmento: None,
        }
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !jerarquico(&self.esquema) {
            write!(f, "{}:{}", self.esquema, self.ruta)?;
        } else {
            write!(f, "{}://{}", self.esquema, self.anfitrion)?;
            if let Some(p) = self.puerto {
                write!(f, ":{p}")?;
            }
            write!(f, "{}", self.ruta)?;
            if let Some(q) = &self.consulta {
                write!(f, "?{q}")?;
            }
        }
        if let Some(fr) = &self.fragmento {
            write!(f, "#{fr}")?;
        }
        Ok(())
    }
}

/// `texto` partido por la primera aparición de `c` (lo de después, sin `c`).
fn partir(texto: &str, c: char) -> (&str, Option<String>) {
    match texto.split_once(c) {
        Some((a, b)) => (a, Some(String::from(b))),
        None => (texto, None),
    }
}

/// `usuario:clave@anfitrion:puerto` → (anfitrión en minúsculas, puerto).
fn autoridad_de(a: &str) -> Result<(String, Option<u16>), Error> {
    let a = a.rsplit_once('@').map_or(a, |(_, h)| h);
    let (anfitrion, puerto) = if let Some(resto) = a.strip_prefix('[') {
        let (ip6, tras) = resto.split_once(']').ok_or_else(|| error("IPv6 sin cerrar"))?;
        (format!("[{ip6}]"), tras.strip_prefix(':'))
    } else {
        match a.rsplit_once(':') {
            Some((h, p)) => (String::from(h), Some(p)),
            None => (String::from(a), None),
        }
    };
    let puerto = match puerto.filter(|p| !p.is_empty()) {
        Some(p) => Some(p.parse::<u16>().map_err(|_| error(format!("puerto no válido: {p}")))?),
        None => None,
    };
    Ok((anfitrion.to_ascii_lowercase(), puerto))
}

/// Quita los segmentos `.` y `..` (RFC 3986, 5.2.4).
fn quitar_puntos(ruta: &str) -> String {
    let mut salida: Vec<&str> = Vec::new();
    let segmentos: Vec<&str> = ruta.split('/').collect();
    for (i, s) in segmentos.iter().enumerate() {
        let ultimo = i + 1 == segmentos.len();
        match *s {
            "." | "%2e" | "%2E" => {
                if ultimo {
                    salida.push("");
                }
            }
            ".." | ".%2e" | "%2e." | "%2e%2e" => {
                if salida.len() > 1 {
                    salida.pop();
                }
                if ultimo {
                    salida.push("");
                }
            }
            s => salida.push(s),
        }
    }
    let r = salida.join("/");
    if ruta.starts_with('/') && !r.starts_with('/') { format!("/{r}") } else { r }
}

/// Codifica con `%XX` lo que no puede ir tal cual en una petición (blancos, no ASCII, `"<>`).
fn codificar(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for b in s.bytes() {
        if b <= b' ' || b >= 0x7F || matches!(b, b'"' | b'<' | b'>' | b'`') {
            r.push_str(&format!("%{b:02X}"));
        } else {
            r.push(b as char);
        }
    }
    r
}

/// Decodifica `%XX`.
fn decodificar(s: &str) -> String {
    let b = s.as_bytes();
    let mut v = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |k: usize| b.get(k).and_then(|c| (*c as char).to_digit(16));
        match (b[i], hex(i + 1), hex(i + 2)) {
            (b'%', Some(a), Some(c)) => {
                v.push((a * 16 + c) as u8);
                i += 3;
            }
            (x, _, _) => {
                v.push(x);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&v).into_owned()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn u(s: &str) -> Url {
        Url::analizar(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn analizar() {
        let x = u("HTTPS://Usuario:clave@Ejemplo.COM:8443/a/./b/../c?q=1#arriba");
        assert_eq!(
            (x.esquema.as_str(), x.anfitrion.as_str(), x.puerto, x.ruta.as_str()),
            ("https", "ejemplo.com", Some(8443), "/a/c")
        );
        assert_eq!((x.consulta.as_deref(), x.fragmento.as_deref()), (Some("q=1"), Some("arriba")));
        assert_eq!(x.to_string(), "https://ejemplo.com:8443/a/c?q=1#arriba");
        assert_eq!(u("http://ejemplo.com").ruta, "/");
        assert_eq!(u("http://[::1]:8080/x").anfitrion, "[::1]");
        assert_eq!(u("http:\\\\ejemplo.com\\a\\b").ruta, "/a/b");
        assert_eq!(u("http://e.com/a b/ñ").ruta_y_consulta(), "/a%20b/%C3%B1");
        assert_eq!(u("data:text/html,<p>hola").ruta, "text/html,<p>hola");
        assert!(Url::analizar("sin-esquema.com").is_err() && Url::analizar("http://:80/").is_err());
        assert!(Url::analizar("http://e.com:99999/").is_err());
    }

    #[test]
    fn resolver_relativas_como_el_rfc_3986() {
        // Los ejemplos normales de la sección 5.4.1 del RFC 3986.
        let base = u("http://a/b/c/d;p?q");
        for (rel, esperada) in [
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g/"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            (";x", "http://a/b/c/;x"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../g", "http://a/g"),
            ("../../../g", "http://a/g"),
            ("https://otro.com/x", "https://otro.com/x"),
        ] {
            assert_eq!(base.unir(rel).map(|x| x.to_string()).unwrap_or_default(), esperada, "{rel:?}");
        }
    }

    #[test]
    fn archivos_locales() {
        let f = u("file:///C:/Users/Yo/mi%20web/index.html");
        assert_eq!(f.ruta_local().as_deref(), Some("C:/Users/Yo/mi web/index.html"));
        assert_eq!(
            f.unir("css/estilo.css").unwrap().ruta_local().as_deref(),
            Some("C:/Users/Yo/mi web/css/estilo.css")
        );
        assert_eq!(Url::de_archivo("C:\\x\\y.html").to_string(), "file:///C:/x/y.html");
        assert_eq!(u("file:///home/yo/a.html").ruta_local().as_deref(), Some("/home/yo/a.html"));
    }
}
