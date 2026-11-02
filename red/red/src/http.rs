//! HTTP/1.1: la petición que se envía y cómo se lee la respuesta.
//!
//! Se pide con `Connection: close` y se lee hasta que el servidor cierra: sin conexiones
//! reutilizadas todavía, pero sencillo y robusto. El cuerpo se entrega según manda la
//! respuesta: por trozos (`Transfer-Encoding: chunked`), con `Content-Length`, o hasta el
//! cierre.

use crate::Error;
use crate::url::Url;

/// La petición HTTP para `url` según `p` (método, cuerpo, cabeceras). Con `galletas`, la
/// cabecera `Cookie`.
pub(crate) fn peticion(url: &Url, agente: &str, galletas: Option<&str>, p: &crate::Peticion) -> Vec<u8> {
    let mut anfitrion = url.anfitrion.clone();
    let por_defecto = if url.esquema == "https" { 443 } else { 80 };
    if let Some(puerto) = url.puerto.filter(|x| *x != por_defecto) {
        anfitrion.push_str(&format!(":{puerto}"));
    }
    let metodo = p.metodo.as_str();
    let mut s = format!(
        "{metodo} {} HTTP/1.1\r\nHost: {anfitrion}\r\nUser-Agent: {agente}\r\nAccept: text/html,application/xhtml+xml,text/css,*/*;q=0.8\r\nAccept-Language: es,en;q=0.8\r\nAccept-Encoding: identity\r\nConnection: close\r\n",
        url.ruta_y_consulta()
    );
    if let Some(g) = galletas {
        s.push_str(&format!("Cookie: {g}\r\n"));
    }
    // Cabeceras de la página, sin saltos de línea (no pueden colar otra cabecera) y sin las que
    // pone la red (`Host`, `Cookie`, `Content-Length`...).
    for (k, v) in &p.cabeceras {
        let prohibida = matches!(
            k.to_ascii_lowercase().as_str(),
            "host"
                | "cookie"
                | "content-length"
                | "connection"
                | "accept-encoding"
                | "user-agent"
                | "transfer-encoding"
        );
        if !prohibida && !k.contains(['\r', '\n', ':']) && !v.contains(['\r', '\n']) {
            s.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    if let Some(cuerpo) = &p.cuerpo {
        if !p.cabeceras.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            s.push_str("Content-Type: application/x-www-form-urlencoded\r\n");
        }
        s.push_str(&format!("Content-Length: {}\r\n", cuerpo.len()));
    }
    s.push_str("\r\n");
    let mut bytes = s.into_bytes();
    bytes.extend_from_slice(p.cuerpo.as_deref().unwrap_or_default());
    bytes
}

/// Una respuesta leída: estado, cabeceras (nombres en minúsculas) y cuerpo.
pub(crate) struct Leida {
    pub estado: u16,
    pub cabeceras: Vec<(String, String)>,
    pub cuerpo: Vec<u8>,
}

fn error(m: impl Into<String>) -> Error {
    Error::Http(m.into())
}

/// Lee una respuesta completa (los `1xx` previos, como `100 Continue`, se saltan).
pub(crate) fn leer(mut datos: &[u8]) -> Result<Leida, Error> {
    loop {
        let fin = datos
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| error("respuesta sin cabeceras completas"))?;
        let cabecera = String::from_utf8_lossy(&datos[..fin]);
        let mut lineas = cabecera.split("\r\n");
        let estado_linea = lineas.next().unwrap_or("");
        let mut partes = estado_linea.split_whitespace();
        let version = partes.next().unwrap_or("");
        if !version.starts_with("HTTP/") {
            return Err(error(format!("no es HTTP: {estado_linea:.60}")));
        }
        let estado: u16 = partes
            .next()
            .and_then(|e| e.parse().ok())
            .ok_or_else(|| error(format!("estado no válido: {estado_linea:.60}")))?;
        let cabeceras: Vec<(String, String)> = lineas
            .filter_map(|l| l.split_once(':'))
            .map(|(n, v)| (n.trim().to_ascii_lowercase(), String::from(v.trim())))
            .collect();
        let resto = &datos[fin + 4..];
        if (100..200).contains(&estado) {
            datos = resto;
            continue;
        }
        let valor = |n: &str| cabeceras.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
        let por_trozos = valor("transfer-encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked"));
        let cuerpo = if por_trozos {
            trozos(resto)?
        } else if let Some(largo) = valor("content-length").and_then(|l| l.parse::<usize>().ok()) {
            resto[..largo.min(resto.len())].to_vec()
        } else {
            resto.to_vec()
        };
        return Ok(Leida { estado, cabeceras, cuerpo });
    }
}

/// `Transfer-Encoding: chunked`: tamaño en hexadecimal, `\r\n`, datos, `\r\n`... hasta un 0.
fn trozos(mut d: &[u8]) -> Result<Vec<u8>, Error> {
    let mut cuerpo = Vec::with_capacity(d.len());
    loop {
        let Some(fin_linea) = d.windows(2).position(|w| w == b"\r\n") else {
            return Ok(cuerpo); // cortado: lo que haya
        };
        let linea = String::from_utf8_lossy(&d[..fin_linea]);
        let tam_hex = linea.split(';').next().unwrap_or("").trim();
        let tam = usize::from_str_radix(tam_hex, 16).map_err(|_| error(format!("trozo no válido: {tam_hex:.20}")))?;
        d = &d[fin_linea + 2..];
        if tam == 0 {
            return Ok(cuerpo);
        }
        let tomar = tam.min(d.len());
        cuerpo.extend_from_slice(&d[..tomar]);
        d = d.get(tomar + 2..).unwrap_or(&[]);
    }
}

/// `Windows-1252` (y `ISO-8859-1`, que los navegadores tratan igual) a texto.
fn windows_1252(b: &[u8]) -> String {
    const ALTOS: [u32; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D,
        0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
        0x0153, 0x9D, 0x017E, 0x0178,
    ];
    b.iter()
        .map(|&x| match x {
            0x80..=0x9F => char::from_u32(ALTOS[(x - 0x80) as usize]).unwrap_or('\u{FFFD}'),
            x => char::from(x),
        })
        .collect()
}

/// El texto de un cuerpo: el `charset` de la cabecera, o el de `<meta charset>`, o UTF-8.
pub(crate) fn texto(cuerpo: &[u8], tipo: Option<&str>) -> String {
    let de_cabecera = tipo.and_then(|t| {
        t.split(';').find_map(|p| p.trim().strip_prefix("charset=").map(|c| c.trim_matches('"').to_ascii_lowercase()))
    });
    let de_meta = || {
        let inicio = String::from_utf8_lossy(&cuerpo[..cuerpo.len().min(1024)]).to_ascii_lowercase();
        let i = inicio.find("charset=")? + 8;
        let c: String = inicio[i..]
            .trim_start_matches(['"', '\''])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        Some(c)
    };
    let cuerpo = cuerpo.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(cuerpo); // BOM de UTF-8
    match de_cabecera.or_else(de_meta).as_deref() {
        Some("iso-8859-1" | "latin1" | "windows-1252" | "us-ascii" | "ascii") => windows_1252(cuerpo),
        _ => String::from_utf8_lossy(cuerpo).into_owned(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn respuestas() {
        let r = leer(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\n\r\nholaXXX").unwrap();
        assert_eq!((r.estado, r.cuerpo.as_slice()), (200, &b"holaX"[..]), "Content-Length manda");
        let r = leer(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 404 Not Found\r\nTransfer-Encoding: chunked\r\n\r\n4;ext=1\r\nWiki\r\n6\r\npedia \r\n0\r\nX-Final: 1\r\n\r\n").unwrap();
        assert_eq!((r.estado, String::from_utf8_lossy(&r.cuerpo).as_ref()), (404, "Wikipedia "));
        assert_eq!(r.cabeceras[0], (String::from("transfer-encoding"), String::from("chunked")));
        assert!(leer(b"no es http\r\n\r\n").is_err());
        assert!(leer(b"HTTP/1.1 200 OK\r\nContent-Length: 3").is_err(), "cabeceras sin terminar");
    }

    #[test]
    fn juegos_de_caracteres() {
        assert_eq!(texto(b"caf\xE9 \x93hola\x94", Some("text/html; charset=ISO-8859-1")), "café “hola”");
        assert_eq!(texto("café".as_bytes(), None), "café");
        assert_eq!(texto(b"<meta charset='windows-1252'>\x80", None), "<meta charset='windows-1252'>€");
        assert_eq!(texto(b"\xEF\xBB\xBFhola", None), "hola");
    }
}
