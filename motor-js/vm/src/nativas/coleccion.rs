//! Colecciones: `Object.keys`, `hasOwnProperty`, `Array.isArray`, `Function.prototype.apply`
//! y los métodos de arreglos (`slice`, `concat`, `map`, `filter`, `reduce`, `sort`...).
//!
//! Los métodos con función de JavaScript (`map`, `sort`...) la llaman con `llamar_interno`,
//! y cualquier llamada puede mover objetos: el arreglo, la función y el resultado viven en
//! raíces propias (desde `marca`) y se vuelven a leer de ahí después de cada llamada.

use super::texto::relativo;
use super::{arg, definir_nativa};
use crate::llamadas::Llamada;
use crate::{Motor, R_ARREGLO, R_FUNCION, R_OBJETO, operaciones as op};
use alloc::vec::Vec;
use quimera_objetos::{Cadenas, Clase, Ref, Tipo, Valor};

const OBJECT_KEYS: u32 = 400;
const HAS_OWN: u32 = 401;
const IS_ARRAY: u32 = 402;
/// `f.apply(este, [args])`.
pub(super) const APPLY: u32 = 403;
const DEFINE_PROPERTY: u32 = 404;
const DESCRIPTOR: u32 = 405;
/// `Object.getOwnPropertyNames(o)`: todas las claves propias, también las no enumerables.
const NOMBRES_PROPIOS: u32 = 408;
const GET_PROTOTYPE_OF: u32 = 406;
const SET_PROTOTYPE_OF: u32 = 407;
const SLICE: u32 = 410;
const CONCAT: u32 = 411;
const REVERSE: u32 = 412;
const SHIFT: u32 = 413;
const UNSHIFT: u32 = 414;
const INCLUDES: u32 = 415;
const SORT: u32 = 416;
/// Desde aquí, métodos que llaman a una función por elemento.
const FOR_EACH: u32 = 430;
const MAP: u32 = 431;
const FILTER: u32 = 432;
const REDUCE: u32 = 433;
const SOME: u32 = 434;
const EVERY: u32 = 435;
const FIND: u32 = 436;
const FIND_INDEX: u32 = 437;

/// `objeto` y `arreglo`: posiciones en la pila de los constructores `Object` y `Array`.
pub(super) fn preparar(m: &mut Motor, objeto: usize, arreglo: usize) {
    definir_nativa(m, objeto, "keys", OBJECT_KEYS);
    definir_nativa(m, objeto, "defineProperty", DEFINE_PROPERTY);
    definir_nativa(m, objeto, "getOwnPropertyDescriptor", DESCRIPTOR);
    definir_nativa(m, objeto, "getOwnPropertyNames", NOMBRES_PROPIOS);
    definir_nativa(m, objeto, "getPrototypeOf", GET_PROTOTYPE_OF);
    definir_nativa(m, objeto, "setPrototypeOf", SET_PROTOTYPE_OF);
    definir_nativa(m, R_OBJETO, "hasOwnProperty", HAS_OWN);
    definir_nativa(m, arreglo, "isArray", IS_ARRAY);
    definir_nativa(m, R_FUNCION, "apply", APPLY);
    for (nombre, id) in METODOS {
        definir_nativa(m, R_ARREGLO, nombre, id);
    }
}

/// Los métodos de los arreglos (también para llamarlos sobre algo que no es un arreglo).
const METODOS: [(&str, u32); 15] = [
    ("slice", SLICE),
    ("concat", CONCAT),
    ("reverse", REVERSE),
    ("shift", SHIFT),
    ("unshift", UNSHIFT),
    ("includes", INCLUDES),
    ("sort", SORT),
    ("forEach", FOR_EACH),
    ("map", MAP),
    ("filter", FILTER),
    ("reduce", REDUCE),
    ("some", SOME),
    ("every", EVERY),
    ("find", FIND),
    ("findIndex", FIND_INDEX),
];

/// `Object.keys(proxy)`: lo que da su trampa `ownKeys`, o las claves del objetivo.
fn claves_de_proxy(m: &mut Motor, p: Valor) -> Result<Valor, Valor> {
    let objetivo = match m.proxy_claves(p)? {
        Ok(lista) => return Ok(lista),
        Err(objetivo) => objetivo,
    };
    let marca = m.mundo.m.marca_raices();
    m.mundo.m.raiz(objetivo);
    let mut claves: Vec<Valor> = indices(m, objetivo);
    let objetivo = m.pila(marca);
    claves.extend(m.claves_enumerables(objetivo).into_iter().map(|k| Valor::cadena(Ref::desde_u32(k))));
    let r = m.mundo.crear_arreglo(&claves, m.pila(R_ARREGLO));
    m.mundo.m.soltar_raices(marca);
    Ok(r)
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    match id {
        OBJECT_KEYS => {
            let o = arg(m, ll, 0);
            if matches!(o.tipo(), Tipo::Indefinido | Tipo::Nulo) {
                return Err(m.error("TypeError", "Object.keys necesita un objeto"));
            }
            if m.es_proxy(o) {
                return claves_de_proxy(m, o);
            }
            if !m.mundo.es_objeto(o) {
                // Un primitivo se convierte en objeto: una cadena da sus índices; lo demás, nada.
                let n = m.mundo.texto(o).map_or(0, |s| s.chars().count());
                let claves: Vec<Valor> = (0..n).map(|i| m.mundo.cadena(&alloc::format!("{i}"))).collect();
                return Ok(m.mundo.crear_arreglo(&claves, m.pila(R_ARREGLO)));
            }
            let mut claves: Vec<Valor> = indices(m, o);
            claves.extend(m.claves_enumerables(o).into_iter().map(|k| Valor::cadena(Ref::desde_u32(k))));
            return Ok(m.mundo.crear_arreglo(&claves, m.pila(R_ARREGLO)));
        }
        HAS_OWN => {
            let (o, k) = (m.pila(ll.este), arg(m, ll, 0));
            let clave = clave_de(m, k);
            let indice = k.como_entero().and_then(|i| u32::try_from(i).ok());
            let es_indice = m.mundo.clase(o) == Some(Clase::Arreglo)
                && indice.is_some_and(|i| i < m.mundo.longitud(o).unwrap_or(0) && !m.mundo.es_hueco(o, i));
            let propia = m.mundo.es_objeto(o)
                && (m.mundo.leer_propia(o, clave).is_some()
                    || (clave != u32::MAX && m.accesor_propio(o, clave).is_some()));
            return Ok(Valor::booleano(es_indice || propia));
        }
        IS_ARRAY => return Ok(Valor::booleano(m.mundo.clase(arg(m, ll, 0)) == Some(Clase::Arreglo))),
        DEFINE_PROPERTY => return definir_propiedad(m, ll),
        DESCRIPTOR => return descriptor(m, ll),
        NOMBRES_PROPIOS => return nombres_propios(m, ll),
        SET_PROTOTYPE_OF => {
            let (o, p) = (arg(m, ll, 0), arg(m, ll, 1));
            if !(m.mundo.es_objeto(p) || p == Valor::NULO) {
                return Err(m.error("TypeError", "el prototipo debe ser un objeto o null"));
            }
            // En un ciclo (`o` en la cadena de `p`) no se cambia.
            let mut actual = p;
            while m.mundo.es_objeto(actual) {
                if actual == o {
                    return Err(m.error("TypeError", "setPrototypeOf haría un ciclo de prototipos"));
                }
                actual = m.mundo.prototipo(actual);
            }
            m.mundo.poner_prototipo(o, p);
            return Ok(o);
        }
        GET_PROTOTYPE_OF => {
            let o = arg(m, ll, 0);
            return Ok(if m.mundo.es_objeto(o) { m.mundo.prototipo(o) } else { Valor::NULO });
        }
        // `apply` no llega aquí: `nativas::llamar` lo lleva a `aplicar`, que abre un marco.
        APPLY => return Err(m.error("InternalError", "apply va por su propio camino")),
        _ => {}
    }
    if m.clase_en(ll.este) != Some(Clase::Arreglo) {
        // Sobre algo parecido a un arreglo (`Array.prototype.slice.call(lista)`), la versión
        // genérica del preludio.
        let nombre = METODOS.iter().find(|(_, i)| *i == id).map_or("slice", |(n, _)| n);
        return super::generico(m, nombre, ll);
    }
    let marca = m.mundo.m.marca_raices();
    let r = if id >= FOR_EACH { por_elemento(m, id, ll, marca) } else { arreglo(m, id, ll, marca) };
    m.mundo.m.soltar_raices(marca);
    r
}

/// `"0"`, `"1"`... para un arreglo; nada para otro objeto.
fn indices(m: &mut Motor, o: Valor) -> Vec<Valor> {
    // Un arreglo, o un `new String(...)` (sus caracteres son índices, como en los navegadores).
    let n = if m.mundo.clase(o) == Some(Clase::Arreglo) {
        m.mundo.longitud(o).unwrap_or(0)
    } else {
        let texto = crate::operaciones::valor_primitivo(&m.mundo, o).and_then(|v| m.mundo.texto(v));
        texto.map_or(0, |t| t.encode_utf16().count() as u32)
    };
    let presentes: Vec<u32> = (0..n).filter(|&i| !m.mundo.es_hueco(o, i)).collect();
    presentes.into_iter().map(|i| m.mundo.cadena(&alloc::format!("{i}"))).collect()
}

/// La clave para escribir (un átomo: las formas guardan su número).
fn clave_atomo(m: &mut Motor, k: Valor) -> u32 {
    match m.mundo.cadenas.clave_para_escribir(k) {
        Some(id) => id,
        None => {
            let s = m.texto(k);
            m.mundo.cadenas.internar(&s)
        }
    }
}

/// `Object.defineProperty(o, k, {get, set})` o `{value}`. De los atributos solo se guarda
/// `enumerable` (por defecto `false` al crear la propiedad, como en los navegadores); todo es
/// escribible y configurable.
fn definir_propiedad(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let (o, d) = (arg(m, ll, 0), arg(m, ll, 2));
    if !m.mundo.es_objeto(o) || !m.mundo.es_objeto(d) {
        return Err(m.error("TypeError", "Object.defineProperty necesita un objeto y un descriptor"));
    }
    let k = arg(m, ll, 1);
    let clave = clave_atomo(m, k);
    let leer = |m: &mut Motor, nombre: &str| {
        let c = m.mundo.cadenas.internar(nombre);
        let d = arg(m, ll, 2);
        m.mundo.leer(d, c)
    };
    let (get, set, valor) = (leer(m, "get"), leer(m, "set"), leer(m, "value"));
    let enumerable = leer(m, "enumerable").map(|e| crate::operaciones::a_booleano(&m.mundo, e));
    let o = arg(m, ll, 0);
    let existia = m.mundo.leer_propia(o, clave).is_some() || m.accesor_propio(o, clave).is_some();
    if get.is_some() || set.is_some() {
        let o = arg(m, ll, 0);
        m.definir_accesor(o, clave, get, set);
    } else if let Some(v) = valor {
        let o = arg(m, ll, 0);
        m.mundo.borrar(o, clave | crate::accesores::ACCESOR);
        let o = arg(m, ll, 0);
        m.mundo.escribir(o, clave, v);
    }
    match enumerable {
        Some(e) => m.marcar_enumerable(ll.args, clave, e),
        None if !existia => m.marcar_enumerable(ll.args, clave, false),
        None => {}
    }
    Ok(arg(m, ll, 0))
}

/// `Object.getOwnPropertyNames(o)`: los índices, las claves propias en orden de creación (también
/// las no enumerables y las de la biblioteca, salvo las ranuras internas) y `length` en un arreglo.
fn nombres_propios(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let o = arg(m, ll, 0);
    if matches!(o.tipo(), Tipo::Indefinido | Tipo::Nulo) {
        return Err(m.error("TypeError", "Object.getOwnPropertyNames necesita un objeto"));
    }
    if m.es_proxy(o) || !m.mundo.es_objeto(o) {
        return llamar(m, OBJECT_KEYS, ll);
    }
    let marca = m.mundo.m.marca_raices();
    m.mundo.m.raiz(o);
    let mut claves: Vec<Valor> = indices(m, o);
    let o = m.pila(marca);
    let propias: Vec<u32> = m
        .mundo
        .claves(o)
        .iter()
        .map(|&k| k & !crate::accesores::ACCESOR)
        .filter(|&k| !crate::iteracion::oculta(m.mundo.cadenas.texto(k)))
        .collect();
    claves.extend(propias.into_iter().map(|k| Valor::cadena(Ref::desde_u32(k))));
    if m.mundo.clase(o) == Some(Clase::Arreglo) {
        claves.push(Valor::cadena(Ref::desde_u32(m.mundo.clave_length)));
    }
    let r = m.mundo.crear_arreglo(&claves, m.pila(R_ARREGLO));
    m.mundo.m.soltar_raices(marca);
    Ok(r)
}

/// `Object.getOwnPropertyDescriptor(o, k)`.
fn descriptor(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let (o, k) = (arg(m, ll, 0), arg(m, ll, 1));
    if !m.mundo.es_objeto(o) {
        return Ok(Valor::INDEFINIDO);
    }
    let clave = clave_atomo(m, k);
    let o = arg(m, ll, 0);
    let campos: Vec<(&str, Valor)> = if let Some(par) = m.accesor_propio(o, clave) {
        let g = m.mundo.leer_elemento(par, 0).unwrap_or(Valor::INDEFINIDO);
        let s = m.mundo.leer_elemento(par, 1).unwrap_or(Valor::INDEFINIDO);
        alloc::vec![("get", g), ("set", s)]
    } else if let Some(v) = m.mundo.leer_propia(o, clave) {
        alloc::vec![("value", v), ("writable", Valor::VERDADERO)]
    } else {
        return Ok(Valor::INDEFINIDO);
    };
    let marca = m.mundo.m.marca_raices();
    for (_, v) in &campos {
        m.mundo.m.raiz(*v);
    }
    let d = m.mundo.crear_objeto(m.pila(R_OBJETO));
    m.mundo.m.raiz(d);
    let pos = marca + campos.len();
    let enumerable = Valor::booleano(m.es_enumerable_propia(arg(m, ll, 0), clave));
    let todos = campos.iter().map(|(n, _)| *n).chain(["enumerable", "configurable"]);
    for (i, nombre) in todos.enumerate() {
        let v = match i.cmp(&campos.len()) {
            core::cmp::Ordering::Less => m.pila(marca + i),
            core::cmp::Ordering::Equal => enumerable,
            core::cmp::Ordering::Greater => Valor::VERDADERO,
        };
        let c = m.mundo.cadenas.internar(nombre);
        m.mundo.escribir(m.pila(pos), c, v);
    }
    let d = m.pila(pos);
    m.mundo.m.soltar_raices(marca);
    Ok(d)
}

fn clave_de(m: &mut Motor, k: Valor) -> u32 {
    match Cadenas::id_de_valor(k) {
        Some(id) => id,
        None => {
            let s = m.texto(k);
            m.mundo.cadenas.internar(&s)
        }
    }
}

/// Los elementos con sus huecos ([`Valor::HUECO`]): para copiar arreglos (`slice`, `concat`...).
/// Lo que se devuelva a JavaScript desde aquí debe pasar por [`sin_hueco`].
fn elementos_crudos(m: &Motor, a: Valor) -> Vec<Valor> {
    let n = m.mundo.longitud(a).unwrap_or(0);
    (0..n).map(|i| m.mundo.leer_elemento_crudo(a, i).unwrap_or(Valor::INDEFINIDO)).collect()
}

/// Un hueco leído en crudo, como lo ve JavaScript.
fn sin_hueco(v: Valor) -> Valor {
    if v == Valor::HUECO { Valor::INDEFINIDO } else { v }
}

fn elementos(m: &Motor, a: Valor) -> Vec<Valor> {
    let n = m.mundo.longitud(a).unwrap_or(0);
    (0..n).map(|i| m.mundo.leer_elemento(a, i).unwrap_or(Valor::INDEFINIDO)).collect()
}

/// `f.apply(este, args)`: llama a `f` con los elementos de `args` como argumentos, abriendo su
/// marco como una llamada más (sin recursión de Rust). `Ok(true)`: hay un marco que ejecutar.
pub(crate) fn aplicar(m: &mut Motor, ll: &Llamada) -> Result<bool, Valor> {
    let (f, este, lista) = (m.pila(ll.este), arg(m, ll, 0), arg(m, ll, 1));
    let args = match m.mundo.clase(lista) {
        Some(Clase::Arreglo) => elementos(m, lista),
        _ if matches!(lista.tipo(), Tipo::Indefinido | Tipo::Nulo) => Vec::new(),
        _ => return Err(m.error("TypeError", "apply necesita un arreglo de argumentos")),
    };
    m.llamar_con(f, este, &args, ll.destino)
}

/// Métodos sin función: el arreglo está en `marca`.
fn arreglo(m: &mut Motor, id: u32, ll: &Llamada, marca: usize) -> Result<Valor, Valor> {
    let a = m.pila(ll.este);
    m.mundo.m.raiz(a);
    let lista = elementos_crudos(m, a);
    let n = lista.len();
    let nuevo = |m: &mut Motor, v: &[Valor]| m.mundo.crear_arreglo(v, m.pila(R_ARREGLO));
    Ok(match id {
        SLICE => {
            let (x, y) = (op::a_numero(&m.mundo, arg(m, ll, 0)), op::a_numero(&m.mundo, arg(m, ll, 1)));
            let inicio = relativo(x, n);
            let fin = if arg(m, ll, 1) == Valor::INDEFINIDO { n } else { relativo(y, n) };
            nuevo(m, if inicio < fin { &lista[inicio..fin] } else { &[] })
        }
        CONCAT => {
            let mut todo = lista;
            for i in 0..ll.n {
                let v = arg(m, ll, i);
                if m.mundo.clase(v) == Some(Clase::Arreglo) {
                    todo.extend(elementos_crudos(m, v))
                } else {
                    todo.push(v)
                }
            }
            nuevo(m, &todo)
        }
        REVERSE => {
            for (i, v) in lista.into_iter().rev().enumerate() {
                m.mundo.escribir_elemento(a, i as u32, v);
            }
            a
        }
        SHIFT => {
            let Some(&primero) = lista.first() else { return Ok(Valor::INDEFINIDO) };
            for (i, v) in lista.into_iter().enumerate().skip(1) {
                m.mundo.escribir_elemento(a, i as u32 - 1, v);
            }
            m.mundo.sacar(a);
            sin_hueco(primero)
        }
        UNSHIFT => {
            let mut todo: Vec<Valor> = (0..ll.n).map(|i| arg(m, ll, i)).collect();
            todo.extend(lista);
            for (i, v) in todo.iter().enumerate() {
                let a = m.pila(marca);
                if !m.mundo.escribir_elemento(a, i as u32, *v) {
                    return Err(m.error("RangeError", "arreglo demasiado largo"));
                }
            }
            Valor::entero(todo.len() as i32)
        }
        INCLUDES => {
            let buscado = arg(m, ll, 0);
            let igual = |e: &Valor| {
                let e = sin_hueco(*e);
                op::estricto(e, buscado) || (es_nan(e) && es_nan(buscado))
            };
            Valor::booleano(lista.iter().any(igual))
        }
        _ => return ordenar(m, ll, marca),
    })
}

fn es_nan(v: Valor) -> bool {
    v.como_numero().is_some_and(f64::is_nan)
}

/// `sort(cmp)`: inserción binaria en su sitio (estable). Raíces: arreglo, función, elemento.
fn ordenar(m: &mut Motor, ll: &Llamada, marca: usize) -> Result<Valor, Valor> {
    let cmp = arg(m, ll, 0);
    if cmp != Valor::INDEFINIDO && !matches!(m.mundo.clase(cmp), Some(Clase::Funcion | Clase::Nativa)) {
        return Err(m.error("TypeError", "la función de sort no es una función"));
    }
    m.mundo.m.raiz(cmp);
    m.mundo.m.raiz(Valor::INDEFINIDO);
    let n = m.mundo.longitud(m.pila(marca)).unwrap_or(0);
    for i in 1..n {
        let x = m.mundo.leer_elemento(m.pila(marca), i).unwrap_or(Valor::INDEFINIDO);
        m.mundo.m.pila_mut()[marca + 2] = x;
        let (mut bajo, mut alto) = (0, i);
        while bajo < alto {
            let medio = (bajo + alto) / 2;
            let y = m.mundo.leer_elemento(m.pila(marca), medio).unwrap_or(Valor::INDEFINIDO);
            if orden(m, marca, m.pila(marca + 2), y)? < 0.0 {
                alto = medio;
            } else {
                bajo = medio + 1;
            }
        }
        let a = m.pila(marca);
        for j in (bajo..i).rev() {
            let v = m.mundo.leer_elemento(a, j).unwrap_or(Valor::INDEFINIDO);
            m.mundo.escribir_elemento(a, j + 1, v);
        }
        m.mundo.escribir_elemento(a, bajo, m.pila(marca + 2));
    }
    Ok(m.pila(marca))
}

/// Negativo si `x` va antes que `y`. `undefined` siempre al final; sin función, por texto.
fn orden(m: &mut Motor, marca: usize, x: Valor, y: Valor) -> Result<f64, Valor> {
    match (x == Valor::INDEFINIDO, y == Valor::INDEFINIDO) {
        (true, true) => return Ok(0.0),
        (true, false) => return Ok(1.0),
        (false, true) => return Ok(-1.0),
        _ => {}
    }
    let cmp = m.pila(marca + 1);
    if cmp == Valor::INDEFINIDO {
        let (a, b) = (m.texto(x), m.texto(y));
        return Ok(match a.encode_utf16().cmp(b.encode_utf16()) {
            core::cmp::Ordering::Less => -1.0,
            core::cmp::Ordering::Equal => 0.0,
            core::cmp::Ordering::Greater => 1.0,
        });
    }
    let r = m.llamar_interno(cmp, Valor::INDEFINIDO, &[x, y])?;
    let n = m.a_numero(r)?;
    Ok(if n.is_nan() { 0.0 } else { n })
}

/// `forEach`, `map`, `filter`, `reduce`, `some`, `every`, `find`, `findIndex`.
/// Raíces desde `marca`: arreglo, función, `this` de la función, acumulador o resultado.
fn por_elemento(m: &mut Motor, id: u32, ll: &Llamada, marca: usize) -> Result<Valor, Valor> {
    let (a, f) = (m.pila(ll.este), arg(m, ll, 0));
    if !matches!(m.mundo.clase(f), Some(Clase::Funcion | Clase::Nativa)) {
        return Err(m.error("TypeError", "el argumento no es una función"));
    }
    let este = if id == REDUCE { Valor::INDEFINIDO } else { arg(m, ll, 1) };
    for v in [a, f, este, Valor::INDEFINIDO] {
        m.mundo.m.raiz(v);
    }
    let n = m.mundo.longitud(a).unwrap_or(0);
    let mut desde = 0;
    if matches!(id, MAP | FILTER) {
        let r = m.mundo.crear_arreglo(&[], m.pila(R_ARREGLO));
        m.mundo.m.pila_mut()[marca + 3] = r;
    } else if id == REDUCE {
        let inicial = if ll.n >= 2 { Some(arg(m, ll, 1)) } else { None };
        // Sin valor inicial: el primer elemento que no sea un hueco.
        let primero = (0..n).find(|&i| !m.mundo.es_hueco(a, i));
        let Some(inicial) =
            inicial.or_else(|| primero.map(|i| m.mundo.leer_elemento(a, i).unwrap_or(Valor::INDEFINIDO)))
        else {
            return Err(m.error("TypeError", "reduce de un arreglo vacío sin valor inicial"));
        };
        desde = if ll.n < 2 { primero.map_or(n, |i| i + 1) } else { 0 };
        m.mundo.m.pila_mut()[marca + 3] = inicial;
    }
    for i in desde..n {
        let (a, f, este) = (m.pila(marca), m.pila(marca + 1), m.pila(marca + 2));
        // Los huecos no se visitan (salvo en `find`, que los ve como `undefined`); `map` los deja.
        if !matches!(id, FIND | FIND_INDEX) && m.mundo.es_hueco(a, i) {
            if id == MAP && !m.mundo.escribir_elemento(m.pila(marca + 3), i, Valor::HUECO) {
                return Err(m.error("RangeError", "arreglo demasiado largo"));
            }
            continue;
        }
        let e = m.mundo.leer_elemento(a, i).unwrap_or(Valor::INDEFINIDO);
        let indice = Valor::entero(i as i32);
        let r = if id == REDUCE {
            m.llamar_interno(f, este, &[m.pila(marca + 3), e, indice, a])?
        } else {
            m.llamar_interno(f, este, &[e, indice, a])?
        };
        let si = op::a_booleano(&m.mundo, r);
        // La llamada pudo mover el elemento: se vuelve a leer.
        let e = m.mundo.leer_elemento(m.pila(marca), i).unwrap_or(Valor::INDEFINIDO);
        match id {
            MAP => {
                if !m.mundo.escribir_elemento(m.pila(marca + 3), i, r) {
                    return Err(m.error("RangeError", "arreglo demasiado largo"));
                }
            }
            FILTER => {
                if si && m.mundo.empujar(m.pila(marca + 3), e).is_none() {
                    return Err(m.error("RangeError", "arreglo demasiado largo"));
                }
            }
            REDUCE => m.mundo.m.pila_mut()[marca + 3] = r,
            SOME if si => return Ok(Valor::VERDADERO),
            EVERY if !si => return Ok(Valor::FALSO),
            FIND if si => return Ok(e),
            FIND_INDEX if si => return Ok(indice),
            _ => {}
        }
    }
    Ok(match id {
        MAP | FILTER | REDUCE => m.pila(marca + 3),
        SOME => Valor::FALSO,
        EVERY => Valor::VERDADERO,
        FIND_INDEX => Valor::entero(-1),
        _ => Valor::INDEFINIDO,
    })
}
