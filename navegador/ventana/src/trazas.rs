//! Trazas en vivo para SUPER GRAFO (la Ciudad en 4D, en vivo).
//!
//! Con `QUIMERA_VIVO=archivo`, cada tramo de trabajo escribe una línea JSON al terminar:
//! `{"t":1234.5,"ruta":"web/estilo","us":5400,"desde":"web/dom"}`. `ruta` es la carpeta del
//! crate que trabajó, `us` cuánto (microsegundos), `t` cuándo empezó (ms desde que arrancó
//! QUIMERA) y `desde` de qué parte venía el trabajo. SUPER GRAFO lee el archivo mientras crece
//! e ilumina esas carpetas en su Ciudad, con el tráfico entre ellas.
//!
//! Sin la variable no se hace nada (ni se mide el tiempo). El archivo se vacía al arrancar.

use std::fs::File;
use std::io::{LineWriter, Write};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// El archivo (si se pidió) y cuándo arrancó QUIMERA.
fn salida() -> Option<&'static (Mutex<LineWriter<File>>, Instant)> {
    static SALIDA: OnceLock<Option<(Mutex<LineWriter<File>>, Instant)>> = OnceLock::new();
    SALIDA
        .get_or_init(|| {
            let ruta = std::env::var_os("QUIMERA_VIVO")?;
            match File::create(&ruta) {
                Ok(f) => Some((Mutex::new(LineWriter::new(f)), Instant::now())),
                Err(e) => {
                    eprintln!("QUIMERA_VIVO: no se pudo crear {}: {e}", ruta.to_string_lossy());
                    None
                }
            }
        })
        .as_ref()
}

/// Un tramo de trabajo en marcha: escribe su línea al soltarse.
pub struct Tramo {
    ruta: &'static str,
    desde: Option<&'static str>,
    inicio: Instant,
}

/// Empieza un tramo de `ruta` (la carpeta del crate) que viene de `desde`. `None` si no se
/// piden trazas: no cuesta nada.
pub fn tramo(ruta: &'static str, desde: Option<&'static str>) -> Option<Tramo> {
    salida()?;
    Some(Tramo { ruta, desde, inicio: Instant::now() })
}

/// Un tramo que ya se midió por otro lado (por ejemplo, la red de una página).
pub fn hecho(ruta: &'static str, desde: Option<&'static str>, inicio: Instant, us: u64) {
    if let Some((m, arranque)) = salida() {
        escribir(m, linea(ruta, desde, inicio.saturating_duration_since(*arranque).as_secs_f64() * 1e3, us));
    }
}

fn linea(ruta: &str, desde: Option<&str>, t_ms: f64, us: u64) -> String {
    let desde = desde.map(|d| format!(",\"desde\":\"{d}\"")).unwrap_or_default();
    format!("{{\"t\":{t_ms:.1},\"ruta\":\"{ruta}\",\"us\":{us}{desde}}}\n")
}

fn escribir(m: &Mutex<LineWriter<File>>, l: String) {
    if let Ok(mut w) = m.lock() {
        let _ = w.write_all(l.as_bytes());
    }
}

impl Drop for Tramo {
    fn drop(&mut self) {
        // Lo que no llega a 20 µs (un latido sin nada que hacer) no se escribe.
        let us = self.inicio.elapsed().as_micros() as u64;
        if us >= 20 {
            hecho(self.ruta, self.desde, self.inicio, us);
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn una_linea_por_tramo() {
        assert_eq!(
            linea("web/estilo", Some("web/dom"), 12.34, 5400),
            "{\"t\":12.3,\"ruta\":\"web/estilo\",\"us\":5400,\"desde\":\"web/dom\"}\n"
        );
        assert_eq!(linea("red/red", None, 0.0, 7), "{\"t\":0.0,\"ruta\":\"red/red\",\"us\":7}\n");
    }
}
