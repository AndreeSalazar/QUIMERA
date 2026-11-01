//! Lo que QUIMERA recuerda entre una vez y otra: por ahora, las cookies que no son de sesión.
//!
//! Van en `%APPDATA%\QUIMERA\galletas.txt` (Windows) o `~/.local/share/quimera/galletas.txt`
//! (`$XDG_DATA_HOME` si está). Es texto sin cifrar, como el `Cookies` de un perfil de Firefox
//! sin contraseña maestra: quien pueda leer la carpeta del usuario puede leer las sesiones.

use quimera_red::Cliente;
use std::path::PathBuf;

/// La carpeta de datos de QUIMERA.
fn carpeta() -> Option<PathBuf> {
    if let Some(a) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(a).join("QUIMERA"));
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(x).join("quimera"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/quimera"))
}

fn archivo_galletas() -> Option<PathBuf> {
    carpeta().map(|c| c.join("galletas.txt"))
}

/// Carga en el cliente las cookies guardadas (si hay).
pub fn cargar_galletas(cliente: &Cliente) {
    if let Some(texto) = archivo_galletas().and_then(|a| std::fs::read_to_string(a).ok()) {
        cliente.cargar_galletas(&texto);
    }
}

/// Guarda las cookies del cliente que duran más que la sesión.
pub fn guardar_galletas(cliente: &Cliente) {
    let Some(archivo) = archivo_galletas() else { return };
    let texto = cliente.galletas_texto();
    let hecho = archivo.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&archivo, texto));
    if let Err(e) = hecho {
        eprintln!("no se pudieron guardar las cookies en {}: {e}", archivo.display());
    }
}
