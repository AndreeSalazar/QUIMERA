//! Funciones nativas mínimas: lo que usan los benchmarks de Boa (y un poco más).
//!
//! Sin `std` no hay `floor` ni `sqrt` de la biblioteca: se calculan aquí. Lo que aún no
//! existe (la mayoría de la biblioteca estándar de JavaScript) da `undefined` al leerlo.

use crate::llamadas::Llamada;
use crate::{Motor, R_ARREGLO, R_FUNCION, R_GLOBAL, R_OBJETO, operaciones as op};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_objetos::{Clase, Valor};

const IMPRIMIR: u32 = 0;
const OBJETO_CREAR: u32 = 1;
const ARREGLO_EMPUJAR: u32 = 2;
const ARREGLO_SACAR: u32 = 3;
const MATH_FLOOR: u32 = 4;
const MATH_ABS: u32 = 5;
const MATH_MAX: u32 = 6;
const MATH_MIN: u32 = 7;
const MATH_SQRT: u32 = 8;
const MATH_RANDOM: u32 = 9;
const FUNCION_CALL: u32 = 10;
const OBJETO: u32 = 11;
const ARREGLO: u32 = 12;

/// Crea el objeto global, los prototipos y las nativas.
pub(crate) fn preparar(m: &mut Motor) {
    let w = &mut m.mundo;
    let objeto = w.crear_objeto(Valor::NULO);
    w.m.pila_mut()[R_OBJETO] = objeto;
    for r in [R_FUNCION, R_ARREGLO, R_GLOBAL] {
        let o = w.crear_objeto(w.m.pila()[R_OBJETO]);
        w.m.pila_mut()[r] = o;
    }
    definir_valor(m, R_GLOBAL, "globalThis", None);
    for (nombre, v) in
        [("undefined", Valor::INDEFINIDO), ("NaN", Valor::numero(f64::NAN)), ("Infinity", Valor::numero(f64::INFINITY))]
    {
        definir_valor(m, R_GLOBAL, nombre, Some(v));
    }
    definir_nativa(m, R_GLOBAL, "print", IMPRIMIR);
    definir_nativa(m, R_FUNCION, "call", FUNCION_CALL);
    definir_nativa(m, R_ARREGLO, "push", ARREGLO_EMPUJAR);
    definir_nativa(m, R_ARREGLO, "pop", ARREGLO_SACAR);
    let console = nuevo_objeto(m, R_GLOBAL, "console");
    definir_nativa(m, console, "log", IMPRIMIR);
    let math = nuevo_objeto(m, R_GLOBAL, "Math");
    for (nombre, id) in [
        ("floor", MATH_FLOOR),
        ("abs", MATH_ABS),
        ("max", MATH_MAX),
        ("min", MATH_MIN),
        ("sqrt", MATH_SQRT),
        ("random", MATH_RANDOM),
    ] {
        definir_nativa(m, math, nombre, id);
    }
    definir_valor(m, math, "PI", Some(Valor::numero(core::f64::consts::PI)));
    definir_valor(m, math, "E", Some(Valor::numero(core::f64::consts::E)));
    let objeto = constructor(m, "Object", OBJETO, R_OBJETO);
    definir_nativa(m, objeto, "create", OBJETO_CREAR);
    constructor(m, "Array", ARREGLO, R_ARREGLO);
    m.mundo.m.soltar_raices(crate::N_FIJAS);
}

/// `dueño.nombre = v` (o el propio objeto global si `v` es `None`). `dueño` es una posición de la pila.
fn definir_valor(m: &mut Motor, dueno: usize, nombre: &str, v: Option<Valor>) {
    let clave = m.mundo.cadenas.internar(nombre);
    let v = v.unwrap_or_else(|| m.pila(R_GLOBAL));
    let o = m.pila(dueno);
    m.mundo.escribir(o, clave, v);
}

fn definir_nativa(m: &mut Motor, dueno: usize, nombre: &str, id: u32) {
    let f = m.mundo.crear_nativa(id, m.pila(R_FUNCION));
    definir_valor(m, dueno, nombre, Some(f));
}

/// Crea `dueño.nombre = {}` y lo deja en la pila; devuelve su posición.
fn nuevo_objeto(m: &mut Motor, dueno: usize, nombre: &str) -> usize {
    let o = m.mundo.crear_objeto(m.pila(R_OBJETO));
    m.mundo.m.raiz(o);
    let pos = m.mundo.m.marca_raices() - 1;
    definir_valor(m, dueno, nombre, Some(m.pila(pos)));
    pos
}

/// Una función constructora global con su `prototype`; devuelve su posición en la pila.
fn constructor(m: &mut Motor, nombre: &str, id: u32, proto: usize) -> usize {
    let f = m.mundo.crear_nativa(id, m.pila(R_FUNCION));
    m.mundo.m.raiz(f);
    let pos = m.mundo.m.marca_raices() - 1;
    definir_valor(m, R_GLOBAL, nombre, Some(m.pila(pos)));
    definir_valor(m, pos, "prototype", Some(m.pila(proto)));
    let constructor = m.mundo.cadenas.internar("constructor");
    let (p, f) = (m.pila(proto), m.pila(pos));
    m.mundo.escribir(p, constructor, f);
    pos
}

fn arg(m: &Motor, ll: &Llamada, i: usize) -> Valor {
    if i < ll.n { m.pila(ll.args + i) } else { Valor::INDEFINIDO }
}

fn numeros(m: &Motor, ll: &Llamada) -> Vec<f64> {
    (0..ll.n).map(|i| op::a_numero(&m.mundo, arg(m, ll, i))).collect()
}

/// `floor` sin `std`.
fn suelo(x: f64) -> f64 {
    if !x.is_finite() || !(x > -4.5e15 && x < 4.5e15) {
        return x;
    }
    let t = (x as i64) as f64;
    if t > x { t - 1.0 } else { t }
}

/// `sqrt` sin `std`: Newton desde una buena aproximación de los bits.
fn raiz(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    let mut y = f64::from_bits((x.to_bits() >> 1) + (1023u64 << 51));
    for _ in 0..6 {
        y = 0.5 * (y + x / y);
    }
    y
}

/// Ejecuta la nativa `id`. `Ok(true)` si dejó un marco de JavaScript pendiente (`call`).
pub(crate) fn llamar(m: &mut Motor, id: u32, ll: Llamada) -> Result<bool, Valor> {
    if id == FUNCION_CALL {
        // f.call(este, a, b): la función es `this`; el primer argumento, el nuevo `this`.
        let n = ll.n.saturating_sub(1);
        let nueva = Llamada { f: ll.este, este: ll.args, args: ll.args + 1, n, destino: ll.destino, construir: false };
        if ll.n == 0 {
            m.mundo.m.pila_mut()[ll.destino] = Valor::INDEFINIDO;
            let sin_this = Llamada { este: ll.destino, ..nueva };
            return m.llamar(Llamada { n: 0, ..sin_this });
        }
        return m.llamar(nueva);
    }
    if ll.construir && !matches!(id, OBJETO | ARREGLO) {
        return Err(m.error("TypeError", "esta función nativa no es un constructor"));
    }
    let v = resultado(m, id, &ll)?;
    m.mundo.m.pila_mut()[ll.destino] = v;
    Ok(false)
}

fn resultado(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    Ok(match id {
        IMPRIMIR => {
            let partes: Vec<String> = (0..ll.n).map(|i| m.texto(arg(m, ll, i))).collect();
            m.salida.push(partes.join(" "));
            Valor::INDEFINIDO
        }
        OBJETO_CREAR => {
            let proto = arg(m, ll, 0);
            if !(m.mundo.es_objeto(proto) || proto == Valor::NULO) {
                return Err(m.error("TypeError", "Object.create necesita un objeto o null"));
            }
            m.mundo.crear_objeto(proto)
        }
        ARREGLO_EMPUJAR | ARREGLO_SACAR => return arreglo(m, id, ll),
        MATH_FLOOR => op::numero(suelo(op::a_numero(&m.mundo, arg(m, ll, 0)))),
        MATH_ABS => op::numero(f64::from_bits(op::a_numero(&m.mundo, arg(m, ll, 0)).to_bits() & !(1 << 63))),
        MATH_MAX => op::numero(
            numeros(m, ll)
                .into_iter()
                .fold(f64::NEG_INFINITY, |a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }),
        ),
        MATH_MIN => op::numero(
            numeros(m, ll)
                .into_iter()
                .fold(f64::INFINITY, |a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }),
        ),
        MATH_SQRT => op::numero(raiz(op::a_numero(&m.mundo, arg(m, ll, 0)))),
        MATH_RANDOM => {
            m.azar ^= m.azar << 13;
            m.azar ^= m.azar >> 7;
            m.azar ^= m.azar << 17;
            Valor::numero((m.azar >> 11) as f64 / (1u64 << 53) as f64)
        }
        OBJETO => m.mundo.crear_objeto(m.pila(R_OBJETO)),
        ARREGLO => {
            let elementos: Vec<Valor> = (0..ll.n).map(|i| arg(m, ll, i)).collect();
            m.mundo.crear_arreglo(&elementos, m.pila(R_ARREGLO))
        }
        _ => Valor::INDEFINIDO,
    })
}

fn arreglo(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    if m.clase_en(ll.este) != Some(Clase::Arreglo) {
        return Err(m.error("TypeError", "push/pop necesitan un arreglo"));
    }
    if id == ARREGLO_SACAR {
        let a = m.pila(ll.este);
        return Ok(m.mundo.sacar(a).unwrap_or(Valor::INDEFINIDO));
    }
    let mut largo = m.mundo.longitud(m.pila(ll.este)).unwrap_or(0);
    for i in 0..ll.n {
        let (a, v) = (m.pila(ll.este), arg(m, ll, i));
        largo = m.mundo.empujar(a, v).ok_or_else(|| m.error("RangeError", "arreglo demasiado largo"))?;
    }
    Ok(Valor::entero(largo as i32))
}
