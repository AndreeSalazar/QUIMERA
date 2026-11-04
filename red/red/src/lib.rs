//! Red de QUIMERA: pedir una URL y recibir su respuesta.
//!
//! - **URLs** propias ([`Url`]): análisis y resolución de relativas (RFC 3986).
//! - **HTTP/1.1** propio (`http.rs`): petición, `chunked`, `Content-Length`, juegos de
//!   caracteres, redirecciones (hasta 10).
//! - **HTTPS** con rustls (TLS 1.2 y 1.3, Rust) y los certificados raíz de Mozilla
//!   (`webpki-roots`): se comprueba el certificado y el nombre del servidor, como un navegador.
//! - `file:` (archivos locales) y `data:` (texto o base64 dentro de la URL).
//!
//! ```no_run
//! let cliente = quimera_red::Cliente::nuevo();
//! let r = cliente.pedir(&quimera_red::Url::analizar("https://example.com/").unwrap()).unwrap();
//! println!("{} {}", r.estado, r.texto());
//! ```

pub mod galletas;
mod http;
mod url;

pub use url::Url;

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

/// Algo salió mal al pedir una URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// La URL no es válida.
    Url(String),
    /// No se pudo conectar o leer (DNS, conexión rechazada, tiempo agotado...).
    Red(String),
    /// La conexión segura falló (certificado no válido, nombre que no coincide...).
    Tls(String),
    /// La respuesta no es HTTP válido.
    Http(String),
    /// Esquema que todavía no se sabe pedir.
    NoSoportado(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Url(m) => write!(f, "URL no válida: {m}"),
            Error::Red(m) => write!(f, "error de red: {m}"),
            Error::Tls(m) => write!(f, "conexión segura fallida: {m}"),
            Error::Http(m) => write!(f, "respuesta HTTP no válida: {m}"),
            Error::NoSoportado(m) => write!(f, "no se sabe abrir: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Una respuesta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Respuesta {
    /// La URL final (tras las redirecciones): la base para resolver las relativas.
    pub url: Url,
    /// El estado HTTP (200 para `file:` y `data:`).
    pub estado: u16,
    /// Cabeceras con el nombre en minúsculas.
    pub cabeceras: Vec<(String, String)>,
    /// El cuerpo tal cual llegó.
    pub cuerpo: Vec<u8>,
}

impl Respuesta {
    /// El valor de una cabecera.
    pub fn cabecera(&self, nombre: &str) -> Option<&str> {
        let n = nombre.to_ascii_lowercase();
        self.cabeceras.iter().find(|(k, _)| *k == n).map(|(_, v)| v.as_str())
    }

    /// Si el estado es 2xx.
    pub fn exito(&self) -> bool {
        (200..300).contains(&self.estado)
    }

    /// El cuerpo como texto (con el `charset` de la cabecera o del `<meta>`; si no, UTF-8).
    pub fn texto(&self) -> String {
        http::texto(&self.cuerpo, self.cabecera("content-type"))
    }
}

/// Máximo de bytes que se aceptan en una respuesta.
const MAXIMO: u64 = 64 << 20;
const REDIRECCIONES: usize = 10;

/// El cliente: la configuración de TLS (se prepara una vez) y cómo se presenta.
pub struct Cliente {
    tls: Arc<rustls::ClientConfig>,
    /// La cabecera `User-Agent`.
    pub agente: String,
    /// Tiempo máximo para conectar y para cada lectura.
    pub espera: Duration,
    /// Las cookies (compartidas por los hilos que piden con este cliente).
    galletas: std::sync::Mutex<galletas::Tarro>,
}

/// Lo que se pide: método, cuerpo, cabeceras extra y si van las cookies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peticion {
    /// `GET`, `POST`...
    pub metodo: String,
    /// El cuerpo (con `Content-Length`; `Content-Type` sale de `cabeceras` o es de formulario).
    pub cuerpo: Option<Vec<u8>>,
    /// Cabeceras extra (las peligrosas, como `Cookie` o `Host`, se ignoran).
    pub cabeceras: Vec<(String, String)>,
    /// Si se mandan las cookies y se guardan las que lleguen. No, en una petición a otro sitio.
    pub galletas: bool,
}

impl Peticion {
    /// Un GET normal, con cookies.
    pub fn get() -> Peticion {
        Peticion { metodo: String::from("GET"), cuerpo: None, cabeceras: Vec::new(), galletas: true }
    }
}

/// Segundos desde 1970 (para la caducidad de las cookies).
fn ahora() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Default for Cliente {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl Cliente {
    /// Un cliente con los certificados raíz de Mozilla.
    pub fn nuevo() -> Cliente {
        let raices = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let proveedor = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(proveedor)
            .with_safe_default_protocol_versions()
            .map(|b| b.with_root_certificates(raices).with_no_client_auth());
        let tls = match config {
            Ok(c) => Arc::new(c),
            // No debería pasar con el proveedor de serie; sin TLS, `https:` dará error al pedir.
            Err(_) => Arc::new(
                rustls::ClientConfig::builder()
                    .with_root_certificates(rustls::RootCertStore::empty())
                    .with_no_client_auth(),
            ),
        };
        Cliente {
            tls,
            agente: format!("QUIMERA/{} (+navegador personal en Rust)", env!("CARGO_PKG_VERSION")),
            espera: Duration::from_secs(15),
            galletas: std::sync::Mutex::new(galletas::Tarro::default()),
        }
    }

    /// Pide una URL siguiendo las redirecciones.
    pub fn pedir(&self, url: &Url) -> Result<Respuesta, Error> {
        self.solicitar(url, &Peticion::get())
    }

    /// Pide una URL, o envía un formulario por POST si hay `formulario` (codificado como
    /// `application/x-www-form-urlencoded`), siguiendo las redirecciones: tras un POST, una 301,
    /// 302 o 303 se pide por GET; una 307 o 308 vuelve a enviar el formulario.
    pub fn enviar(&self, url: &Url, formulario: Option<&[u8]>) -> Result<Respuesta, Error> {
        let p = match formulario {
            Some(f) => Peticion { metodo: String::from("POST"), cuerpo: Some(f.to_vec()), ..Peticion::get() },
            None => Peticion::get(),
        };
        self.solicitar(url, &p)
    }

    /// Hace la petición `p` a `url` siguiendo las redirecciones (tras una 301, 302 o 303 de
    /// algo que no sea GET, se sigue por GET y sin cuerpo; una 307 o 308 la repite igual).
    pub fn solicitar(&self, url: &Url, p: &Peticion) -> Result<Respuesta, Error> {
        let mut actual = url.clone();
        let mut p = p.clone();
        for _ in 0..=REDIRECCIONES {
            let r = self.pedir_una(&actual, &p)?;
            let destino = r.cabecera("location").filter(|_| matches!(r.estado, 301 | 302 | 303 | 307 | 308));
            match destino {
                Some(d) => {
                    actual = actual.unir(d)?;
                    if !matches!(r.estado, 307 | 308) && p.metodo != "HEAD" {
                        p.metodo = String::from("GET");
                        p.cuerpo = None;
                    }
                }
                None => return Ok(r),
            }
        }
        Err(Error::Http(format!("más de {REDIRECCIONES} redirecciones desde {url}")))
    }

    /// Las cookies que duran más que la sesión, como texto (para guardarlas en disco).
    pub fn galletas_texto(&self) -> String {
        self.galletas.lock().map(|t| t.a_texto(ahora())).unwrap_or_default()
    }

    /// Recupera las cookies guardadas con [`Cliente::galletas_texto`].
    pub fn cargar_galletas(&self, texto: &str) {
        if let Ok(mut t) = self.galletas.lock() {
            *t = galletas::Tarro::desde_texto(texto, ahora());
        }
    }

    fn pedir_una(&self, url: &Url, p: &Peticion) -> Result<Respuesta, Error> {
        match url.esquema.as_str() {
            "http" | "https" => {
                let galleta =
                    if p.galletas { self.galletas.lock().ok().and_then(|t| t.cabecera(url, ahora())) } else { None };
                let bruto = self.intercambio(url, galleta.as_deref(), p)?;
                let l = http::leer(&bruto)?;
                // Sin cookies (una petición a otro sitio), tampoco se guardan las que lleguen.
                if p.galletas
                    && let Ok(mut t) = self.galletas.lock()
                {
                    for (_, v) in l.cabeceras.iter().filter(|(k, _)| k == "set-cookie") {
                        t.recibir(url, v, ahora());
                    }
                }
                let mut sin_fragmento = url.clone();
                sin_fragmento.fragmento = None;
                Ok(Respuesta { url: sin_fragmento, estado: l.estado, cabeceras: l.cabeceras, cuerpo: l.cuerpo })
            }
            "file" => {
                let ruta = url.ruta_local().unwrap_or_default();
                let cuerpo = std::fs::read(&ruta).map_err(|e| Error::Red(format!("{ruta}: {e}")))?;
                Ok(Respuesta { url: url.clone(), estado: 200, cabeceras: Vec::new(), cuerpo })
            }
            "data" => datos(url),
            e => Err(Error::NoSoportado(format!("{e}:"))),
        }
    }

    /// Conecta, envía la petición y lee todo hasta que el servidor cierra.
    fn intercambio(&self, url: &Url, galletas: Option<&str>, p: &Peticion) -> Result<Vec<u8>, Error> {
        let anfitrion = url.anfitrion.trim_start_matches('[').trim_end_matches(']');
        let direcciones = (anfitrion, url.puerto_efectivo())
            .to_socket_addrs()
            .map_err(|e| Error::Red(format!("{anfitrion}: {e}")))?;
        let mut ultimo = Error::Red(format!("{anfitrion}: sin direcciones"));
        let mut tcp = None;
        for d in direcciones {
            match TcpStream::connect_timeout(&d, self.espera) {
                Ok(s) => {
                    tcp = Some(s);
                    break;
                }
                Err(e) => ultimo = Error::Red(format!("{d}: {e}")),
            }
        }
        let tcp = tcp.ok_or(ultimo)?;
        tcp.set_read_timeout(Some(self.espera)).map_err(|e| Error::Red(e.to_string()))?;
        tcp.set_write_timeout(Some(self.espera)).map_err(|e| Error::Red(e.to_string()))?;
        let peticion = http::peticion(url, &self.agente, galletas, p);
        if url.esquema == "https" {
            let nombre = rustls::pki_types::ServerName::try_from(anfitrion.to_string())
                .map_err(|e| Error::Tls(e.to_string()))?;
            let conexion =
                rustls::ClientConnection::new(self.tls.clone(), nombre).map_err(|e| Error::Tls(e.to_string()))?;
            let mut flujo = rustls::StreamOwned::new(conexion, tcp);
            enviar_y_leer(&mut flujo, &peticion)
        } else {
            let mut tcp = tcp;
            enviar_y_leer(&mut tcp, &peticion)
        }
    }
}

/// Escribe la petición y lee la respuesta entera (hasta `MAXIMO`).
fn enviar_y_leer<S: Read + Write>(flujo: &mut S, peticion: &[u8]) -> Result<Vec<u8>, Error> {
    flujo.write_all(peticion).map_err(clasificar)?;
    flujo.flush().map_err(clasificar)?;
    let mut bruto = Vec::new();
    let mut trozo = alloc_trozo();
    // Hasta que la respuesta esté entera o el servidor cierre (lo que antes pase).
    while !http::completa(&bruto) && (bruto.len() as u64) < MAXIMO {
        match flujo.read(&mut trozo) {
            Ok(0) => break,
            Ok(n) => bruto.extend_from_slice(&trozo[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            // Muchos servidores cierran sin avisar a TLS («close_notify»): si ya hay respuesta, vale.
            Err(e) if e.kind() == ErrorKind::UnexpectedEof && !bruto.is_empty() => break,
            Err(e) => return Err(clasificar(e)),
        }
    }
    Ok(bruto)
}

/// El búfer de lectura (64 KB, en el montón para no cargar la pila).
fn alloc_trozo() -> Vec<u8> {
    vec![0u8; 64 * 1024]
}

/// Un error de E/S como error de red o de TLS (rustls los envuelve en `InvalidData`).
fn clasificar(e: std::io::Error) -> Error {
    if e.kind() == ErrorKind::InvalidData { Error::Tls(e.to_string()) } else { Error::Red(e.to_string()) }
}

/// `data:[tipo][;base64],datos`.
fn datos(url: &Url) -> Result<Respuesta, Error> {
    let (cabeza, contenido) = url.ruta.split_once(',').ok_or_else(|| Error::Url(String::from("data: sin coma")))?;
    let es_base64 = cabeza.to_ascii_lowercase().ends_with(";base64");
    let tipo = cabeza.trim_end_matches(";base64").trim_end_matches(";BASE64");
    let cuerpo = if es_base64 {
        base64(contenido).ok_or_else(|| Error::Url(String::from("base64 no válido")))?
    } else {
        porcentaje(contenido)
    };
    let tipo = if tipo.is_empty() { "text/plain;charset=US-ASCII" } else { tipo };
    Ok(Respuesta {
        url: url.clone(),
        estado: 200,
        cabeceras: vec![(String::from("content-type"), String::from(tipo))],
        cuerpo,
    })
}

fn porcentaje(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut v = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |k: usize| b.get(k).and_then(|c| (*c as char).to_digit(16));
        if let (b'%', Some(x), Some(y)) = (b[i], hex(i + 1), hex(i + 2)) {
            v.push((x * 16 + y) as u8);
            i += 3;
        } else {
            v.push(b[i]);
            i += 1;
        }
    }
    v
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let valor = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let limpio: Vec<u8> = porcentaje(s).into_iter().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut v = Vec::with_capacity(limpio.len() * 3 / 4);
    for grupo in limpio.chunks(4) {
        let n = grupo.iter().try_fold(0u32, |acc, &c| Some(acc << 6 | valor(c)?))? << (6 * (4 - grupo.len()));
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        v.extend_from_slice(&bytes[..grupo.len().saturating_sub(1)]);
    }
    Some(v)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn urls_data() {
        let c = Cliente::nuevo();
        let r = c.pedir(&Url::analizar("data:text/html,<p>hola%20mundo</p>").unwrap()).unwrap();
        assert_eq!((r.texto().as_str(), r.cabecera("Content-Type")), ("<p>hola mundo</p>", Some("text/html")));
        let r = c.pedir(&Url::analizar("data:text/plain;base64,SG9sYSwgUVVJTUVSQQ==").unwrap()).unwrap();
        assert_eq!(r.texto(), "Hola, QUIMERA");
        assert!(matches!(c.pedir(&Url::analizar("gopher://x/").unwrap()), Err(Error::NoSoportado(_))));
    }
}
