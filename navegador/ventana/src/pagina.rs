//! Cargar una página: el HTML y sus hojas `<link rel="stylesheet">`, por red o de disco.
//!
//! Se carga una vez (redimensionar no vuelve a descargar nada; F5 sí) y las direcciones
//! relativas se resuelven contra la URL final, tras las redirecciones, o contra `<base href>`.

use quimera_html::{Documento, analizar};
use quimera_red::{Cliente, Respuesta, Url};
use std::time::Instant;

/// Una página cargada.
pub struct Pagina {
    /// La URL final.
    pub url: Url,
    /// El HTML como texto.
    pub html: String,
    /// (href tal como aparece, CSS) de cada hoja que se pudo cargar.
    pub hojas: Vec<(String, String)>,
    /// Lo que tardó en llegar todo, en milisegundos.
    pub ms_red: f64,
}

/// Lo que se escribe para abrir: una URL (`https://...`, `file:...`, `data:...`) o una ruta local.
pub fn direccion(texto: &str) -> Url {
    match Url::analizar(texto) {
        // `C:\x` también "parece" tener esquema (`c`): una letra sola es una unidad de Windows.
        Ok(u) if u.esquema.len() > 1 => u,
        _ => {
            let ruta =
                std::fs::canonicalize(texto).map(|p| p.display().to_string()).unwrap_or_else(|_| texto.to_string());
            Url::de_archivo(ruta.trim_start_matches(r"\\?\"))
        }
    }
}

/// Una página de error, con el estilo de QUIMERA.
fn error(url: &Url, motivo: &str) -> String {
    let escapar = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;");
    format!(
        "<title>No se pudo abrir</title><style>body{{font-family:sans-serif;margin:48px;color:#333}}\
         h1{{color:#6b2d8f}} code{{background:#f3f0f7;padding:2px 6px}}</style>\
         <h1>No se pudo abrir la página</h1><p><code>{}</code></p><p>{}</p>",
        escapar(&url.to_string()),
        escapar(motivo)
    )
}

impl Pagina {
    /// Descarga (o lee) la página y sus hojas de estilo.
    pub fn cargar(cliente: &Cliente, url: &Url) -> Pagina {
        let inicio = Instant::now();
        let (url_final, html) = match cliente.pedir(url) {
            Ok(r) if r.exito() || !r.cuerpo.is_empty() => (r.url.clone(), texto_html(&r)),
            Ok(r) => (r.url.clone(), error(url, &format!("el servidor respondió {}", r.estado))),
            Err(e) => (url.clone(), error(url, &e.to_string())),
        };
        let doc = analizar(&html);
        let base = base_de(&doc, &url_final);
        let mut hojas = Vec::new();
        for id in doc.descendientes(Documento::RAIZ) {
            let Some(e) = doc.elemento(id).filter(|e| e.nombre == "link") else { continue };
            let es_hoja = e
                .atributo("rel")
                .is_some_and(|r| r.split_ascii_whitespace().any(|x| x.eq_ignore_ascii_case("stylesheet")));
            let Some(href) = e.atributo("href").filter(|_| es_hoja) else { continue };
            if let Some(css) = base.unir(href).ok().and_then(|u| cliente.pedir(&u).ok()).filter(Respuesta::exito) {
                hojas.push((String::from(href), css.texto()));
            }
        }
        Pagina { url: url_final, html, hojas, ms_red: inicio.elapsed().as_secs_f64() * 1e3 }
    }

    /// El árbol del documento.
    pub fn documento(&self) -> Documento {
        analizar(&self.html)
    }

    /// El CSS de una hoja por su `href`.
    pub fn hoja(&self, href: &str) -> Option<String> {
        self.hojas.iter().find(|(h, _)| h == href).map(|(_, css)| css.clone())
    }
}

/// El cuerpo como HTML; si es texto plano, dentro de un `<pre>` como hacen los navegadores.
fn texto_html(r: &Respuesta) -> String {
    let tipo = r.cabecera("content-type").unwrap_or("text/html").to_ascii_lowercase();
    let texto = r.texto();
    if tipo.starts_with("text/plain")
        || tipo.starts_with("text/css")
        || tipo.contains("javascript")
        || tipo.contains("json")
    {
        format!("<pre>{}</pre>", texto.replace('&', "&amp;").replace('<', "&lt;"))
    } else {
        texto
    }
}

/// `<base href>` si lo hay (resuelto contra la URL); si no, la propia URL.
fn base_de(doc: &Documento, url: &Url) -> Url {
    doc.buscar("base")
        .and_then(|b| doc.elemento(b))
        .and_then(|e| e.atributo("href"))
        .and_then(|h| url.unir(h).ok())
        .unwrap_or_else(|| url.clone())
}
