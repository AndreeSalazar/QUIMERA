//! El cliente contra un servidor HTTP de verdad en 127.0.0.1 (dentro de la prueba, sin internet).

use quimera_red::{Cliente, Error, Url};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Arranca un servidor que responde según la ruta; devuelve su URL base y las peticiones vistas.
fn servidor() -> (String, Arc<Mutex<Vec<String>>>) {
    let escucha = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", escucha.local_addr().unwrap().port());
    let vistas = Arc::new(Mutex::new(Vec::new()));
    let registro = vistas.clone();
    std::thread::spawn(move || {
        for conexion in escucha.incoming().flatten() {
            let mut c = conexion;
            let mut buf = [0u8; 4096];
            let n = c.read(&mut buf).unwrap_or(0);
            let peticion = String::from_utf8_lossy(&buf[..n]).into_owned();
            let ruta = peticion.split_whitespace().nth(1).unwrap_or("/").to_string();
            registro.lock().unwrap().push(peticion);
            let respuesta: Vec<u8> = match ruta.as_str() {
                "/" => b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n<p>Hola\r\nA\r\n, QUIMERA!\r\n0\r\n\r\n".to_vec(),
                "/r" => b"HTTP/1.1 302 Found\r\nLocation: /dir/destino?x=1\r\nContent-Length: 0\r\n\r\n".to_vec(),
                "/dir/destino?x=1" => b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\ndestino".to_vec(),
                "/bucle" => b"HTTP/1.1 301 Moved\r\nLocation: /bucle\r\n\r\n".to_vec(),
                "/latin1" => b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=iso-8859-1\r\n\r\nma\xF1ana".to_vec(),
                "/a%20b" => b"HTTP/1.1 200 OK\r\n\r\nespacio codificado".to_vec(),
                _ => b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\nno".to_vec(),
            };
            let _ = c.write_all(&respuesta);
        }
    });
    (base, vistas)
}

fn pedir(base: &str, ruta: &str) -> Result<quimera_red::Respuesta, Error> {
    Cliente::nuevo().pedir(&Url::analizar(&format!("{base}{ruta}")).unwrap())
}

#[test]
fn trozos_cabeceras_y_peticion() {
    let (base, vistas) = servidor();
    let r = pedir(&base, "/").unwrap();
    assert_eq!((r.estado, r.texto().as_str()), (200, "<p>Hola, QUIMERA!"));
    assert_eq!(r.cabecera("Content-Type"), Some("text/html; charset=utf-8"));
    let peticion = vistas.lock().unwrap()[0].clone();
    assert!(peticion.starts_with("GET / HTTP/1.1\r\n"), "{peticion}");
    assert!(
        peticion.contains(&format!("Host: {}\r\n", &base[7..])) && peticion.contains("Connection: close"),
        "{peticion}"
    );
    assert!(peticion.contains("User-Agent: QUIMERA/"), "{peticion}");
}

#[test]
fn redirecciones_y_errores() {
    let (base, _) = servidor();
    let r = pedir(&base, "/r").unwrap();
    assert_eq!(
        (r.texto().as_str(), r.url.to_string()),
        ("destino", format!("{base}/dir/destino?x=1")),
        "la URL final es la base de lo relativo"
    );
    assert!(matches!(pedir(&base, "/bucle"), Err(Error::Http(m)) if m.contains("redirecciones")));
    let r = pedir(&base, "/no-existe").unwrap();
    assert!(!r.exito() && r.estado == 404);
    assert_eq!(pedir(&base, "/latin1").unwrap().texto(), "mañana");
    assert_eq!(pedir(&base, "/a b").unwrap().texto(), "espacio codificado");
    // Un puerto donde nadie escucha.
    let cerrado = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    assert!(matches!(pedir(&format!("http://127.0.0.1:{cerrado}"), "/"), Err(Error::Red(_))));
}
