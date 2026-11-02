//! Funciones nativas: el objeto global, los prototipos y la biblioteca estándar mínima.
//!
//! Cada nativa es un número; `llamar` las reparte por grupos (un archivo por grupo). Lo
//! que aún no existe da `undefined` al leerlo.

mod coleccion;
mod fecha;
mod json;
mod numeros;
mod regexp;
mod texto;
mod uri;

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
/// `Function(a, b, cuerpo)`: compila el texto.
const FUNCION: u32 = 13;
/// `__quimera_no_enumerar(o)`: las claves que tiene ahora `o` no salen en `for-in` (métodos de clase).
const NO_ENUMERAR: u32 = 14;
/// `Symbol(descripción)`, `Symbol.for(clave)` y `Symbol.keyFor(s)`.
const SIMBOLO: u32 = 15;
const SIMBOLO_FOR: u32 = 16;
const SIMBOLO_KEY_FOR: u32 = 17;
/// `gen.next(x)`, `gen.return(x)`, `gen.throw(x)` (`generadores.rs`).
const GEN_NEXT: u32 = 18;
const GEN_RETURN: u32 = 19;
const GEN_THROW: u32 = 20;
/// `Object.prototype.toString`: `[object Array]`, `[object Map]`... La conversión a texto
/// interna (`a_primitivo`) la salta y usa el texto por defecto, que es lo mismo y más rápido.
pub(crate) const OBJETO_TO_STRING: u32 = 21;
/// `Function.prototype.toString`.
const FUNCION_TO_STRING: u32 = 22;
/// `__quimera_modulo(texto)`: compila un módulo y da la función que lo ejecuta.
const MODULO: u32 = 23;
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
/// Desde aquí, `JSON` (`json.rs`).
const GRUPO_JSON: u32 = 600;
/// Desde aquí, `encodeURIComponent` y compañía, `btoa`/`atob` (`uri.rs`).
const GRUPO_URI: u32 = 700;

/// Crea el objeto global, los prototipos y las nativas.
pub(crate) fn preparar(m: &mut Motor) {
    let objeto = m.mundo.crear_objeto(Valor::NULO);
    m.mundo.m.pila_mut()[R_OBJETO] = objeto;
    for r in [R_FUNCION, R_ARREGLO, R_GLOBAL, R_NUMERO, R_CADENA, R_FECHA, R_ERROR, R_REGEXP, crate::R_ANFITRION] {
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
    for nombre in ["log", "info", "warn", "error", "debug", "trace"] {
        definir_nativa(m, console, nombre, IMPRIMIR);
    }
    let objeto = constructor(m, "Object", OBJETO, R_OBJETO);
    definir_nativa(m, objeto, "create", OBJETO_CREAR);
    let arreglo = constructor(m, "Array", ARREGLO, R_ARREGLO);
    constructor(m, "Function", FUNCION, R_FUNCION);
    definir_nativa(m, R_GLOBAL, "__quimera_no_enumerar", NO_ENUMERAR);
    definir_nativa(m, R_OBJETO, "toString", OBJETO_TO_STRING);
    definir_nativa(m, R_FUNCION, "toString", FUNCION_TO_STRING);
    definir_nativa(m, R_GLOBAL, "__quimera_modulo", MODULO);
    simbolos(m);
    let generador = nuevo_objeto(m, R_GLOBAL, "__quimera_generador");
    definir_nativa(m, generador, "next", GEN_NEXT);
    definir_nativa(m, generador, "return", GEN_RETURN);
    definir_nativa(m, generador, "throw", GEN_THROW);
    coleccion::preparar(m, objeto, arreglo);
    numeros::preparar(m);
    texto::preparar(m);
    fecha::preparar(m);
    regexp::preparar(m);
    json::preparar(m);
    uri::preparar(m);
    m.fijar_claves_ocultas();
    m.mundo.m.soltar_raices(crate::N_FIJAS);
}

/// La etiqueta de `Object.prototype.toString`: `Symbol.toStringTag` si la tiene, o según su
/// clase (`Array`, `Function`, `Error`, `Date`...).
fn etiqueta_de(m: &mut Motor, v: Valor) -> String {
    use quimera_objetos::Tipo;
    match v.tipo() {
        Tipo::Indefinido => return String::from("Undefined"),
        Tipo::Nulo => return String::from("Null"),
        Tipo::Numero | Tipo::Entero => return String::from("Number"),
        Tipo::Cadena => return String::from("String"),
        Tipo::Booleano => return String::from("Boolean"),
        Tipo::Simbolo => return String::from("Symbol"),
        Tipo::Referencia => {}
    }
    let clave = m.mundo.cadenas.internar("\u{2}@@\u{2}Symbol.toStringTag");
    if let Some(t) = m.mundo.leer(v, clave).filter(|t| t.tipo() == Tipo::Cadena) {
        return m.texto(t);
    }
    match m.mundo.clase(v) {
        Some(Clase::Arreglo) => return String::from("Array"),
        Some(Clase::Funcion | Clase::Nativa) => return String::from("Function"),
        Some(Clase::RegExp) => return String::from("RegExp"),
        _ => {}
    }
    // Por su cadena de prototipos: un Error o una fecha.
    let mut p = m.mundo.prototipo(v);
    while m.mundo.es_objeto(p) {
        if p == m.pila(R_ERROR) {
            return String::from("Error");
        }
        if p == m.pila(R_FECHA) {
            return String::from("Date");
        }
        p = m.mundo.prototipo(p);
    }
    String::from("Object")
}

/// Un método de arreglo sobre algo que no es un arreglo pero lo parece (`length` e índices):
/// `__quimera_generico` del preludio lo copia a un arreglo, aplica el método y, si lo cambia,
/// escribe el resultado de vuelta.
pub(super) fn generico(m: &mut Motor, nombre: &str, ll: &Llamada) -> Result<Valor, Valor> {
    let args: Vec<Valor> = (0..ll.n).map(|i| arg(m, ll, i)).collect();
    let lista = m.mundo.crear_arreglo(&args, m.pila(R_ARREGLO));
    let marca = m.mundo.m.marca_raices();
    m.mundo.m.raiz(lista);
    let clave = m.mundo.cadenas.internar("__quimera_generico");
    let f = m.mundo.leer(m.pila(R_GLOBAL), clave).unwrap_or(Valor::INDEFINIDO);
    if !m.es_funcion(f) {
        m.mundo.m.soltar_raices(marca);
        return Err(m.error("TypeError", "este método necesita un arreglo"));
    }
    let n = m.mundo.cadena(nombre);
    let (este, lista) = (m.pila(ll.este), m.pila(marca));
    let r = m.llamar_interno(f, Valor::INDEFINIDO, &[n, este, lista]);
    m.mundo.m.soltar_raices(marca);
    r
}

/// `Symbol` y los símbolos conocidos (`Symbol.iterator`...).
fn simbolos(m: &mut Motor) {
    let f = m.mundo.crear_nativa(SIMBOLO, m.pila(R_FUNCION));
    m.mundo.m.raiz(f);
    let pos = m.mundo.m.marca_raices() - 1;
    definir_valor(m, R_GLOBAL, "Symbol", Some(m.pila(pos)));
    definir_nativa(m, pos, "for", SIMBOLO_FOR);
    definir_nativa(m, pos, "keyFor", SIMBOLO_KEY_FOR);
    for nombre in ["iterator", "asyncIterator", "hasInstance", "toPrimitive", "toStringTag", "species", "unscopables"] {
        let id = m.mundo.cadenas.internar(&alloc::format!("\u{2}@@\u{2}Symbol.{nombre}"));
        definir_valor(m, pos, nombre, Some(Valor::simbolo(quimera_objetos::Ref::desde_u32(id))));
    }
}

/// Un símbolo nuevo: una cadena interna única que nadie puede escribir (`\u{2}n\u{2}desc`).
fn nuevo_simbolo(m: &mut Motor, descripcion: &str, global: bool) -> Valor {
    let interno = if global {
        alloc::format!("\u{2}for\u{2}{descripcion}")
    } else {
        m.simbolos += 1;
        alloc::format!("\u{2}{}\u{2}{descripcion}", m.simbolos)
    };
    let id = m.mundo.cadenas.internar(&interno);
    Valor::simbolo(quimera_objetos::Ref::desde_u32(id))
}

/// Ejecuta el preludio en JavaScript (`preludio.js`) y oculta a `for-in` lo que creó.
pub(crate) fn preludio(m: &mut Motor) {
    if let Err(e) = m.ejecutar(include_str!("preludio.js")) {
        debug_assert!(false, "el preludio falló: {}", e.mensaje);
        return;
    }
    let global = m.pila(R_GLOBAL);
    let clave = m.mundo.cadenas.internar("__quimera_ocultar");
    let lista = m.mundo.leer(global, clave).unwrap_or(Valor::INDEFINIDO);
    for i in 0..m.mundo.longitud(lista).unwrap_or(0) {
        let o = m.mundo.leer_elemento(lista, i).unwrap_or(Valor::INDEFINIDO);
        if let Some(forma) = m.mundo.forma(o) {
            let n = m.mundo.claves(o).len();
            m.formas_ocultas.insert(forma, n);
        }
    }
    m.mundo.borrar(global, clave);
    m.fijar_claves_ocultas();
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
    if (GEN_NEXT..=GEN_THROW).contains(&id) {
        use crate::generadores::{Modo, reanudar};
        let modo = [Modo::Siguiente, Modo::Retornar, Modo::Lanzar][(id - GEN_NEXT) as usize];
        return reanudar(m, &ll, modo);
    }
    if id >= crate::anfitrion::GRUPO_ANFITRION {
        let args = crate::Argumentos { este: ll.este, args: ll.args, n: ll.n };
        let v = m.llamar_anfitrion(id, &args)?;
        m.mundo.m.pila_mut()[ll.destino] = v;
        return Ok(false);
    }
    if id == coleccion::APPLY {
        // `apply` abre el marco de la función como una llamada más: sin recursión de Rust.
        return coleccion::aplicar(m, &ll);
    }
    let construye = matches!(id, OBJETO | ARREGLO | FUNCION | regexp::REGEXP) || fecha::es_constructor(id);
    if ll.construir && !construye {
        return Err(m.error("TypeError", "esta función nativa no es un constructor"));
    }
    let v = match id {
        GRUPO_URI.. => uri::llamar(m, id, &ll)?,
        GRUPO_JSON.. => json::llamar(m, id, &ll)?,
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
        SIMBOLO..=SIMBOLO_KEY_FOR | FUNCION_TO_STRING | OBJETO_TO_STRING | NO_ENUMERAR => return otras(m, id, ll),
        MODULO => {
            let fuente = m.texto(arg(m, ll, 0));
            m.compilar_modulo(&fuente).map_err(|e| m.mundo.cadena(&e.mensaje))?
        }
        FUNCION => {
            // Como los navegadores: `function anonymous(parámetros\n) {\ncuerpo\n}`.
            let partes: Vec<String> = (0..ll.n).map(|i| m.texto(arg(m, ll, i))).collect();
            let (cuerpo, parametros) = partes.split_last().map_or((String::new(), &[][..]), |(c, p)| (c.clone(), p));
            let fuente =
                alloc::format!("__quimera_funcion = function anonymous({}\n) {{\n{cuerpo}\n}};", parametros.join(","));
            m.ejecutar(&fuente).map_err(|e| m.mundo.cadena(&e.mensaje))?;
            let clave = m.mundo.cadenas.internar("__quimera_funcion");
            let global = m.pila(R_GLOBAL);
            let f = m.mundo.leer(global, clave).unwrap_or(Valor::INDEFINIDO);
            m.mundo.borrar(global, clave);
            f
        }
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

/// Símbolos, `toString` de objetos y funciones, `__quimera_no_enumerar`.
fn otras(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    Ok(match id {
        SIMBOLO | SIMBOLO_FOR => {
            let d = arg(m, ll, 0);
            let texto = if d == Valor::INDEFINIDO && id == SIMBOLO { String::new() } else { m.texto(d) };
            nuevo_simbolo(m, &texto, id == SIMBOLO_FOR)
        }
        SIMBOLO_KEY_FOR => {
            let s = arg(m, ll, 0);
            let interno = s
                .ref_del_monton()
                .filter(|_| s.tipo() == quimera_objetos::Tipo::Simbolo)
                .map(|r| String::from(m.mundo.cadenas.texto(r.numero() as u32)));
            match interno.as_deref().and_then(|i| i.strip_prefix("\u{2}for\u{2}")) {
                Some(k) => {
                    let k = String::from(k);
                    m.mundo.cadena(&k)
                }
                None => Valor::INDEFINIDO,
            }
        }
        FUNCION_TO_STRING => {
            // Sin el código fuente guardado: la forma que dan los navegadores para las nativas.
            let k = m.mundo.cadenas.internar("name");
            let f = m.pila(ll.este);
            let nombre = m.mundo.leer(f, k).map(|n| m.texto(n)).unwrap_or_default();
            m.mundo.cadena(&alloc::format!("function {nombre}() {{ [native code] }}"))
        }
        OBJETO_TO_STRING => {
            let etiqueta = etiqueta_de(m, m.pila(ll.este));
            m.mundo.cadena(&alloc::format!("[object {etiqueta}]"))
        }
        NO_ENUMERAR => {
            let o = arg(m, ll, 0);
            if let Some(forma) = m.mundo.forma(o).filter(|_| m.mundo.es_objeto(o)) {
                let n = m.mundo.claves(o).len();
                m.formas_ocultas.insert(forma, n);
            }
            Valor::INDEFINIDO
        }
        _ => Valor::INDEFINIDO,
    })
}

fn arreglo(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    if m.clase_en(ll.este) != Some(Clase::Arreglo) {
        return generico(m, if id == ARREGLO_SACAR { "pop" } else { "push" }, ll);
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
