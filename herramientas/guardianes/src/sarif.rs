//! Los hallazgos en SARIF 2.1.0, el formato estándar de los analizadores (GitHub, editores,
//! SUPER GRAFO). Así otras herramientas pintan el semáforo sin entender nuestro informe.
//!
//! - Rojo: `error`. Amarillo sin revisar: `warning`. Amarillo revisado: `note`, con la
//!   revisión de `guardianes.toml` como `suppressions` (quien lee SARIF puede ocultarlo).
//! - Un hallazgo del crate entero apunta a su `Cargo.toml`; uno del proyecto, sin lugar.

use crate::semaforo::{Color, Hallazgo, REGLAS};

/// Texto JSON entre comillas, con lo necesario escapado.
fn cadena(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn nivel(h: &Hallazgo) -> &'static str {
    match (h.regla.color, &h.revisado) {
        (Color::Rojo, _) => "error",
        (Color::Amarillo, None) => "warning",
        _ => "note",
    }
}

/// Dónde está: el archivo, o el `Cargo.toml` del crate si es del crate entero.
fn lugar(h: &Hallazgo, crates: &[(String, String)]) -> Option<String> {
    if !h.archivo.is_empty() {
        return Some(h.archivo.clone());
    }
    crates.iter().find(|(n, _)| *n == h.crate_).map(|(_, ruta)| format!("{ruta}/Cargo.toml"))
}

fn resultado(h: &Hallazgo, crates: &[(String, String)]) -> String {
    let mut r = format!(
        "{{\"ruleId\":{},\"level\":\"{}\",\"message\":{{\"text\":{}}}",
        cadena(h.regla.id),
        nivel(h),
        cadena(&h.mensaje)
    );
    if let Some(uri) = lugar(h, crates) {
        let region = if h.linea > 0 { format!(",\"region\":{{\"startLine\":{}}}", h.linea) } else { String::new() };
        r.push_str(&format!(
            ",\"locations\":[{{\"physicalLocation\":{{\"artifactLocation\":{{\"uri\":{}}}{region}}}}}]",
            cadena(&uri)
        ));
    }
    if let Some(motivo) = &h.revisado {
        r.push_str(&format!(",\"suppressions\":[{{\"kind\":\"external\",\"justification\":{}}}]", cadena(motivo)));
    }
    r.push('}');
    r
}

/// El documento SARIF. `crates`: (nombre, ruta relativa a la raíz) de cada crate.
pub fn documento(hallazgos: &[Hallazgo], crates: &[(String, String)]) -> String {
    let reglas: Vec<String> = REGLAS
        .iter()
        .map(|r| {
            format!(
                "{{\"id\":{},\"shortDescription\":{{\"text\":{}}},\"fullDescription\":{{\"text\":{}}}}}",
                cadena(r.id),
                cadena(r.exige),
                cadena(r.porque)
            )
        })
        .collect();
    let resultados: Vec<String> = hallazgos.iter().map(|h| resultado(h, crates)).collect();
    format!(
        "{{\"version\":\"2.1.0\",\"$schema\":\"https://json.schemastore.org/sarif-2.1.0.json\",\"runs\":[{{\"tool\":{{\"driver\":{{\"name\":\"Guardianes de QUIMERA\",\"rules\":[{}]}}}},\"results\":[{}]}}]}}\n",
        reglas.join(","),
        resultados.join(",")
    )
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::semaforo::regla;

    fn hallazgo(id: &str, archivo: &str, linea: usize, revisado: Option<&str>) -> Hallazgo {
        Hallazgo {
            regla: regla(id),
            crate_: String::from("quimera-dom"),
            archivo: archivo.to_string(),
            linea,
            mensaje: String::from("dice \"hola\"\ny adiós"),
            revisado: revisado.map(str::to_string),
        }
    }

    #[test]
    fn niveles_lugares_y_revisiones() {
        let crates = [(String::from("quimera-dom"), String::from("web/dom"))];
        let h = [
            hallazgo("R-TAMANO", "web/dom/src/lib.rs", 1001, None),
            hallazgo("A-CRATE", "", 0, None),
            hallazgo("A-CRATE", "", 0, Some("se parte en la fase 2")),
        ];
        let d = documento(&h, &crates);
        assert!(d.contains("\"version\":\"2.1.0\""));
        assert!(d.contains("\"level\":\"error\""), "rojo es error");
        assert!(d.contains("\"uri\":\"web/dom/src/lib.rs\"},\"region\":{\"startLine\":1001}"));
        assert!(d.contains("\"uri\":\"web/dom/Cargo.toml\""), "el crate entero apunta a su Cargo.toml");
        assert!(d.contains("\"level\":\"note\""), "un amarillo revisado es nota");
        assert!(d.contains("\"justification\":\"se parte en la fase 2\""));
        assert!(d.contains("dice \\\"hola\\\"\\ny adiós"), "comillas y saltos escapados");
    }
}
