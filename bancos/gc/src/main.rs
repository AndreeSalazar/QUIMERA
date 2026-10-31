//! quimera-gc contra boa_gc, con la misma carga.
//!
//! La carga imita a Splay (el benchmark donde el GC de Boa se llevaba el 27 %): una tabla
//! de larga vida que se va modificando con objetos nuevos. En cada paso:
//! - se crea un subárbol de 15 nodos (profundidad 3, cuatro números por nodo),
//! - se guarda en una casilla al azar de la tabla (el subárbol viejo pasa a ser basura),
//! - y se crean 10 objetos temporales que mueren enseguida.
//!
//! Uso: `cargo run --release -p quimera-banco-gc [pasos] [casillas]`
#![allow(missing_docs)]

use std::time::{Duration, Instant};

const NUMEROS: usize = 4;
const PROFUNDIDAD: u32 = 3;
const TEMPORALES: usize = 10;

struct Azar(u64);
impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

struct Medida {
    nombre: &'static str,
    total: Duration,
    peor_paso: Duration,
    pasos_lentos: usize,
    comprobacion: f64,
    extra: String,
}

/// Paso lento: más de 1 ms (una recolección que se nota).
const LENTO: Duration = Duration::from_millis(1);

// ---------------- QUIMERA ----------------

fn quimera(pasos: usize, casillas: usize) -> Medida {
    use quimera_gc::{Config, Monton};
    use quimera_valor::Valor;

    fn subarbol(m: &mut Monton, prof: u32, semilla: f64) -> Valor {
        // Cuatro números y dos hijos, en la pila: crear el objeto no pide memoria al sistema.
        let mut campos = [Valor::NULO; NUMEROS + 2];
        for (i, c) in campos.iter_mut().take(NUMEROS).enumerate() {
            *c = Valor::numero(semilla + i as f64);
        }
        if prof == 0 {
            return m.asignar(&campos);
        }
        let marca = m.marca_raices();
        let izq = subarbol(m, prof - 1, semilla * 0.5);
        let r_izq = m.raiz(izq);
        let der = subarbol(m, prof - 1, semilla * 0.25);
        campos[NUMEROS] = m.valor(&r_izq);
        campos[NUMEROS + 1] = der;
        let nodo = m.asignar(&campos);
        m.soltar_raices(marca);
        nodo
    }

    let mut m = Monton::nuevo(Config::default());
    let tabla = m.asignar(&vec![Valor::NULO; casillas]);
    let r_tabla = m.raiz(tabla);
    let mut azar = Azar(0x9E37_79B9_7F4A_7C15);
    let (mut peor, mut lentos) = (Duration::ZERO, 0);
    let inicio = Instant::now();
    for paso in 0..pasos {
        let t = Instant::now();
        let arbol = subarbol(&mut m, PROFUNDIDAD, paso as f64);
        let casilla = (azar.siguiente() % casillas as u64) as usize;
        let tabla = m.valor(&r_tabla);
        m.escribir(tabla, casilla, arbol);
        for k in 0..TEMPORALES {
            m.asignar(&[Valor::entero(k as i32); 3]);
        }
        let d = t.elapsed();
        peor = peor.max(d);
        lentos += usize::from(d > LENTO);
    }
    let total = inicio.elapsed();
    // Comprobación: suma del primer número de cada raíz de subárbol.
    let tabla = m.valor(&r_tabla);
    let comprobacion = (0..casillas)
        .filter_map(|i| m.leer(tabla, i))
        .filter_map(|a| m.leer(a, 0).and_then(Valor::como_numero))
        .sum();
    let e = m.estadisticas();
    Medida {
        nombre: "quimera-gc",
        total,
        peor_paso: peor,
        pasos_lentos: lentos,
        comprobacion,
        extra: format!(
            "menores {}, mayores {}, grandes {}, promovidos {}, vivos {}, ~{:.1} MB",
            e.menores,
            e.mayores,
            e.grandes,
            e.promovidos,
            e.vivos_principal,
            m.bytes_aprox() as f64 / 1e6
        ),
    }
}

// ---------------- boa_gc ----------------

fn boa(pasos: usize, casillas: usize) -> Medida {
    use boa_gc::{Finalize, Gc, GcRefCell, Trace};

    #[derive(Trace, Finalize)]
    struct Nodo {
        #[unsafe_ignore_trace]
        numeros: [f64; NUMEROS],
        izq: Option<Gc<Nodo>>,
        der: Option<Gc<Nodo>>,
    }

    #[derive(Trace, Finalize)]
    struct Temporal {
        #[unsafe_ignore_trace]
        datos: [i32; 3],
    }

    fn subarbol(prof: u32, semilla: f64) -> Gc<Nodo> {
        let numeros = std::array::from_fn(|i| semilla + i as f64);
        if prof == 0 {
            return Gc::new(Nodo {
                numeros,
                izq: None,
                der: None,
            });
        }
        let izq = subarbol(prof - 1, semilla * 0.5);
        let der = subarbol(prof - 1, semilla * 0.25);
        Gc::new(Nodo {
            numeros,
            izq: Some(izq),
            der: Some(der),
        })
    }

    let tabla: Gc<GcRefCell<Vec<Option<Gc<Nodo>>>>> = Gc::new(GcRefCell::new(vec![None; casillas]));
    let mut azar = Azar(0x9E37_79B9_7F4A_7C15);
    let (mut peor, mut lentos) = (Duration::ZERO, 0);
    let inicio = Instant::now();
    for paso in 0..pasos {
        let t = Instant::now();
        let arbol = subarbol(PROFUNDIDAD, paso as f64);
        let casilla = (azar.siguiente() % casillas as u64) as usize;
        tabla.borrow_mut()[casilla] = Some(arbol);
        for k in 0..TEMPORALES {
            let t = Gc::new(Temporal {
                datos: [k as i32; 3],
            });
            std::hint::black_box(t.datos);
        }
        let d = t.elapsed();
        peor = peor.max(d);
        lentos += usize::from(d > LENTO);
    }
    let total = inicio.elapsed();
    let comprobacion = tabla.borrow().iter().flatten().map(|a| a.numeros[0]).sum();
    Medida {
        nombre: "boa_gc 0.22",
        total,
        peor_paso: peor,
        pasos_lentos: lentos,
        comprobacion,
        extra: String::new(),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let pasos: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(200_000);
    let casillas: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(8_000);
    println!(
        "Carga: {pasos} pasos, tabla de {casillas} casillas, {} objetos por paso",
        15 + TEMPORALES
    );
    let medidas = [quimera(pasos, casillas), boa(pasos, casillas)];
    assert_eq!(
        medidas[0].comprobacion, medidas[1].comprobacion,
        "las dos tablas deben quedar iguales"
    );
    println!(
        "{:<12} {:>10} {:>12} {:>14}  detalle",
        "gc", "total", "peor paso", "pasos > 1 ms"
    );
    for m in &medidas {
        println!(
            "{:<12} {:>8.0}ms {:>10.2}ms {:>14}  {}",
            m.nombre,
            m.total.as_secs_f64() * 1e3,
            m.peor_paso.as_secs_f64() * 1e3,
            m.pasos_lentos,
            m.extra
        );
    }
    println!(
        "QUIMERA / Boa: {:.2}x mas rapido en total, peor pausa {:.2}x",
        medidas[1].total.as_secs_f64() / medidas[0].total.as_secs_f64(),
        medidas[1].peor_paso.as_secs_f64() / medidas[0].peor_paso.as_secs_f64()
    );
}
