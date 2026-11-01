//! Funciones nativas: el objeto global, los prototipos y la biblioteca estándar mínima.
//!
//! Cada nativa es un número; `llamar` las reparte por grupos (un archivo por grupo). Lo
//! que aún no existe da `undefined` al leerlo.

mod coleccion;
mod fecha;
mod numeros;
mod regexp;
mod texto;

use crate::llamadas::Llamada;
use crate::{
    Motor, R_ARREGLO, R_CADENA, R_ERROR, R_FECHA, R_FUNCION, R_GLOBAL, R_NUMERO, R_OBJETO, R_REGEXP, operaciones as op,
};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_objetos::{Clase, Valor};

const IMPRIMIR: u32 = 0;
const OBJETO_CREAR: u32 = 1;
const ARREGLO_EMPUJAR: u32 = 2;
const ARREGLO_SACAR: u32 = 3;
const FUNCION_CALL: u32 = 10;
const OBJETO: u32 = 11;
const ARREGLO: u32 = 12;
/// Desde aquí, `Math` y los métodos de números (`numeros.rs`).
const GRUPO_NUMEROS: u32 = 100;
/// Desde aquí, cadenas y `join`/`indexOf` de arreglos (`texto.rs`).
const GRUPO_TEXTO: u32 = 200;
/// Desde aquí, `Date` y `Error` (`fecha.rs`).
const GRUPO_FECHA: u32 = 300;
/// Desde aquí, `Object.keys`, `apply` y los métodos de arreglos (`coleccion.rs`).
const GRUPO_COLECCION: u32 = 400;
/// Desde aquí, `RegExp` y los métodos de cadenas que la usan (`regexp.rs`).
const GRUPO_REGEXP: u32 = 500;

/// Crea el objeto global, los prototipos y las nativas.
pub(crate) fn preparar(m: &mut Motor) {
    let objeto = m.mundo.crear_objeto(Valor::NULO);
    m.mundo.m.pila_mut()[R_OBJETO] = objeto;
    for r in [R_FUNCION, R_ARREGLO, R_GLOBAL, R_NUMERO, R_CADENA, R_FECHA, R_ERROR, R_REGEXP] {
        let o = m.mundo.crear_objeto(m.pila(R_OBJETO));
        m.mundo.m.pila_mut()[r] = o;
    }
    definir_valor(m, R_GLOBAL, "globalThis", None);
    let globales = [
        ("undefined", Valor::INDEFINIDO),
        ("NaN", Valor::numero(f64::NAN)),
        ("Infinity", Valor::numero(f64::INFINITY)),
    ];
    for (nombre, v) in globales {
        definir_valor(m, R_GLOBAL, nombre, Some(v));
    }
    definir_nativa(m, R_GLOBAL, "print", IMPRIMIR);
    definir_nativa(m, R_FUNCION, "call", FUNCION_CALL);
    definir_nativa(m, R_ARREGLO, "push", ARREGLO_EMPUJAR);
    definir_nativa(m, R_ARREGLO, "pop", ARREGLO_SACAR);
    let console = nuevo_objeto(m, R_GLOBAL, "console");
    definir_nativa(m, console, "log", IMPRIMIR);
    let objeto = constructor(m, "Object", OBJETO, R_OBJETO);
    definir_nativa(m, objeto, "create", OBJETO_CREAR);
    let arreglo = constructor(m, "Array", ARREGLO, R_ARREGLO);
    coleccion::preparar(m, objeto, arreglo);
    numeros::preparar(m);
    texto::preparar(m);
    fecha::preparar(m);
    regexp::preparar(m);
    m.fijar_claves_ocultas();
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

/// Una cadena como valor de una propiedad.
fn definir_texto(m: &mut Motor, dueno: usize, nombre: &str, texto: &str) {
    let v = m.mundo.cadena(texto);
    definir_valor(m, dueno, nombre, Some(v));
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

/// El argumento `i` (`undefined` si no llegó).
fn arg(m: &Motor, ll: &Llamada, i: usize) -> Valor {
    if i < ll.n { m.pila(ll.args + i) } else { Valor::INDEFINIDO }
}

/// Ejecuta la nativa `id`. `Ok(true)` si dejó un marco de JavaScript pendiente (`call`).
pub(crate) fn llamar(m: &mut Motor, id: u32, ll: Llamada) -> Result<bool, Valor> {
    if id == FUNCION_CALL {
        return llamar_call(m, ll);
    }
    if id == coleccion::APPLY {
        // `apply` abre el marco de la función como una llamada más: sin recursión de Rust.
        return coleccion::aplicar(m, &ll);
    }
    let construye = matches!(id, OBJETO | ARREGLO | regexp::REGEXP) || fecha::es_constructor(id);
    if ll.construir && !construye {
        return Err(m.error("TypeError", "esta función nativa no es un constructor"));
    }
    let v = match id {
        GRUPO_REGEXP.. => regexp::llamar(m, id, &ll)?,
        GRUPO_COLECCION.. => coleccion::llamar(m, id, &ll)?,
        GRUPO_FECHA.. => fecha::llamar(m, id, &ll)?,
        GRUPO_TEXTO.. => texto::llamar(m, id, &ll)?,
        GRUPO_NUMEROS.. => numeros::llamar(m, id, &ll)?,
        _ => basica(m, id, &ll)?,
    };
    m.mundo.m.pila_mut()[ll.destino] = v;
    Ok(false)
}

/// `f.call(este, a, b)`: la función es `this` y el primer argumento, el nuevo `this`.
fn llamar_call(m: &mut Motor, ll: Llamada) -> Result<bool, Valor> {
    if ll.n == 0 {
        m.mundo.m.pila_mut()[ll.destino] = Valor::INDEFINIDO;
        return m.llamar(Llamada {
            f: ll.este,
            este: ll.destino,
            args: ll.args,
            n: 0,
            destino: ll.destino,
            construir: false,
        });
    }
    m.llamar(Llamada {
        f: ll.este,
        este: ll.args,
        args: ll.args + 1,
        n: ll.n - 1,
        destino: ll.destino,
        construir: false,
    })
}

fn basica(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
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
        OBJETO => m.mundo.crear_objeto(m.pila(R_OBJETO)),
        ARREGLO => {
            // `new Array(n)` crea n huecos; `new Array(a, b)`, esos elementos.
            let un_numero = ll.n == 1 && arg(m, ll, 0).como_numero().is_some();
            let elementos: Vec<Valor> = if un_numero {
                let n = op::a_numero(&m.mundo, arg(m, ll, 0));
                if !((0.0..=1e7).contains(&n) && (n as u32) as f64 == n) {
                    return Err(m.error("RangeError", "longitud de arreglo no válida"));
                }
                alloc::vec![Valor::INDEFINIDO; n as usize]
            } else {
                (0..ll.n).map(|i| arg(m, ll, i)).collect()
            };
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
