//! Cookies (RFC 6265): las que manda cada respuesta (`Set-Cookie`) y las que se devuelven en
//! cada petición (`Cookie`).
//!
//! - **Dominio**: sin `Domain`, la cookie es solo de ese anfitrión. Con `Domain`, también de sus
//!   subdominios, pero solo si el anfitrión es ese dominio o uno de sus subdominios (una web no
//!   puede poner cookies a otra) y el dominio tiene algún punto (no `com` a secas).
//! - **Ruta**: sin `Path`, la carpeta de la dirección. Se manda a esa ruta y a lo que cuelga de ella.
//! - **`Secure`**: solo por HTTPS, y solo una respuesta HTTPS puede ponerla.
//! - **Caducidad**: `Max-Age` (gana) o `Expires`. Sin ninguna, dura lo que la sesión: no se
//!   guarda en disco.

use crate::url::Url;

/// Una cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Galleta {
    /// El nombre.
    pub nombre: String,
    /// El valor (sin comillas).
    pub valor: String,
    /// En minúsculas y sin punto inicial.
    pub dominio: String,
    /// Sin `Domain`: solo para ese anfitrión exacto.
    pub solo_anfitrion: bool,
    /// La ruta a partir de la que se manda.
    pub ruta: String,
    /// `Secure`: solo por HTTPS.
    pub segura: bool,
    /// Segundos desde 1970 en que caduca; `None`: de sesión.
    pub caduca: Option<u64>,
}

/// Todas las cookies.
#[derive(Debug, Clone, Default)]
pub struct Tarro {
    galletas: Vec<Galleta>,
}

/// Si `anfitrion` cae bajo `dominio` (es él o un subdominio).
fn bajo_dominio(anfitrion: &str, dominio: &str) -> bool {
    anfitrion == dominio || anfitrion.strip_suffix(dominio).is_some_and(|r| r.ends_with('.'))
}

/// La ruta por defecto: la carpeta de la dirección (`/a/b/c` → `/a/b`).
fn ruta_por_defecto(ruta: &str) -> String {
    match ruta.rfind('/') {
        Some(0) | None => String::from("/"),
        Some(i) => String::from(&ruta[..i]),
    }
}

/// Si una petición a `ruta` lleva una cookie de `de_cookie`.
fn bajo_ruta(ruta: &str, de_cookie: &str) -> bool {
    ruta == de_cookie || ruta.strip_prefix(de_cookie).is_some_and(|r| de_cookie.ends_with('/') || r.starts_with('/'))
}

impl Tarro {
    /// Guarda lo que manda una cabecera `Set-Cookie` de una respuesta de `url`.
    pub fn recibir(&mut self, url: &Url, cabecera: &str, ahora: u64) {
        let mut partes = cabecera.split(';');
        let Some((nombre, valor)) = partes.next().and_then(|p| p.split_once('=')) else { return };
        let (nombre, valor) = (nombre.trim(), valor.trim().trim_matches('"'));
        if nombre.is_empty() {
            return;
        }
        let anfitrion = url.anfitrion.to_ascii_lowercase();
        let mut g = Galleta {
            nombre: String::from(nombre),
            valor: String::from(valor),
            dominio: anfitrion.clone(),
            solo_anfitrion: true,
            ruta: ruta_por_defecto(&url.ruta),
            segura: false,
            caduca: None,
        };
        let mut max_age: Option<i64> = None;
        for atributo in partes {
            let (clave, v) = atributo.split_once('=').map_or((atributo.trim(), ""), |(k, v)| (k.trim(), v.trim()));
            match clave.to_ascii_lowercase().as_str() {
                "domain" => {
                    let d = v.trim_start_matches('.').to_ascii_lowercase();
                    if !d.is_empty() && bajo_dominio(&anfitrion, &d) && (d.contains('.') || d == anfitrion) {
                        (g.dominio, g.solo_anfitrion) = (d, false);
                    } else if !d.is_empty() {
                        return; // para otro dominio: se rechaza entera
                    }
                }
                "path" if v.starts_with('/') => g.ruta = String::from(v),
                "secure" => g.segura = true,
                "max-age" => max_age = v.parse().ok(),
                "expires" if max_age.is_none() => g.caduca = fecha_http(v),
                _ => {}
            }
        }
        if let Some(s) = max_age {
            g.caduca = Some(if s <= 0 { 0 } else { ahora.saturating_add(s as u64) });
        }
        if g.segura && url.esquema != "https" {
            return;
        }
        self.galletas.retain(|x| !(x.nombre == g.nombre && x.dominio == g.dominio && x.ruta == g.ruta));
        if g.caduca.is_none_or(|c| c > ahora) {
            self.galletas.push(g);
        }
    }

    /// La cabecera `Cookie` para una petición a `url` (`None` si no hay ninguna).
    pub fn cabecera(&self, url: &Url, ahora: u64) -> Option<String> {
        let anfitrion = url.anfitrion.to_ascii_lowercase();
        let ruta = if url.ruta.is_empty() { "/" } else { url.ruta.as_str() };
        let mut validas: Vec<&Galleta> = self
            .galletas
            .iter()
            .filter(|g| g.caduca.is_none_or(|c| c > ahora))
            .filter(|g| if g.solo_anfitrion { anfitrion == g.dominio } else { bajo_dominio(&anfitrion, &g.dominio) })
            .filter(|g| bajo_ruta(ruta, &g.ruta))
            .filter(|g| !g.segura || url.esquema == "https")
            .collect();
        // Las de ruta más larga primero, como manda la RFC.
        validas.sort_by_key(|g| core::cmp::Reverse(g.ruta.len()));
        let texto: Vec<String> = validas.iter().map(|g| format!("{}={}", g.nombre, g.valor)).collect();
        (!texto.is_empty()).then(|| texto.join("; "))
    }

    /// Las cookies que duran más que la sesión, una por línea (para guardarlas en disco).
    pub fn a_texto(&self, ahora: u64) -> String {
        let mut s = String::new();
        for g in self.galletas.iter().filter(|g| g.caduca.is_some_and(|c| c > ahora)) {
            let campos = [
                g.nombre.as_str(),
                g.valor.as_str(),
                g.dominio.as_str(),
                if g.solo_anfitrion { "1" } else { "0" },
                g.ruta.as_str(),
                if g.segura { "1" } else { "0" },
            ];
            s.push_str(&format!("{}\t{}\n", campos.join("\t"), g.caduca.unwrap_or(0)));
        }
        s
    }

    /// Las cookies guardadas con [`Tarro::a_texto`] (se saltan las líneas rotas y las caducadas).
    pub fn desde_texto(texto: &str, ahora: u64) -> Tarro {
        let galletas = texto
            .lines()
            .filter_map(|l| {
                let c: Vec<&str> = l.split('\t').collect();
                let [nombre, valor, dominio, solo, ruta, segura, caduca] = c.as_slice() else { return None };
                let caduca: u64 = caduca.parse().ok()?;
                (caduca > ahora).then(|| Galleta {
                    nombre: String::from(*nombre),
                    valor: String::from(*valor),
                    dominio: String::from(*dominio),
                    solo_anfitrion: *solo == "1",
                    ruta: String::from(*ruta),
                    segura: *segura == "1",
                    caduca: Some(caduca),
                })
            })
            .collect();
        Tarro { galletas }
    }

    /// Cuántas hay.
    pub fn total(&self) -> usize {
        self.galletas.len()
    }
}

/// Una fecha HTTP (`Wed, 21 Oct 2015 07:28:00 GMT`, también `21-Oct-15`) en segundos desde 1970.
pub fn fecha_http(texto: &str) -> Option<u64> {
    const MESES: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let (mut dia, mut mes, mut anio, mut hora) = (None, None, None, None);
    for trozo in texto.split([' ', ',', '-']).filter(|t| !t.is_empty()) {
        let t = trozo.to_ascii_lowercase();
        if let Some(m) = MESES.iter().position(|m| t.starts_with(m)) {
            mes = Some(m as u64 + 1);
        } else if t.contains(':') {
            let n: Vec<u64> = t.split(':').filter_map(|x| x.parse().ok()).collect();
            hora = (n.len() == 3).then(|| n[0] * 3600 + n[1] * 60 + n[2]);
        } else if let Ok(n) = t.parse::<u64>() {
            match (dia, t.len()) {
                (None, 1 | 2) => dia = Some(n),
                (_, 4) => anio = Some(n),
                (Some(_), 2) => anio = Some(if n < 70 { 2000 + n } else { 1900 + n }),
                _ => {}
            }
        }
    }
    let (d, m, a) = (dia?, mes?, anio?);
    // Días desde 1970 (el algoritmo «days from civil» de Howard Hinnant).
    let (a, m) = if m <= 2 { (a - 1, m + 9) } else { (a, m - 3) };
    let era = a / 400;
    let dia_del_anio = (153 * m + 2) / 5 + d - 1;
    let dia_de_la_era = (a - era * 400) * 365 + (a - era * 400) / 4 - (a - era * 400) / 100 + dia_del_anio;
    let dias = (era * 146_097 + dia_de_la_era).checked_sub(719_468)?;
    Some(dias * 86_400 + hora.unwrap_or(0))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn url(s: &str) -> Url {
        Url::analizar(s).expect("url")
    }

    #[test]
    fn dominio_ruta_y_secure() {
        let mut t = Tarro::default();
        let ahora = 1_000;
        t.recibir(&url("https://www.ejemplo.com/a/b"), "sesion=1", ahora);
        t.recibir(&url("https://www.ejemplo.com/"), "todos=2; Domain=.ejemplo.com; Path=/", ahora);
        t.recibir(&url("https://www.ejemplo.com/"), "s=3; Secure; Path=/", ahora);
        t.recibir(&url("http://www.ejemplo.com/"), "insegura=4; Secure", ahora);
        t.recibir(&url("https://www.ejemplo.com/"), "ajena=5; Domain=otro.com", ahora);
        t.recibir(&url("https://www.ejemplo.com/"), "tld=6; Domain=com", ahora);
        assert_eq!(t.cabecera(&url("https://www.ejemplo.com/a/c"), ahora).as_deref(), Some("sesion=1; todos=2; s=3"));
        assert_eq!(
            t.cabecera(&url("http://cdn.ejemplo.com/x"), ahora).as_deref(),
            Some("todos=2"),
            "sin Secure por HTTP"
        );
        assert_eq!(
            t.cabecera(&url("https://www.ejemplo.com/ab"), ahora).as_deref(),
            Some("todos=2; s=3"),
            "/a no es /ab"
        );
        assert_eq!(t.cabecera(&url("https://otro.com/"), ahora), None);
    }

    #[test]
    fn caducidad_y_disco() {
        let mut t = Tarro::default();
        let ahora = fecha_http("Wed, 21 Oct 2015 07:28:00 GMT").expect("fecha");
        assert_eq!(ahora, 1_445_412_480);
        assert_eq!(fecha_http("Thu, 22-Oct-15 07:28:00 GMT"), Some(ahora + 86_400));
        let u = url("https://ejemplo.com/");
        t.recibir(&u, "larga=1; Max-Age=3600", ahora);
        t.recibir(&u, "fecha=2; Expires=Thu, 22 Oct 2015 07:28:00 GMT", ahora);
        t.recibir(&u, "sesion=3", ahora);
        t.recibir(&u, "vieja=4; Max-Age=0", ahora);
        assert_eq!(t.total(), 3);
        let guardado = t.a_texto(ahora);
        assert_eq!(guardado.lines().count(), 2, "las de sesión no se guardan");
        let otra = Tarro::desde_texto(&guardado, ahora + 7200);
        assert_eq!(otra.cabecera(&u, ahora + 7200).as_deref(), Some("fecha=2"), "la de 1 hora ya caducó");
        t.recibir(&u, "larga=borrar; Max-Age=0", ahora);
        assert_eq!(t.cabecera(&u, ahora).as_deref(), Some("fecha=2; sesion=3"));
    }
}
