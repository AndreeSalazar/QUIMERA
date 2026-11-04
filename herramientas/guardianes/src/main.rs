//! Guardianes de QUIMERA.
//!
//! Vigilan que el proyecto no se corrompa ni sorprenda: un semáforo por crate (carril) con
//! hallazgos rojos (bloquean), amarillos (obligan a revisar y aceptar en `guardianes.toml`)
//! y verdes. El objetivo es no perder nunca la aguja en el pajar: archivos de como mucho 1000
//! líneas, jerarquía por niveles, `unsafe` contado y explicado, nada pendiente escondido.
//!
//! Uso: `cargo run -p quimera-guardianes -- [--estricto] [--reglas] [RAIZ]`
//! - `--estricto`: un amarillo sin revisar cuenta como rojo (lo usa el hook de commit).
//! - `--reglas`: explica cada regla y por qué existe.
//! - `--sarif ARCHIVO`: además, escribe los hallazgos en SARIF 2.1.0 (para SUPER GRAFO, editores...).
//!
//! Sale con 0 si pasa, 1 si hay rojos y 2 si no puede leer el proyecto.

mod lexico;
mod proyecto;
mod reglas;
mod sarif;
mod semaforo;
mod toml;

use semaforo::{Color, Hallazgo, REGLAS};
use std::path::PathBuf;
use std::process::ExitCode;

fn explicar_reglas() {
    println!("Reglas de los guardianes de QUIMERA\n");
    for r in REGLAS {
        println!("{} {:<13} {}\n{:>17}{}\n", r.color.circulo(), r.id, r.exige, "", r.porque);
    }
    println!(
        "Un amarillo se acepta en guardianes.toml:\n\n[[revisado]]\nregla = \"A-UNWRAP\"\narchivo = \"motor-js/gc/src/lib.rs\"\nmotivo = \"por qué es correcto aquí\""
    );
}

fn informe(p: &proyecto::Proyecto, hallazgos: &[Hallazgo], estricto: bool) -> Color {
    println!("SEMÁFORO DE QUIMERA{}\n", if estricto { " (estricto)" } else { "" });
    let mut crates: Vec<&proyecto::Crate> = p.crates.iter().collect();
    crates.sort_by_key(|c| (c.meta.as_ref().map_or(-1, |m| m.nivel), c.nombre.clone()));
    for c in crates {
        let suyos: Vec<&Hallazgo> = hallazgos.iter().filter(|h| h.crate_ == c.nombre).collect();
        let color = suyos.iter().map(|h| h.color(estricto)).max().unwrap_or(Color::Verde);
        let (nivel, capa) = c.meta.as_ref().map_or(("?".to_string(), "?"), |m| (m.nivel.to_string(), m.capa.as_str()));
        let mayor = c.archivos.iter().map(|a| a.texto.lines().count()).max().unwrap_or(0);
        let total: usize = c.archivos.iter().map(|a| a.texto.lines().count()).sum();
        let n = c.archivos.len();
        let archivos = format!("{n} archivo{}", if n == 1 { "" } else { "s" });
        println!(
            "{} nivel {nivel:>3}  {capa:<13} {:<22} {archivos}, {total} líneas, el mayor {mayor}/{}",
            color.circulo(),
            c.nombre,
            reglas::MAX_LINEAS
        );
        for h in suyos {
            println!("      {h}");
        }
    }
    let sueltos: Vec<&Hallazgo> = hallazgos.iter().filter(|h| h.crate_.is_empty()).collect();
    if !sueltos.is_empty() {
        println!("\nProyecto:");
        for h in &sueltos {
            println!("      {h}");
        }
    }
    let rojos = hallazgos.iter().filter(|h| h.regla.color == Color::Rojo).count();
    let amarillos = hallazgos.iter().filter(|h| h.regla.color == Color::Amarillo).count();
    let revisados = hallazgos.iter().filter(|h| h.revisado.is_some()).count();
    let pendientes = amarillos - revisados;
    let color = hallazgos.iter().map(|h| h.color(estricto)).max().unwrap_or(Color::Verde);
    println!(
        "\n{rojos} rojos, {amarillos} amarillos ({revisados} revisados, {pendientes} sin revisar) en {} crates",
        p.crates.len()
    );
    let veredicto = match color {
        Color::Rojo if rojos == 0 => {
            "🔴 NO PASA: en modo estricto, un amarillo sin revisar cuenta como rojo. Arreglarlo o aceptarlo en guardianes.toml"
        }
        Color::Rojo => "🔴 NO PASA: arreglar los rojos",
        Color::Amarillo => "🟡 PASA, con amarillos por revisar",
        Color::Verde => "🟢 PASA",
    };
    println!("{veredicto}");
    color
}

fn main() -> ExitCode {
    let mut estricto = false;
    let mut raiz = None;
    let mut sarif = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--estricto" => estricto = true,
            "--sarif" => sarif = args.next().map(PathBuf::from),
            "--reglas" => {
                explicar_reglas();
                return ExitCode::SUCCESS;
            }
            otro => raiz = Some(PathBuf::from(otro)),
        }
    }
    let raiz = raiz.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let p = match proyecto::leer(&raiz) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("guardianes: {e}");
            return ExitCode::from(2);
        }
    };
    let hallazgos = reglas::revisar(&p);
    if let Some(ruta) = sarif {
        let crates: Vec<(String, String)> = p.crates.iter().map(|c| (c.nombre.clone(), c.ruta.clone())).collect();
        if let Err(e) = std::fs::write(&ruta, sarif::documento(&hallazgos, &crates)) {
            eprintln!("guardianes: no se pudo escribir {}: {e}", ruta.display());
            return ExitCode::from(2);
        }
    }
    match informe(&p, &hallazgos, estricto) {
        Color::Rojo => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    }
}

#[cfg(test)]
mod pruebas;
