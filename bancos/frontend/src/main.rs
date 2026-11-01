//! quimera-frontend contra el parser de Boa 0.22, con el mismo JavaScript.
//!
//! La entrada es el corpus de pruebas del frontend (los benchmarks de Boa y V8: Richards,
//! DeltaBlue...) repetido hasta pasar de 1 MB. Cada copia va dentro de su propia función,
//! para que las `const` repetidas no choquen. Se mide el mejor de cinco intentos:
//! - Boa: texto → árbol (su parser).
//! - QUIMERA: texto → árbol (léxico + parser), y texto → bytecode (todo el frontend).
//!
//! Uso: `cargo run --release -p quimera-banco-frontend [copias]`

use std::path::Path;
use std::time::{Duration, Instant};

fn corpus(copias: usize) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../motor-js/frontend/tests/datos");
    let mut scripts: Vec<String> = std::fs::read_dir(dir)
        .expect("la carpeta del corpus existe")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "js"))
        .map(|e| std::fs::read_to_string(e.path()).expect("el corpus se lee"))
        .collect();
    scripts.sort();
    let mut texto = String::new();
    for _ in 0..copias {
        for s in &scripts {
            texto.push_str("(function () {\n");
            texto.push_str(s);
            texto.push_str("\n})();\n");
        }
    }
    texto
}

/// El mejor de cinco: lo que tarda `f` sin ruido de otros procesos.
fn mejor(f: impl Fn() -> usize) -> (Duration, usize) {
    let mut mejor = Duration::MAX;
    let mut resultado = 0;
    for _ in 0..5 {
        let t = Instant::now();
        resultado = std::hint::black_box(f());
        mejor = mejor.min(t.elapsed());
    }
    (mejor, resultado)
}

fn boa(codigo: &str) -> usize {
    let mut interner = boa_interner::Interner::default();
    let scope = boa_ast::scope::Scope::new_global();
    let script = boa_parser::Parser::new(boa_parser::Source::from_bytes(codigo))
        .parse_script(&scope, &mut interner)
        .expect("Boa analiza el corpus");
    script.statements().statements().len()
}

fn main() {
    // Boa recursa en la pila nativa: EarleyBoyer no cabe en el 1 MB del hilo principal de Windows.
    let hilo = std::thread::Builder::new().stack_size(512 << 20).spawn(medir).expect("hilo de medida");
    hilo.join().expect("la medida termina");
}

fn medir() {
    let copias: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(16);
    let texto = corpus(copias);
    let mb = texto.len() as f64 / 1e6;
    println!("Entrada: {mb:.2} MB ({copias} copias del corpus del frontend)\n");

    let (t_boa, n_boa) = mejor(|| boa(&texto));
    let (t_arbol, n_arbol) = mejor(|| quimera_frontend::analizar(&texto).expect("QUIMERA analiza el corpus").len());
    let (t_todo, instrucciones) =
        mejor(|| quimera_frontend::compilar(&texto).expect("QUIMERA compila el corpus").instrucciones());
    assert_eq!(n_boa, n_arbol, "los dos parsers deben ver las mismas sentencias");

    let fila = |nombre: &str, t: Duration| {
        println!("{nombre:<34} {:>8.1} ms {:>8.1} MB/s", t.as_secs_f64() * 1e3, mb / t.as_secs_f64());
    };
    fila("Boa 0.22: texto → árbol", t_boa);
    fila("QUIMERA: texto → árbol", t_arbol);
    fila("QUIMERA: texto → bytecode (todo)", t_todo);
    println!("\n{instrucciones} instrucciones de bytecode");
    println!(
        "QUIMERA / Boa: árbol {:.2}x más rápido; frontend completo {:.2}x",
        t_boa.as_secs_f64() / t_arbol.as_secs_f64(),
        t_boa.as_secs_f64() / t_todo.as_secs_f64()
    );
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn los_dos_parsers_ven_las_mismas_sentencias() {
        let texto = super::corpus(1);
        // QUIMERA en la pila normal de una prueba (2 MB); Boa recursa en Rust y con
        // EarleyBoyer necesita más, así que va en un hilo con pila grande.
        let quimera = quimera_frontend::analizar(&texto).unwrap();
        let copia = texto.clone();
        let boa = std::thread::Builder::new().stack_size(256 << 20).spawn(move || super::boa(&copia)).unwrap();
        assert_eq!(boa.join().unwrap(), quimera.len());
        assert!(quimera_frontend::compilar(&texto).unwrap().instrucciones() > 1000);
    }
}
