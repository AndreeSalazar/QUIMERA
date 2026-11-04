//! Las piezas nativas de `ArrayBuffer`, los arreglos tipados, `DataView`, `TextEncoder` y
//! `TextDecoder` (la parte de JavaScript está en `binarios.js`).
//!
//! Son funciones internas (`__quimera_bytes`...): `binarios.js` les pone encima la API de la web.
//! Lo que mueve muchos bytes (copiar, rellenar, UTF-8) va aquí, sin un bucle de JavaScript.

use super::{arg, definir_nativa};
use crate::llamadas::Llamada;
use crate::{Motor, R_GLOBAL, operaciones as op};
use alloc::string::String;
use quimera_objetos::{Clase, Especie, Valor};

const BYTES: u32 = 800;
const BLARGO: u32 = 801;
const VISTA: u32 = 802;
const VINFO: u32 = 803;
const BCOPIAR: u32 = 804;
const VSET: u32 = 805;
const VRELLENAR: u32 = 806;
const DV_LEER: u32 = 807;
const DV_ESCRIBIR: u32 = 808;
const UTF8: u32 = 809;
const TEXTO_UTF8: u32 = 810;

pub(super) fn preparar(m: &mut Motor) {
    for (nombre, id) in [
        ("__quimera_bytes", BYTES),
        ("__quimera_blargo", BLARGO),
        ("__quimera_vista", VISTA),
        ("__quimera_vinfo", VINFO),
        ("__quimera_bcopiar", BCOPIAR),
        ("__quimera_vset", VSET),
        ("__quimera_vrellenar", VRELLENAR),
        ("__quimera_dv", DV_LEER),
        ("__quimera_dvset", DV_ESCRIBIR),
        ("__quimera_utf8", UTF8),
        ("__quimera_texto_utf8", TEXTO_UTF8),
    ] {
        definir_nativa(m, R_GLOBAL, nombre, id);
    }
}

/// Un argumento como entero no negativo (lo que no es número, 0).
fn natural(m: &Motor, ll: &Llamada, i: usize) -> usize {
    let x = op::a_numero(&m.mundo, arg(m, ll, i));
    if x.is_finite() && x > 0.0 { x as usize } else { 0 }
}

fn especie(m: &Motor, ll: &Llamada, i: usize) -> Option<Especie> {
    Especie::desde(u8::try_from(natural(m, ll, i)).ok()?)
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    match id {
        BYTES => {
            let (n, proto) = (natural(m, ll, 0), arg(m, ll, 1));
            m.mundo.crear_bytes(n, proto).ok_or_else(|| m.error("RangeError", "ArrayBuffer demasiado grande"))
        }
        BLARGO => Ok(m.mundo.bytes_largo(arg(m, ll, 0)).map_or(Valor::INDEFINIDO, |n| Valor::entero(n as i32))),
        VISTA => {
            let (b, desde, largo, proto) = (arg(m, ll, 0), natural(m, ll, 1), natural(m, ll, 2), arg(m, ll, 4));
            let Some(e) = especie(m, ll, 3) else { return Err(m.error("TypeError", "tipo de arreglo no válido")) };
            m.mundo
                .crear_vista(b, desde, largo, e, proto)
                .ok_or_else(|| m.error("RangeError", "la vista se sale de su ArrayBuffer"))
        }
        VINFO => Ok(match m.mundo.vista(arg(m, ll, 0)) {
            Some(v) => match natural(m, ll, 1) {
                0 => v.bufer,
                1 => Valor::entero(v.desde as i32),
                2 => Valor::entero(v.largo as i32),
                _ => Valor::entero(v.especie as i32),
            },
            None => Valor::INDEFINIDO,
        }),
        BCOPIAR => {
            let (o, desde, d, a, n) =
                (arg(m, ll, 0), natural(m, ll, 1), arg(m, ll, 2), natural(m, ll, 3), natural(m, ll, 4));
            m.mundo.copiar_bytes(o, desde, d, a, n);
            Ok(Valor::INDEFINIDO)
        }
        VSET => poner(m, ll),
        VRELLENAR => {
            let v = arg(m, ll, 0);
            let Some(x) = m.mundo.vista(v) else { return Ok(v) };
            let valor = op::a_numero(&m.mundo, arg(m, ll, 1));
            let (a, b) = (natural(m, ll, 2).min(x.largo), natural(m, ll, 3).min(x.largo));
            let n = x.especie.tam();
            let uno = x.especie.codificar(valor);
            let patron: alloc::vec::Vec<u8> = (a..b).flat_map(|_| uno[..n].iter().copied()).collect();
            m.mundo.escribir_bytes(x.bufer, x.desde + a * n, &patron);
            Ok(v)
        }
        DV_LEER | DV_ESCRIBIR => dataview(m, id, ll),
        UTF8 => {
            let s = m.texto(arg(m, ll, 0));
            let proto = arg(m, ll, 1);
            m.mundo.crear_bytes_con(s.as_bytes(), proto).ok_or_else(|| m.error("RangeError", "texto demasiado grande"))
        }
        TEXTO_UTF8 => {
            let (b, desde, n) = (arg(m, ll, 0), natural(m, ll, 1), natural(m, ll, 2));
            let bytes = m.mundo.leer_bytes(b, desde, n);
            // Lo que no es UTF-8 válido sale como U+FFFD, como en los navegadores.
            let s = String::from_utf8_lossy(&bytes);
            Ok(m.mundo.cadena(&s))
        }
        _ => Ok(Valor::INDEFINIDO),
    }
}

/// `destino.set(origen, desde)`: de otra vista (si es de la misma especie, bytes tal cual) o de
/// un arreglo (cada elemento convertido).
fn poner(m: &mut Motor, ll: &Llamada) -> Result<Valor, Valor> {
    let (destino, origen, desde) = (arg(m, ll, 0), arg(m, ll, 1), natural(m, ll, 2));
    let Some(d) = m.mundo.vista(destino) else { return Err(m.error("TypeError", "set necesita un arreglo tipado")) };
    let n = m.mundo.longitud(origen).unwrap_or(0) as usize;
    if desde.saturating_add(n) > d.largo {
        return Err(m.error("RangeError", "set se sale del arreglo"));
    }
    if let Some(o) = m.mundo.vista(origen)
        && mismos_bytes(o.especie, d.especie)
    {
        let t = d.especie.tam();
        m.mundo.copiar_bytes(o.bufer, o.desde, d.bufer, d.desde + desde * t, o.largo * t);
        return Ok(Valor::INDEFINIDO);
    }
    // Primero se leen todos (el origen puede ser una vista del mismo búfer que se pisa).
    let valores: alloc::vec::Vec<f64> = (0..n as u32)
        .map(|i| m.mundo.leer_elemento(origen, i).map_or(f64::NAN, |v| op::a_numero(&m.mundo, v)))
        .collect();
    for (i, x) in valores.into_iter().enumerate() {
        m.mundo.escribir_en_vista(destino, desde + i, x);
    }
    Ok(Valor::INDEFINIDO)
}

/// Si copiar los bytes tal cual da lo mismo que convertir cada elemento: la misma especie, o
/// enteros del mismo tamaño con y sin signo (`ToInt8(200)` = -56 son los mismos bytes). Hacia
/// `Uint8ClampedArray` solo desde bytes sin signo (los negativos se recortan a 0).
fn mismos_bytes(o: Especie, d: Especie) -> bool {
    use Especie::*;
    o == d
        || matches!(
            (o, d),
            (Int8 | Uint8 | Uint8Recortado, Int8 | Uint8)
                | (Uint8, Uint8Recortado)
                | (Int16 | Uint16, Int16 | Uint16)
                | (Int32 | Uint32, Int32 | Uint32)
        )
}

/// `DataView`: leer o escribir un número de `especie` en la posición `pos` de un búfer, en
/// *little endian* (`le`) o *big endian*.
fn dataview(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    let (b, pos) = (arg(m, ll, 0), natural(m, ll, 1));
    let Some(e) = especie(m, ll, 2) else { return Err(m.error("TypeError", "tipo de DataView no válido")) };
    let le = op::a_booleano(&m.mundo, arg(m, ll, 3));
    let n = e.tam();
    let total = m.mundo.bytes_largo(b).unwrap_or(0);
    if m.mundo.clase(b) != Some(Clase::Bytes) || pos.saturating_add(n) > total {
        return Err(m.error("RangeError", "fuera del DataView"));
    }
    if id == DV_LEER {
        let mut bytes = m.mundo.leer_bytes(b, pos, n);
        if !le {
            bytes.reverse();
        }
        let mut c = [0u8; 8];
        c[..n].copy_from_slice(&bytes);
        return Ok(Valor::numero(e.decodificar(&c)));
    }
    let x = op::a_numero(&m.mundo, arg(m, ll, 4));
    let mut c = e.codificar(x);
    if !le {
        c[..n].reverse();
    }
    m.mundo.escribir_bytes(b, pos, &c[..n]);
    Ok(Valor::INDEFINIDO)
}
