//! `Date` (lo justo para medir tiempo) y `Error`.
//!
//! Una fecha es un objeto con su número de milisegundos en la propiedad interna `[[Valor]]`,
//! así que `new Date() - inicio` da los milisegundos transcurridos. El reloj lo pone el
//! anfitrión con [`Motor::con_reloj`] (el motor es `no_std`).

use super::{arg, constructor, definir_nativa, definir_texto};
use crate::llamadas::Llamada;
use crate::{Motor, R_ERROR, R_FECHA, operaciones as op};
use quimera_objetos::Valor;

const DATE: u32 = 300;
const DATE_NOW: u32 = 301;
const GET_TIME: u32 = 302;
const ERROR: u32 = 310;
const ERROR_TO_STRING: u32 = 311;

pub(super) fn preparar(m: &mut Motor) {
    let date = constructor(m, "Date", DATE, R_FECHA);
    definir_nativa(m, date, "now", DATE_NOW);
    definir_nativa(m, R_FECHA, "getTime", GET_TIME);
    definir_nativa(m, R_FECHA, "valueOf", GET_TIME);
    constructor(m, "Error", ERROR, R_ERROR);
    definir_texto(m, R_ERROR, "name", "Error");
    definir_texto(m, R_ERROR, "message", "");
    definir_nativa(m, R_ERROR, "toString", ERROR_TO_STRING);
}

/// Las nativas de este grupo que admiten `new`.
pub(super) fn es_constructor(id: u32) -> bool {
    matches!(id, DATE | ERROR)
}

/// Un objeto nuevo con prototipo `proto` y la propiedad `clave = valor`, dejado en el destino
/// de la llamada (que es una raíz) antes de escribir: escribir puede mover el objeto.
fn objeto_con(m: &mut Motor, ll: &Llamada, proto: usize, clave: &str, valor: Valor) -> Valor {
    let o = m.mundo.crear_objeto(m.pila(proto));
    m.mundo.m.pila_mut()[ll.destino] = o;
    let k = m.mundo.cadenas.internar(clave);
    let marca = m.mundo.m.marca_raices();
    m.mundo.m.raiz(valor);
    let (o, v) = (m.pila(ll.destino), m.pila(marca));
    m.mundo.escribir(o, k, v);
    m.mundo.m.soltar_raices(marca);
    m.pila(ll.destino)
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    Ok(match id {
        DATE => {
            // `new Date(ms)` con un número; sin argumentos, ahora. (Fechas en texto, todavía no.)
            let ms = match arg(m, ll, 0) {
                v if ll.n > 0 && v.como_numero().is_some() => op::a_numero(&m.mundo, v),
                _ => m.ahora(),
            };
            objeto_con(m, ll, R_FECHA, op::CLAVE_VALOR, Valor::numero(ms))
        }
        DATE_NOW => Valor::numero(m.ahora()),
        GET_TIME => match op::valor_primitivo(&m.mundo, m.pila(ll.este)) {
            Some(v) => v,
            None => return Err(m.error("TypeError", "getTime necesita una fecha")),
        },
        ERROR => {
            let mensaje = arg(m, ll, 0);
            let texto = (mensaje != Valor::INDEFINIDO).then(|| op::a_texto(&m.mundo, mensaje));
            m.objeto_error(m.pila(R_ERROR), "Error", texto.as_deref())
        }
        ERROR_TO_STRING => {
            let texto = m.texto(m.pila(ll.este));
            m.mundo.cadena(&texto)
        }
        _ => Valor::INDEFINIDO,
    })
}
