//! Pruebas de los guardianes con proyectos de mentira escritos en una carpeta temporal.

use crate::proyecto;
use crate::reglas;
use crate::semaforo::{Color, Hallazgo};
use std::fs;
use std::path::PathBuf;

struct Falso(PathBuf);

impl Falso {
    fn nuevo(nombre: &str) -> Falso {
        let d = std::env::temp_dir().join(format!("guardianes-{nombre}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Falso(d)
    }

    fn escribir(&self, ruta: &str, texto: &str) -> &Self {
        let p = self.0.join(ruta);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, texto).unwrap();
        self
    }

    fn crate_(&self, ruta: &str, nombre: &str, meta: &str, deps: &str, lib: &str) -> &Self {
        self.escribir(
            &format!("{ruta}/Cargo.toml"),
            &format!(
                "[package]\nname = \"{nombre}\"\n\n[package.metadata.quimera]\n{meta}\n\n[dependencies]\n{deps}\n"
            ),
        );
        self.escribir(&format!("{ruta}/src/lib.rs"), lib)
    }

    fn revisar(&self, miembros: &[&str]) -> Vec<Hallazgo> {
        let lista: Vec<String> = miembros.iter().map(|m| format!("\"{m}\"")).collect();
        self.escribir("Cargo.toml", &format!("[workspace]\nmembers = [{}]\n", lista.join(", ")));
        reglas::revisar(&proyecto::leer(&self.0).unwrap())
    }
}

impl Drop for Falso {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ids(h: &[Hallazgo]) -> Vec<&'static str> {
    let mut v: Vec<_> = h.iter().map(|x| x.regla.id).collect();
    v.sort();
    v.dedup();
    v
}

const PRUEBA: &str = "\n#[cfg(test)]\nmod p {\n    #[test]\n    fn t() { assert!(true); }\n}\n";

#[test]
fn un_proyecto_limpio_es_verde() {
    let f = Falso::nuevo("limpio");
    f.crate_(
        "cimientos/a",
        "a",
        "nivel = 0\ncapa = \"cimientos\"\nno_std = true",
        "",
        &format!("#![no_std]\npub fn f() -> u8 {{ 1 }}\n{PRUEBA}"),
    );
    f.crate_(
        "motor-js/b",
        "b",
        "nivel = 1\ncapa = \"motor-js\"",
        "a = { path = \"../../cimientos/a\" }",
        &format!("pub fn g() {{}}\n{PRUEBA}"),
    );
    let h = f.revisar(&["cimientos/a", "motor-js/b"]);
    assert!(h.is_empty(), "{h:#?}");
}

#[test]
fn mil_y_una_lineas_es_rojo_y_setecientas_amarillo() {
    let f = Falso::nuevo("tamano");
    let grande = format!("{}{PRUEBA}", "// relleno\n".repeat(1001));
    let mediano = format!("{}{PRUEBA}", "// relleno\n".repeat(750));
    f.crate_("cimientos/a", "a", "nivel = 0\ncapa = \"cimientos\"", "", &grande);
    f.crate_("cimientos/b", "b", "nivel = 0\ncapa = \"cimientos\"", "", &mediano);
    let h = f.revisar(&["cimientos/a", "cimientos/b"]);
    assert_eq!(ids(&h), ["A-TAMANO", "R-TAMANO"]);
    assert_eq!(h.iter().find(|x| x.regla.id == "R-TAMANO").unwrap().crate_, "a");
}

#[test]
fn la_jerarquia_y_los_externos_se_vigilan() {
    let f = Falso::nuevo("niveles");
    f.crate_("cimientos/a", "a", "nivel = 0\ncapa = \"cimientos\"", "b.workspace = true\nserde = \"1\"", PRUEBA);
    f.crate_("motor-js/b", "b", "nivel = 1\ncapa = \"cimientos\"", "", PRUEBA);
    let h = f.revisar(&["cimientos/a", "motor-js/b"]);
    assert_eq!(ids(&h), ["R-EXTERNO", "R-META", "R-NIVEL"], "{h:#?}");
}

#[test]
fn unsafe_sin_permiso_es_rojo_y_con_permiso_pide_safety() {
    let f = Falso::nuevo("unsafe");
    let codigo = format!("pub fn f(p: *const u8) -> u8 {{ unsafe {{ *p }} }}\n{PRUEBA}");
    f.crate_("cimientos/a", "a", "nivel = 0\ncapa = \"cimientos\"", "", &codigo);
    f.crate_("cimientos/b", "b", "nivel = 0\ncapa = \"cimientos\"\npermite_unsafe = true", "", &codigo);
    let bien =
        format!("pub fn f(p: *const u8) -> u8 {{\n    // SAFETY: p es válido.\n    unsafe {{ *p }}\n}}\n{PRUEBA}");
    f.crate_("cimientos/c", "c", "nivel = 0\ncapa = \"cimientos\"\npermite_unsafe = true", "", &bien);
    let h = f.revisar(&["cimientos/a", "cimientos/b", "cimientos/c"]);
    let de = |c: &str| h.iter().filter(|x| x.crate_ == c).map(|x| x.regla.id).collect::<Vec<_>>();
    assert_eq!(de("a"), ["R-UNSAFE"]);
    assert_eq!(de("b"), ["R-SAFETY"]);
    assert!(de("c").is_empty());
}

#[test]
fn los_amarillos_se_revisan_y_el_modo_estricto_los_exige() {
    let f = Falso::nuevo("amarillos");
    let codigo = "#![allow(dead_code)]\npub fn f(x: Option<u8>) -> u8 { x.unwrap() } // TODO: devolver error\n";
    f.crate_("cimientos/a", "a", "nivel = 0\ncapa = \"cimientos\"", "", codigo);
    let h = f.revisar(&["cimientos/a"]);
    assert_eq!(ids(&h), ["A-ALLOW", "A-PENDIENTE", "A-SINPRUEBAS", "A-UNWRAP"]);
    assert!(h.iter().all(|x| x.color(false) == Color::Amarillo));
    assert!(h.iter().all(|x| x.color(true) == Color::Rojo), "estricto: sin revisar es rojo");

    f.escribir("guardianes.toml", "[[revisado]]\nregla = \"A-UNWRAP\"\narchivo = \"cimientos/a/src/lib.rs\"\nmotivo = \"x siempre es Some aquí\"\n\n[[revisado]]\nregla = \"A-PANIC\"\narchivo = \"cimientos/a/src/lib.rs\"\nmotivo = \"vieja\"\n");
    let h = f.revisar(&["cimientos/a"]);
    let unwrap = h.iter().find(|x| x.regla.id == "A-UNWRAP").unwrap();
    assert_eq!(unwrap.color(true), Color::Verde);
    assert!(h.iter().any(|x| x.regla.id == "R-REVISION"), "la revisión de A-PANIC ya no aplica");
}

#[test]
fn las_pruebas_pueden_usar_unwrap_y_las_funciones_largas_avisan() {
    let f = Falso::nuevo("funciones");
    let larga = format!(
        "pub fn larga() {{\n{}}}\n#[cfg(test)]\nmod p {{\n    #[test]\n    fn t() {{ Some(1).unwrap(); }}\n}}\n",
        "    let _x = 1;\n".repeat(90)
    );
    f.crate_("cimientos/a", "a", "nivel = 0\ncapa = \"cimientos\"", "", &larga);
    let h = f.revisar(&["cimientos/a"]);
    assert_eq!(ids(&h), ["A-FUNCION"], "{h:#?}");
}

#[test]
fn quimera_pasa_sus_propios_guardianes() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p = proyecto::leer(&raiz).unwrap();
    let h = reglas::revisar(&p);
    let rojos: Vec<String> = h.iter().filter(|x| x.color(true) == Color::Rojo).map(|x| x.to_string()).collect();
    assert!(rojos.is_empty(), "QUIMERA debe pasar en modo estricto:\n{}", rojos.join("\n"));
}
