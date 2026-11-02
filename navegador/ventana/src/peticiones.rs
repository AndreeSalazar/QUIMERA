//! Lo que la página pide a la red desde JavaScript (`fetch`, `XMLHttpRequest`, `<script src>`
//! añadidos), con las reglas de origen de los navegadores:
//!
//! - **Una web no lee archivos locales**: desde una página `http(s)` no se puede pedir `file:`.
//! - **Las cookies solo van al mismo origen** (esquema, anfitrión y puerto), salvo
//!   `credentials: 'include'`; con `omit`, nunca.
//! - **CORS**: una petición a otro origen lleva `Origin`, y su respuesta solo llega a la página
//!   si el servidor la permite (`Access-Control-Allow-Origin` es `*` o ese origen; con cookies,
//!   ese origen exacto y `Access-Control-Allow-Credentials: true`). Los `<script src>` no pasan
//!   por CORS (la página no lee su texto: solo se ejecuta), pero a otro origen van sin cookies.
//!
//! Falta: la petición previa (`OPTIONS`) que hacen los navegadores antes de una petición «no
//! simple» a otro origen. Sin ella, esas peticiones salen igual (sin cookies).

use quimera_dom::{PeticionJs, RespuestaJs};
use quimera_red::{Cliente, Peticion, Respuesta, Url};

/// Una petición lista para la red, y lo que hay que comprobar al volver.
pub struct Preparada {
    pub url: Url,
    pub peticion: Peticion,
    /// Si es a otro origen y hay que mirar CORS: el origen de la página.
    pub cors: Option<String>,
}

/// El origen de una URL como lo escribe la cabecera `Origin` (`https://ejemplo.com:8443`).
fn origen(u: &Url) -> String {
    let por_defecto = match u.esquema.as_str() {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    };
    match u.puerto.filter(|p| Some(*p) != por_defecto) {
        Some(p) => format!("{}://{}:{p}", u.esquema, u.anfitrion),
        None => format!("{}://{}", u.esquema, u.anfitrion),
    }
}

/// Resuelve la dirección contra la página y aplica las reglas (ver arriba).
pub fn preparar(pagina: &Url, base: &Url, p: &PeticionJs) -> Result<Preparada, String> {
    let url = base.unir(&p.url).map_err(|e| e.to_string())?;
    let web = matches!(pagina.esquema.as_str(), "http" | "https");
    match url.esquema.as_str() {
        "http" | "https" | "data" => {}
        "file" if !web => {}
        "file" => return Err(String::from("una página web no puede leer archivos locales")),
        e => return Err(format!("QUIMERA no pide direcciones {e}:")),
    }
    let mismo = origen(&url) == origen(pagina);
    let galletas = match p.credenciales.as_str() {
        "omit" => false,
        "include" => true,
        _ => mismo,
    };
    let mut cabeceras = p.cabeceras.clone();
    let cors = (!mismo && url.esquema != "data" && !p.es_script).then(|| origen(pagina));
    if let Some(o) = &cors {
        cabeceras.push((String::from("Origin"), o.clone()));
    }
    let peticion = Peticion {
        metodo: p.metodo.clone(),
        cuerpo: p.cuerpo.as_ref().map(|c| c.as_bytes().to_vec()),
        cabeceras,
        // A otro origen, un script tampoco lleva cookies (aunque los navegadores sí las mandan).
        galletas: galletas && (mismo || !p.es_script),
    };
    Ok(Preparada { url, peticion, cors })
}

/// La respuesta para la página, o por qué no le llega.
pub fn completar(prep: &Preparada, r: Result<Respuesta, quimera_red::Error>) -> Result<RespuestaJs, String> {
    let r = r.map_err(|e| e.to_string())?;
    if let Some(o) = &prep.cors {
        let permitido = r.cabecera("access-control-allow-origin").map(str::trim);
        let con_cookies = prep.peticion.galletas;
        let ok = match permitido {
            Some("*") => !con_cookies,
            Some(x) => x == o && (!con_cookies || r.cabecera("access-control-allow-credentials") == Some("true")),
            None => false,
        };
        if !ok {
            return Err(format!("CORS: {} no permite que {o} lea su respuesta", origen(&prep.url)));
        }
    }
    Ok(RespuestaJs { estado: r.estado, url: r.url.to_string(), cabeceras: r.cabeceras.clone(), cuerpo: r.texto() })
}

/// Atiende una petición entera (para el modo captura, que no espera en otro hilo).
pub fn atender(cliente: &Cliente, pagina: &Url, base: &Url, p: &PeticionJs) -> Result<RespuestaJs, String> {
    let prep = preparar(pagina, base, p)?;
    completar(&prep, cliente.solicitar(&prep.url, &prep.peticion))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn peticion(url: &str, credenciales: &str, es_script: bool) -> PeticionJs {
        PeticionJs {
            id: 1,
            url: String::from(url),
            metodo: String::from("GET"),
            cuerpo: None,
            cabeceras: Vec::new(),
            credenciales: String::from(credenciales),
            es_script,
        }
    }

    #[test]
    fn reglas_de_origen() {
        let pagina = Url::analizar("https://ejemplo.com/a/b").expect("url");
        let p = |u: &str, c: &str, s: bool| preparar(&pagina, &pagina, &peticion(u, c, s));
        let mismo = p("datos.json", "same-origin", false).expect("mismo");
        assert_eq!(mismo.url.to_string(), "https://ejemplo.com/a/datos.json");
        assert!(mismo.peticion.galletas && mismo.cors.is_none());
        let otro = p("https://api.otro.com/x", "same-origin", false).expect("otro");
        assert!(!otro.peticion.galletas);
        assert_eq!(otro.cors.as_deref(), Some("https://ejemplo.com"));
        assert!(otro.peticion.cabeceras.contains(&(String::from("Origin"), String::from("https://ejemplo.com"))));
        assert!(p("https://api.otro.com/x", "include", false).expect("include").peticion.galletas);
        assert!(!p("/x", "omit", false).expect("omit").peticion.galletas);
        let script = p("https://cdn.otro.com/lib.js", "same-origin", true).expect("script");
        assert!(script.cors.is_none() && !script.peticion.galletas);
        assert!(p("file:///C:/secreto.txt", "same-origin", false).is_err(), "una web no lee archivos");
        assert!(p("http://ejemplo.com/a", "same-origin", false).expect("http").cors.is_some(), "otro esquema");
        let local = Url::analizar("file:///C:/p/pagina.html").expect("url");
        assert!(preparar(&local, &local, &peticion("datos.json", "same-origin", false)).is_ok());
    }

    #[test]
    fn cors_en_la_respuesta() {
        let pagina = Url::analizar("https://ejemplo.com/").expect("url");
        let prep = preparar(&pagina, &pagina, &peticion("https://api.otro.com/", "same-origin", false)).expect("p");
        let resp = |acao: Option<&str>| {
            let mut cabeceras = Vec::new();
            if let Some(a) = acao {
                cabeceras.push((String::from("access-control-allow-origin"), String::from(a)));
            }
            Ok(Respuesta { url: prep.url.clone(), estado: 200, cabeceras, cuerpo: b"ok".to_vec() })
        };
        assert!(completar(&prep, resp(None)).is_err());
        assert!(completar(&prep, resp(Some("https://malo.com"))).is_err());
        assert_eq!(completar(&prep, resp(Some("*"))).map(|r| r.cuerpo).as_deref(), Ok("ok"));
        assert!(completar(&prep, resp(Some("https://ejemplo.com"))).is_ok());
    }
}
