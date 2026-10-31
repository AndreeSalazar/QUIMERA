//! Lee el workspace: cada crate con su metadata de QUIMERA, sus dependencias y sus archivos.

use crate::toml::{self, Tabla};
use std::fs;
use std::path::Path;

/// Lo que un crate declara en `[package.metadata.quimera]`.
#[derive(Debug, Clone, Default)]
pub struct Meta {
    /// Nivel en la jerarquía (0 = cimientos).
    pub nivel: i64,
    /// Capa: debe coincidir con la primera carpeta de su ruta.
    pub capa: String,
    /// Promete no usar `std`.
    pub no_std: bool,
    /// Crates de fuera permitidos.
    pub externos: Vec<String>,
    /// Puede usar `unsafe` (siempre con `// SAFETY:`).
    pub permite_unsafe: bool,
}

/// Un archivo de Rust del crate.
#[derive(Debug, Clone)]
pub struct Archivo {
    /// Ruta relativa a la raíz del proyecto, con `/`.
    pub ruta: String,
    /// Contenido.
    pub texto: String,
}

/// Un crate del workspace.
#[derive(Debug, Clone)]
pub struct Crate {
    /// Nombre (`quimera-gc`).
    pub nombre: String,
    /// Carpeta relativa a la raíz (`motor-js/gc`).
    pub ruta: String,
    /// `None` si falta `[package.metadata.quimera]`.
    pub meta: Option<Meta>,
    /// Nombres de las dependencias (normales y de compilación).
    pub dependencias: Vec<String>,
    /// Archivos `.rs` (src, tests, examples, benches).
    pub archivos: Vec<Archivo>,
}

/// Una revisión aceptada en `guardianes.toml`.
#[derive(Debug, Clone)]
pub struct Revision {
    /// Regla revisada (`A-UNWRAP`).
    pub regla: String,
    /// Archivo al que se aplica.
    pub archivo: String,
    /// Por qué se acepta.
    pub motivo: String,
}

/// El proyecto completo.
#[derive(Debug, Clone)]
pub struct Proyecto {
    /// Crates en el orden de `members`.
    pub crates: Vec<Crate>,
    /// Miembros que no se pudieron leer: (ruta, motivo).
    pub ilegibles: Vec<(String, String)>,
    /// Revisiones de `guardianes.toml`.
    pub revisiones: Vec<Revision>,
}

fn nombres_de_dependencias(tablas: &[Tabla]) -> Vec<String> {
    let mut v: Vec<String> = tablas
        .iter()
        .filter(|t| matches!(t.nombre.as_str(), "dependencies" | "build-dependencies"))
        .flat_map(|t| t.claves.iter().map(|(k, _)| k.split('.').next().unwrap_or(k).to_string()))
        .collect();
    v.sort();
    v.dedup();
    v
}

fn archivos_rs(raiz: &Path, carpeta: &Path, salida: &mut Vec<Archivo>) {
    let Ok(lista) = fs::read_dir(carpeta) else {
        return;
    };
    let mut entradas: Vec<_> = lista.flatten().collect();
    entradas.sort_by_key(|e| e.file_name());
    for e in entradas {
        let p = e.path();
        let nombre = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if nombre != "target" && !nombre.starts_with('.') {
                archivos_rs(raiz, &p, salida);
            }
        } else if nombre.ends_with(".rs")
            && let Ok(texto) = fs::read_to_string(&p)
        {
            let ruta = p.strip_prefix(raiz).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            salida.push(Archivo { ruta, texto });
        }
    }
}

/// Lee un crate de `raiz/ruta`.
fn leer_crate(raiz: &Path, ruta: &str) -> Result<Crate, String> {
    let carpeta = raiz.join(ruta);
    let texto =
        fs::read_to_string(carpeta.join("Cargo.toml")).map_err(|e| format!("no se puede leer Cargo.toml: {e}"))?;
    let tablas = toml::leer(&texto);
    let nombre = toml::tabla(&tablas, "package")
        .and_then(|t| t.texto("name"))
        .ok_or("Cargo.toml sin [package] name")?
        .to_string();
    let meta = toml::tabla(&tablas, "package.metadata.quimera").map(|t| Meta {
        nivel: t.entero("nivel").unwrap_or(-1),
        capa: t.texto("capa").unwrap_or("").to_string(),
        no_std: t.booleano("no_std").unwrap_or(false),
        externos: t.textos("externos"),
        permite_unsafe: t.booleano("permite_unsafe").unwrap_or(false),
    });
    let mut archivos = Vec::new();
    archivos_rs(raiz, &carpeta, &mut archivos);
    Ok(Crate { nombre, ruta: ruta.to_string(), meta, dependencias: nombres_de_dependencias(&tablas), archivos })
}

/// Lee el workspace de `raiz` y su `guardianes.toml` (si existe).
pub fn leer(raiz: &Path) -> Result<Proyecto, String> {
    let texto = fs::read_to_string(raiz.join("Cargo.toml"))
        .map_err(|e| format!("{}: {e}", raiz.join("Cargo.toml").display()))?;
    let tablas = toml::leer(&texto);
    let miembros = toml::tabla(&tablas, "workspace")
        .map(|t| t.textos("members"))
        .ok_or("el Cargo.toml de la raíz no es un workspace")?;
    let mut crates = Vec::new();
    let mut ilegibles = Vec::new();
    for m in miembros {
        match leer_crate(raiz, &m) {
            Ok(c) => crates.push(c),
            Err(e) => ilegibles.push((m, e)),
        }
    }
    let revisiones = fs::read_to_string(raiz.join("guardianes.toml"))
        .map(|t| {
            toml::leer(&t)
                .iter()
                .filter(|t| t.arreglo && t.nombre == "revisado")
                .map(|t| Revision {
                    regla: t.texto("regla").unwrap_or("").to_string(),
                    archivo: t.texto("archivo").unwrap_or("").to_string(),
                    motivo: t.texto("motivo").unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Proyecto { crates, ilegibles, revisiones })
}
