//! Las fuentes de la página (`@font-face`): se descargan después de la página, en dos hilos, y
//! cada una avisa al llegar; entonces se vuelve a maquetar con ella (hasta que llega, el texto
//! va con la fuente del sistema, como hacen los navegadores con `font-display: swap`).
//!
//! Las direcciones relativas de una fuente se resuelven contra la hoja que la define, no contra
//! la página. Al cambiar de página, las fuentes de la anterior se sueltan.

use crate::pagina::Pagina;
use quimera_estilo::FuenteWeb;
use quimera_red::bloqueo::Tipo;
use quimera_red::{Cliente, Peticion, Url};
use std::sync::{Arc, Mutex};

/// Hilos que descargan fuentes a la vez (suelen ser pocas y pesan más que las imágenes).
const HILOS: usize = 2;

/// Una fuente más grande que esto no se carga.
const MAX_BYTES: usize = 8 << 20;

/// Una fuente que pide la página, con su dirección ya resuelta.
pub struct Pedida {
    pub web: FuenteWeb,
    pub url: Url,
    /// El anfitrión de la página que la pide (para el filtro de anuncios y rastreo).
    pub pagina: String,
}

/// Las fuentes que pide la página, sin repetir dirección.
pub fn pedidas(pagina: &Pagina) -> Vec<Pedida> {
    let doc = pagina.documento();
    let mut v: Vec<Pedida> = Vec::new();
    for web in quimera_estilo::fuentes_web(&doc, &|href| pagina.hoja(href)) {
        let base = match &web.hoja {
            Some(h) => pagina.base.unir(h).unwrap_or_else(|_| pagina.base.clone()),
            None => pagina.base.clone(),
        };
        let Ok(url) = base.unir(web.url.trim()) else { continue };
        if !v.iter().any(|p| p.url == url) {
            v.push(Pedida { web, url, pagina: pagina.url.anfitrion.clone() });
        }
    }
    v
}

/// Descarga una fuente.
pub fn descargar(cliente: &Cliente, p: &Pedida) -> Result<Vec<u8>, String> {
    let peticion = Peticion { tipo: Tipo::Fuente, pagina: Some(p.pagina.clone()), ..Peticion::get() };
    let r = cliente.solicitar(&p.url, &peticion).map_err(|e| e.to_string())?;
    if !r.exito() {
        return Err(format!("el servidor respondió {}", r.estado));
    }
    if r.cuerpo.len() > MAX_BYTES {
        return Err(format!("demasiado grande ({} bytes)", r.cuerpo.len()));
    }
    Ok(r.cuerpo)
}

/// Descarga `lista` en [`HILOS`] hilos; `avisar` recibe cada una al llegar (en su hilo).
pub fn descargar_todas<F>(cliente: Arc<Cliente>, lista: Vec<Pedida>, avisar: F)
where
    F: Fn(Pedida, Result<Vec<u8>, String>) + Send + Sync + 'static,
{
    let cola = Arc::new(Mutex::new(lista.into_iter().rev().collect::<Vec<_>>()));
    let avisar = Arc::new(avisar);
    for _ in 0..HILOS {
        let (cola, cliente, avisar) = (cola.clone(), cliente.clone(), avisar.clone());
        std::thread::spawn(move || {
            loop {
                let siguiente = cola.lock().ok().and_then(|mut c| c.pop());
                let Some(p) = siguiente else { break };
                let r = descargar(&cliente, &p);
                avisar(p, r);
            }
        });
    }
}
