//! Generadores (`function*`, `yield`): funciones que se pausan y se reanudan.
//!
//! - **Al llamar** a un `function*` se abre su marco como en cualquier llamada (parámetros,
//!   `this`, `arguments`) pero, antes de ejecutar nada, se guarda en un objeto generador y se
//!   cierra. Ese objeto es lo que devuelve la llamada.
//! - **`yield v`** copia los registros del marco (y sus `try` abiertos) al generador, lo cierra
//!   y devuelve `{value: v, done: false}` a quien llamó a `next`.
//! - **`next(x)`** vuelve a poner el marco en la pila de raíces tal como estaba, deja `x` como
//!   resultado del `yield` y sigue: es una llamada más, sin recursión de Rust (como `call`).
//!   `throw(e)` reanuda lanzando `e` donde estaba el `yield` (sus `try` lo pueden capturar).
//! - **`return v`** dentro del generador lo termina con `{value: v, done: true}`.
//!
//! El generador guarda su estado en ranuras internas (U+0001): los registros en un arreglo, el
//! punto donde sigue, el registro que recibe lo enviado y sus `try`. Mientras corre, el propio
//! generador va en el hueco 3 de la cabecera del marco (el de `new`, que un generador no usa).
//!
//! Falta: `return()` y `throw()` sobre un generador pausado dentro de un `try` con `finally` no
//! ejecutan el `finally`; `yield*` no reenvía `throw`/`return` al iterador interior.

use crate::llamadas::Llamada;
use crate::{CABECERA, Manejador, Marco, Motor, R_GLOBAL};
use alloc::vec::Vec;
use quimera_objetos::Valor;

const NUEVO: i32 = 0;
const PAUSADO: i32 = 1;
const CORRIENDO: i32 = 2;
const TERMINADO: i32 = 3;

/// Cómo se reanuda: `next(x)`, `return(x)` o `throw(x)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modo {
    Siguiente,
    Retornar,
    Lanzar,
}

impl Motor {
    fn ranura(&mut self, nombre: &str) -> u32 {
        self.mundo.cadenas.internar(nombre)
    }

    fn leer_ranura(&mut self, generador: Valor, nombre: &str) -> Valor {
        let k = self.ranura(nombre);
        self.mundo.leer_propia(generador, k).unwrap_or(Valor::INDEFINIDO)
    }

    /// Escribe una ranura del generador que está en la posición `pos` de la pila.
    fn poner_ranura(&mut self, pos: usize, nombre: &str, v: Valor) {
        let k = self.ranura(nombre);
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        let (g, v) = (self.pila(pos), self.pila(marca));
        self.mundo.escribir(g, k, v);
        self.mundo.m.soltar_raices(marca);
    }

    /// `{value: v, done}`.
    pub(crate) fn resultado_iteracion(&mut self, v: Valor, done: bool) -> Valor {
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        let o = self.mundo.crear_objeto(self.pila(crate::R_OBJETO));
        self.mundo.m.raiz(o);
        let (kv, kd) = (self.ranura("value"), self.ranura("done"));
        let (o, v) = (self.pila(marca + 1), self.pila(marca));
        self.mundo.escribir(o, kv, v);
        let o = self.pila(marca + 1);
        self.mundo.escribir(o, kd, Valor::booleano(done));
        let o = self.pila(marca + 1);
        self.mundo.m.soltar_raices(marca);
        o
    }

    /// Guarda el marco de arriba en su generador (que está en el hueco 3 de su cabecera): los
    /// registros, dónde sigue, qué registro recibe lo enviado y sus `try`.
    fn guardar_marco(&mut self, pc: usize, dst: i32, estado: i32) {
        let Some(marco) = self.marcos.last().copied() else { return };
        let base_marco = marco.base - CABECERA;
        let tam = self.funciones.get(marco.funcion as usize).map_or(0, |c| (c.f.registros.max(c.f.params)) as usize);
        let valores: Vec<Valor> = self.mundo.m.pila()[base_marco..base_marco + CABECERA + tam].to_vec();
        let regs = self.mundo.crear_arreglo(&valores, self.pila(crate::R_ARREGLO));
        let gen_pos = base_marco + 3;
        self.poner_ranura(gen_pos, "\u{1}regs", regs);
        self.poner_ranura(gen_pos, "\u{1}fn", Valor::entero(marco.funcion as i32));
        self.poner_ranura(gen_pos, "\u{1}pc", Valor::entero(pc as i32));
        self.poner_ranura(gen_pos, "\u{1}dst", Valor::entero(dst));
        self.poner_ranura(gen_pos, "\u{1}estado", Valor::entero(estado));
        // Los `try` abiertos en este marco se van con él.
        let n = self.marcos.len();
        let mut capturas = Vec::new();
        while self.manejadores.last().is_some_and(|h| h.marcos >= n) {
            if let Some(h) = self.manejadores.pop() {
                capturas.push(Valor::entero(h.captura as i32));
            }
        }
        capturas.reverse();
        let lista = self.mundo.crear_arreglo(&capturas, self.pila(crate::R_ARREGLO));
        self.poner_ranura(gen_pos, "\u{1}try", lista);
    }

    /// Cierra el marco de arriba (ya guardado) y deja `v` en el destino de quien lo llamó.
    fn soltar_marco(&mut self, v: Valor) {
        let Some(marco) = self.marcos.pop() else { return };
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        let v = self.pila(marca);
        self.mundo.m.soltar_raices(marco.soltar);
        if let Some(d) = marco.destino
            && let Some(hueco) = self.mundo.m.pila_mut().get_mut(d)
        {
            *hueco = v;
        }
    }

    /// Recién abierto el marco de un `function*`: se convierte en un generador sin empezar.
    pub(crate) fn crear_generador(&mut self) {
        let Some(marco) = self.marcos.last().copied() else { return };
        let gen_pos = marco.base - CABECERA + 3;
        let clave = self.ranura("__quimera_generador");
        let proto = self.mundo.leer(self.pila(R_GLOBAL), clave).unwrap_or(Valor::NULO);
        let g = self.mundo.crear_objeto(proto);
        self.mundo.m.pila_mut()[gen_pos] = g;
        self.guardar_marco(0, -1, NUEVO);
        let g = self.pila(gen_pos);
        self.soltar_marco(g);
    }

    /// `yield v` en el marco de arriba: se guarda, se cierra y devuelve `{value: v, done: false}`.
    pub(crate) fn suspender(&mut self, v: Valor, dst: u16, siguiente: usize) {
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        self.guardar_marco(siguiente, i32::from(dst), PAUSADO);
        let v = self.pila(marca);
        let r = self.resultado_iteracion(v, false);
        self.soltar_marco(r);
    }

    /// El marco de un generador termina (con `return` o al acabar): queda terminado y su
    /// resultado es `{value: v, done: true}`.
    pub(crate) fn terminar_generador(&mut self, marco: &Marco, v: Valor) -> Valor {
        let gen_pos = marco.base - CABECERA + 3;
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        self.poner_ranura(gen_pos, "\u{1}estado", Valor::entero(TERMINADO));
        self.poner_ranura(gen_pos, "\u{1}regs", Valor::INDEFINIDO);
        let v = self.pila(marca);
        let r = self.resultado_iteracion(v, true);
        self.mundo.m.soltar_raices(marca);
        r
    }

    /// Un marco de generador deshecho por una excepción: el generador queda terminado.
    pub(crate) fn abortar_generador(&mut self, marco: &Marco) {
        let gen_pos = marco.base - CABECERA + 3;
        self.poner_ranura(gen_pos, "\u{1}estado", Valor::entero(TERMINADO));
    }
}

/// `generador.next(x)`, `generador.return(x)`, `generador.throw(x)`. `Ok(true)`: el marco del generador está
/// arriba y hay que ejecutarlo.
pub(crate) fn reanudar(m: &mut Motor, ll: &Llamada, modo: Modo) -> Result<bool, Valor> {
    let generador = m.pila(ll.este);
    let enviado = if ll.n > 0 { m.pila(ll.args) } else { Valor::INDEFINIDO };
    let estado = m.leer_ranura(generador, "\u{1}estado").como_entero();
    let Some(estado) = estado.filter(|_| m.mundo.es_objeto(generador)) else {
        return Err(m.error("TypeError", "next/return/throw sobre algo que no es un generador"));
    };
    let terminar = |m: &mut Motor, enviado: Valor| -> Result<bool, Valor> {
        m.mundo.m.pila_mut()[ll.destino] = m.pila(ll.este);
        if estado != TERMINADO {
            m.poner_ranura(ll.destino, "\u{1}estado", Valor::entero(TERMINADO));
            m.poner_ranura(ll.destino, "\u{1}regs", Valor::INDEFINIDO);
        }
        match modo {
            Modo::Lanzar => Err(enviado),
            Modo::Retornar => {
                let r = m.resultado_iteracion(enviado, true);
                m.mundo.m.pila_mut()[ll.destino] = r;
                Ok(false)
            }
            Modo::Siguiente => {
                let r = m.resultado_iteracion(Valor::INDEFINIDO, true);
                m.mundo.m.pila_mut()[ll.destino] = r;
                Ok(false)
            }
        }
    };
    match estado {
        CORRIENDO => return Err(m.error("TypeError", "el generador ya se está ejecutando")),
        TERMINADO => return terminar(m, enviado),
        NUEVO if modo != Modo::Siguiente => return terminar(m, enviado),
        _ if modo == Modo::Retornar => return terminar(m, enviado),
        _ => {}
    }
    // Volver a poner el marco tal como estaba.
    let regs = m.leer_ranura(generador, "\u{1}regs");
    let funcion = m.leer_ranura(generador, "\u{1}fn").como_entero().unwrap_or(0) as u32;
    let pc = m.leer_ranura(generador, "\u{1}pc").como_entero().unwrap_or(0) as usize;
    let dst = m.leer_ranura(generador, "\u{1}dst").como_entero().unwrap_or(-1);
    let capturas = m.leer_ranura(generador, "\u{1}try");
    let n = m.mundo.longitud(regs).unwrap_or(0) as usize;
    let base_marco = m.mundo.m.abrir_raices(n, Valor::INDEFINIDO);
    for i in 0..n {
        let v = m.mundo.leer_elemento(regs, i as u32).unwrap_or(Valor::INDEFINIDO);
        m.mundo.m.pila_mut()[base_marco + i] = v;
    }
    let base = base_marco + CABECERA;
    m.marcos.push(Marco {
        funcion,
        pc,
        base,
        destino: Some(ll.destino),
        construir: false,
        soltar: base_marco,
        generador: true,
    });
    let profundidad = m.marcos.len();
    for i in 0..m.mundo.longitud(capturas).unwrap_or(0) {
        let c = m.mundo.leer_elemento(capturas, i).and_then(|c| c.como_entero()).unwrap_or(0);
        m.manejadores.push(Manejador { marcos: profundidad, captura: c as usize });
    }
    m.poner_ranura(base_marco + 3, "\u{1}estado", Valor::entero(CORRIENDO));
    if dst >= 0 {
        m.mundo.m.pila_mut()[base + dst as usize] = enviado;
    }
    if modo == Modo::Lanzar {
        // Se lanza dentro del generador: sus `try` (ya puestos) la pueden capturar.
        return Err(enviado);
    }
    Ok(true)
}
