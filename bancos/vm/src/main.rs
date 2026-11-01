//! La máquina virtual de QUIMERA contra boa_engine 0.22, con los benchmarks de Boa.
//!
//! Para cada script: se ejecuta (define sus funciones), y se mide `main()`, el mejor de
//! tres. Los dos motores tienen que devolver lo mismo.
//!
//! Uso: `cargo run --release -p quimera-banco-vm`
//! Probar scripts sueltos en QUIMERA: `cargo run --release -p quimera-banco-vm -- a.js b.js`

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Los benchmarks de V8 (Octane) con su arnés completo: se miden solos y escriben `SCORE`.
const V8: &[&str] = &[
    "v8-benches-richards.js",
    "v8-benches-deltablue.js",
    "v8-benches-crypto.js",
    "v8-benches-raytrace.js",
    "v8-benches-earley-boyer.js",
    "v8-benches-navier-stokes.js",
    "v8-benches-splay.js",
];

/// Reloj del anfitrión para el `Date` de QUIMERA, en milisegundos.
fn reloj() -> f64 {
    static INICIO: OnceLock<Instant> = OnceLock::new();
    INICIO.get_or_init(Instant::now).elapsed().as_secs_f64() * 1e3
}

/// La puntuación de una salida del arnés de V8 (la línea `SCORE n`).
fn puntuacion(lineas: &[String]) -> f64 {
    lineas.iter().find_map(|l| l.strip_prefix("SCORE ")).and_then(|n| n.trim().parse().ok()).unwrap_or(0.0)
}

/// Ejecuta el arnés completo en QUIMERA y devuelve su puntuación.
fn quimera_v8(codigo: &str) -> f64 {
    let mut m = quimera_vm::Motor::default();
    m.con_reloj(reloj);
    m.ejecutar(codigo).expect("QUIMERA ejecuta el script");
    m.llamar_global("main", &[]).expect("QUIMERA ejecuta main");
    puntuacion(m.salida())
}

thread_local! {
    static SALIDA_BOA: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Ejecuta el arnés completo en Boa (con `console.log` y `print` propios) y devuelve su puntuación.
fn boa_v8(codigo: &str) -> f64 {
    use boa_engine::object::ObjectInitializer;
    use boa_engine::property::Attribute;
    use boa_engine::{Context, JsResult, JsValue, NativeFunction, Source, js_string};
    fn log(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let partes: Vec<String> =
            args.iter().map(|a| a.to_string(ctx).map(|s| s.to_std_string_escaped())).collect::<JsResult<_>>()?;
        SALIDA_BOA.with(|s| s.borrow_mut().push(partes.join(" ")));
        Ok(JsValue::undefined())
    }
    SALIDA_BOA.with(|s| s.borrow_mut().clear());
    let mut ctx = Context::default();
    let console =
        ObjectInitializer::new(&mut ctx).function(NativeFunction::from_fn_ptr(log), js_string!("log"), 0).build();
    ctx.register_global_property(js_string!("console"), console, Attribute::all()).expect("console");
    ctx.register_global_callable(js_string!("print"), 0, NativeFunction::from_fn_ptr(log)).expect("print");
    ctx.eval(Source::from_bytes(codigo)).expect("Boa ejecuta el script");
    let main = ctx.global_object().get(js_string!("main"), &mut ctx).expect("main existe");
    main.as_callable()
        .expect("main es una función")
        .call(&JsValue::undefined(), &[], &mut ctx)
        .expect("Boa ejecuta main");
    SALIDA_BOA.with(|s| puntuacion(&s.borrow()))
}

const SCRIPTS: &[&str] = &[
    "basic-call-loop.js",
    "basic-nested-loop.js",
    "basic-closure.js",
    "closures-create.js",
    "closures-invoke.js",
    "properties-access.js",
    "prototypes-chain.js",
];

fn leer(nombre: &str) -> String {
    let ruta = format!("{}/../../motor-js/frontend/tests/datos/{nombre}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(ruta).expect("el script existe")
}

/// Lo que tarda `main()` en QUIMERA y lo que devuelve.
fn quimera(codigo: &str) -> (Duration, String) {
    let mut mejor = Duration::MAX;
    let mut texto = String::new();
    for _ in 0..3 {
        let mut m = quimera_vm::Motor::default();
        m.ejecutar(codigo).expect("QUIMERA ejecuta el script");
        let t = Instant::now();
        let v = m.llamar_global("main", &[]).expect("QUIMERA ejecuta main");
        mejor = mejor.min(t.elapsed());
        texto = m.texto(v);
    }
    (mejor, texto)
}

/// Lo que tarda `main()` en Boa y lo que devuelve.
fn boa(codigo: &str) -> (Duration, String) {
    use boa_engine::{Context, JsValue, Source, js_string};
    let mut mejor = Duration::MAX;
    let mut texto = String::new();
    for _ in 0..3 {
        let mut ctx = Context::default();
        ctx.eval(Source::from_bytes(codigo)).expect("Boa ejecuta el script");
        let main = ctx.global_object().get(js_string!("main"), &mut ctx).expect("main existe");
        let f = main.as_callable().expect("main es una función").clone();
        let t = Instant::now();
        let v = f.call(&JsValue::undefined(), &[], &mut ctx).expect("Boa ejecuta main");
        mejor = mejor.min(t.elapsed());
        texto = v.to_string(&mut ctx).expect("el resultado se convierte").to_std_string_escaped();
    }
    (mejor, texto)
}

/// Ejecuta cada script con su `main()` y dice si funciona, su puntuación o el primer error.
fn probar(rutas: &[String]) {
    for ruta in rutas {
        let Ok(codigo) = std::fs::read_to_string(ruta) else {
            println!("{ruta}: no se puede leer");
            continue;
        };
        let mut m = quimera_vm::Motor::default();
        m.con_reloj(reloj);
        let r = m.ejecutar(&codigo).and_then(|_| m.llamar_global("main", &[]));
        match r {
            Ok(_) => println!("{ruta}: SCORE {}", puntuacion(m.salida())),
            Err(e) => println!("{ruta}: {}", e.mensaje),
        }
    }
}

fn main() {
    // Boa recursa en la pila nativa: EarleyBoyer no cabe en el 1 MB del hilo principal de Windows.
    let hilo = std::thread::Builder::new().stack_size(512 << 20).spawn(medir).expect("hilo de medida");
    hilo.join().expect("la medida termina");
}

fn medir() {
    let rutas: Vec<String> = std::env::args().skip(1).collect();
    if !rutas.is_empty() {
        return probar(&rutas);
    }
    println!("{:<24} {:>12} {:>12} {:>9}  resultado", "script", "QUIMERA", "Boa 0.22", "QUIMERA/Boa");
    let mut log_suma = 0.0;
    for nombre in SCRIPTS {
        let codigo = leer(nombre);
        let (tq, rq) = quimera(&codigo);
        let (tb, rb) = boa(&codigo);
        assert_eq!(rq, rb, "{nombre}: los dos motores deben devolver lo mismo");
        let veces = tb.as_secs_f64() / tq.as_secs_f64();
        log_suma += veces.ln();
        println!(
            "{nombre:<24} {:>10.1}ms {:>10.1}ms {:>8.2}x  {rq}",
            tq.as_secs_f64() * 1e3,
            tb.as_secs_f64() * 1e3,
            veces
        );
    }
    println!("\nMedia geométrica: QUIMERA {:.2}x más rápido que Boa", (log_suma / SCRIPTS.len() as f64).exp());

    println!("\nV8 (Octane) con su arnés completo; puntuación, más es mejor:");
    println!("{:<30} {:>10} {:>10} {:>9}", "benchmark", "QUIMERA", "Boa 0.22", "QUIMERA/Boa");
    let mut log_v8 = 0.0;
    for nombre in V8 {
        let codigo = leer(nombre);
        let (q, b) = (quimera_v8(&codigo), boa_v8(&codigo));
        log_v8 += (q / b).ln();
        println!("{nombre:<30} {q:>10.0} {b:>10.0} {:>8.2}x", q / b);
    }
    println!(
        "
Media geométrica V8: QUIMERA {:.2}x la puntuación de Boa",
        (log_v8 / V8.len() as f64).exp()
    );
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn los_dos_motores_devuelven_lo_mismo() {
        let codigo = super::leer("closures-invoke.js");
        assert_eq!(super::quimera(&codigo).1, super::boa(&codigo).1);
    }
}
