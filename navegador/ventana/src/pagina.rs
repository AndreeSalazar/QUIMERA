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
    /// Contra qué se resuelven los enlaces (`<base href>` o la URL final).
    pub base: Url,
    /// El HTML como texto.
    pub html: String,
    /// (href tal como aparece, CSS) de cada hoja que se pudo cargar.
    pub hojas: Vec<(String, String)>,
    /// (src tal como aparece, código) de cada `<script src>` que se pudo cargar.
    pub scripts: Vec<(String, String)>,
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

/// Lo escrito en la barra como dirección: una URL completa, una ruta local, o un nombre de sitio
/// (`example.com/x` → `https://example.com/x`; `localhost:8080` → `http://`). `None` si no lo es
/// (QUIMERA no envía lo escrito a ningún buscador).
pub fn direccion_escrita(texto: &str) -> Option<Url> {
    let t = texto.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains("://") || ["file:", "data:", "about:"].iter().any(|p| t.starts_with(p)) {
        return Url::analizar(t).ok();
    }
    let b = t.as_bytes();
    let unidad = b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/');
    if unidad || t.starts_with('/') || t.starts_with('.') {
        return Some(direccion(t));
    }
    let local = t.starts_with("localhost") || t.starts_with("127.");
    if t.contains(char::is_whitespace) || !(t.contains('.') || local) {
        return None;
    }
    Url::analizar(&format!("{}://{t}", if local { "http" } else { "https" })).ok()
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
    /// Descarga (o lee) la página y sus hojas de estilo. Con `formulario`, la pide por POST con
    /// ese cuerpo (un formulario enviado).
    pub fn cargar(cliente: &Cliente, url: &Url, formulario: Option<&[u8]>) -> Pagina {
        let inicio = Instant::now();
        let (url_final, html) = match cliente.enviar(url, formulario) {
            Ok(r) if r.exito() || !r.cuerpo.is_empty() => {
                (r.url.clone(), crate::peticiones::limpiar_youtube(&r.url, texto_html(&r)))
            }
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
        let scripts = descargar_scripts(cliente, &doc, &base);
        Pagina { url: url_final, base, html, hojas, scripts, ms_red: inicio.elapsed().as_secs_f64() * 1e3 }
    }

    /// El árbol del documento.
    pub fn documento(&self) -> Documento {
        analizar(&self.html)
    }

    /// El código de un `<script src>` por su `src`.
    pub fn script(&self, src: &str) -> Option<String> {
        self.scripts.iter().find(|(s, _)| s == src).map(|(_, c)| c.clone())
    }

    /// El CSS de una hoja por su `href`.
    pub fn hoja(&self, href: &str) -> Option<String> {
        self.hojas.iter().find(|(h, _)| h == href).map(|(_, css)| css.clone())
    }
}

/// Los `<script src>` clásicos de la página (los `type=module` todavía no se ejecutan).
fn descargar_scripts(cliente: &Cliente, doc: &Documento, base: &Url) -> Vec<(String, String)> {
    let mut scripts = Vec::new();
    for id in doc.descendientes(Documento::RAIZ) {
        let Some(e) = doc.elemento(id).filter(|e| e.nombre == "script") else { continue };
        let clasico =
            e.atributo("type").is_none_or(|t| t.trim().is_empty() || t.to_ascii_lowercase().contains("javascript"));
        let Some(src) = e.atributo("src").filter(|_| clasico) else { continue };
        if let Some(r) = base.unir(src).ok().and_then(|u| cliente.pedir(&u).ok()).filter(Respuesta::exito) {
            scripts.push((String::from(src), r.texto()));
        }
    }
    scripts
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

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn lo_escrito_en_la_barra() {
        let d = |t: &str| direccion_escrita(t).map(|u| u.to_string());
        assert_eq!(d("example.com"), Some(String::from("https://example.com/")));
        assert_eq!(d("  info.cern.ch/hypertext  "), Some(String::from("https://info.cern.ch/hypertext")));
        assert_eq!(d("localhost:8080/x"), Some(String::from("http://localhost:8080/x")));
        assert_eq!(d("http://info.cern.ch"), Some(String::from("http://info.cern.ch/")));
        assert_eq!(d("hola mundo"), None, "no se manda a un buscador");
        assert_eq!(d("palabra"), None);
        assert!(d("C:/Windows/win.ini").is_some_and(|u| u.starts_with("file:///C:/")));
    }
}
