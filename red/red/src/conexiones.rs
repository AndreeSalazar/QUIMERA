//! Conexiones abiertas que se reutilizan (*keep-alive*), como hacen los navegadores: abrir una
//! conexión TLS cuesta varias idas y vueltas al servidor, y una página pide decenas de cosas al
//! mismo sitio.
//!
//! Tras una respuesta con final claro (`Content-Length` o `chunked`) en la que el servidor no
//! dijo `Connection: close`, la conexión vuelve a la piscina de su sitio (esquema, anfitrión y
//! puerto). Como mucho [`POR_SITIO`] por sitio, y se tiran si pasan [`CADUCA`] sin usarse.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Conexiones libres por sitio, como mucho.
const POR_SITIO: usize = 6;
/// Tras cuánto sin usarse se tira una conexión (los servidores suelen cerrarlas antes de un minuto).
const CADUCA: Duration = Duration::from_secs(20);

/// Una conexión abierta: TCP, o TLS encima.
pub(crate) enum Flujo {
    Plano(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Flujo {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Flujo::Plano(s) => s.read(b),
            Flujo::Tls(s) => s.read(b),
        }
    }
}

impl Write for Flujo {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Flujo::Plano(s) => s.write(b),
            Flujo::Tls(s) => s.write(b),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Flujo::Plano(s) => s.flush(),
            Flujo::Tls(s) => s.flush(),
        }
    }
}

/// Las conexiones libres.
#[derive(Default)]
pub(crate) struct Piscina {
    libres: Mutex<HashMap<String, Vec<(Flujo, Instant)>>>,
}

impl Piscina {
    /// Una conexión libre al sitio `clave` (la usada más recientemente), si hay.
    pub(crate) fn tomar(&self, clave: &str) -> Option<Flujo> {
        let mut libres = self.libres.lock().ok()?;
        let lista = libres.get_mut(clave)?;
        lista.retain(|(_, cuando)| cuando.elapsed() < CADUCA);
        lista.pop().map(|(f, _)| f)
    }

    /// Devuelve una conexión que sigue abierta a la piscina de su sitio.
    pub(crate) fn devolver(&self, clave: String, f: Flujo) {
        let Ok(mut libres) = self.libres.lock() else { return };
        let lista = libres.entry(clave).or_default();
        if lista.len() >= POR_SITIO {
            lista.remove(0);
        }
        lista.push((f, Instant::now()));
    }
}
