//! Los scripts de los benchmarks de Boa y V8 se analizan y se compilan sin errores.

use std::path::Path;

fn corpus() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/datos");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "js"))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn el_corpus_se_analiza() {
    let mut fallos = Vec::new();
    for (nombre, texto) in corpus() {
        match quimera_frontend::analizar(&texto) {
            Ok(p) => assert!(!p.is_empty(), "{nombre}"),
            Err(e) => fallos.push(format!("{nombre}:{e}")),
        }
    }
    assert!(fallos.is_empty(), "\n{}", fallos.join("\n"));
}

#[test]
fn el_corpus_se_compila() {
    let mut fallos = Vec::new();
    for (nombre, texto) in corpus() {
        match quimera_frontend::compilar(&texto) {
            Ok(s) => assert!(s.instrucciones() > 0, "{nombre}"),
            Err(e) => fallos.push(format!("{nombre}:{e}")),
        }
    }
    assert!(
        fallos.is_empty(),
        "
{}",
        fallos.join(
            "
"
        )
    );
}
