//! El anfitrión: quien usa el motor (el navegador) y le da sus propias funciones y objetos.
//!
//! - **Funciones del anfitrión**: nativas cuyo número es del anfitrión; al llamarlas, la máquina
//!   le pasa el número, `this` y los argumentos ([`Anfitrion::llamar`]).
//! - **Objetos del anfitrión** (los nodos del DOM): objetos con un número propio (`dato`, el del
//!   nodo). Al leer o escribir una propiedad suya, primero se pregunta al anfitrión (`textContent`,
//!   `id`...); si no la conoce, es una propiedad normal (sus métodos están en su prototipo). Las
//!   cachés en línea nunca los guardan: cada acceso pasa por el anfitrión.
//! - **Raíz del anfitrión**: un objeto que el GC mantiene vivo, donde el anfitrión guarda lo que
//!   tiene que durar (prototipos, los objetos de cada nodo, funciones de eventos). El montón
//!   mueve los objetos: un [`Valor`] de objeto guardado en Rust deja de valer tras asignar memoria.
//!
//! El anfitrión va en un `Rc` y sus métodos toman `&self`: puede volver a llamar a JavaScript
//! (un evento) y JavaScript a él, sin quedarse sin anfitrión a mitad.

use crate::{ErrorJs, Motor, R_ANFITRION, R_ARREGLO, R_FUNCION, R_GLOBAL, R_OBJETO};
use alloc::rc::Rc;
use quimera_objetos::Valor;

/// Desde aquí, los números de las nativas son del anfitrión.
pub(crate) const GRUPO_ANFITRION: u32 = 1 << 24;

/// Dónde están `this` y los argumentos de una llamada al anfitrión (en la pila de raíces).
#[derive(Debug, Clone, Copy)]
pub struct Argumentos {
    pub(crate) este: usize,
    pub(crate) args: usize,
    /// Cuántos argumentos llegaron.
    pub n: usize,
}

impl Argumentos {
    /// `this` (leído de la pila: sigue valiendo aunque se haya asignado memoria).
    pub fn este(&self, m: &Motor) -> Valor {
        m.pila(self.este)
    }

    /// El argumento `i` (`undefined` si no llegó).
    pub fn arg(&self, m: &Motor, i: usize) -> Valor {
        if i < self.n { m.pila(self.args + i) } else { Valor::INDEFINIDO }
    }
}

/// Lo que el anfitrión da al motor. Un `Err` es una excepción de JavaScript (su valor).
pub trait Anfitrion {
    /// Llama a la función del anfitrión número `id`.
    fn llamar(&self, m: &mut Motor, id: u32, args: &Argumentos) -> Result<Valor, Valor>;
    /// Lee `clave` (un id de cadena) de un objeto del anfitrión. `None`: propiedad normal.
    fn leer(&self, m: &mut Motor, dato: u32, clave: u32) -> Option<Result<Valor, Valor>>;
    /// Escribe `clave` en un objeto del anfitrión. `None`: propiedad normal. `v` deja de valer
    /// si se asigna memoria antes de guardarlo.
    fn escribir(&self, m: &mut Motor, dato: u32, clave: u32, v: Valor) -> Option<Result<(), Valor>>;
}

impl Motor {
    /// Pone el anfitrión.
    pub fn poner_anfitrion(&mut self, a: Rc<dyn Anfitrion>) {
        self.anfitrion = Some(a);
    }

    pub(crate) fn llamar_anfitrion(&mut self, id: u32, args: &Argumentos) -> Result<Valor, Valor> {
        match self.anfitrion.clone() {
            Some(a) => a.llamar(self, id - GRUPO_ANFITRION, args),
            None => Err(self.error("TypeError", "función del anfitrión sin anfitrión")),
        }
    }

    pub(crate) fn leer_anfitrion(&mut self, dato: u32, clave: u32) -> Option<Result<Valor, Valor>> {
        self.anfitrion.clone()?.leer(self, dato, clave)
    }

    pub(crate) fn escribir_anfitrion(&mut self, dato: u32, clave: u32, v: Valor) -> Option<Result<(), Valor>> {
        self.anfitrion.clone()?.escribir(self, dato, clave, v)
    }

    /// El objeto global.
    pub fn global(&self) -> Valor {
        self.fija(R_GLOBAL)
    }

    /// La raíz del anfitrión (un objeto que el GC mantiene vivo).
    pub fn raiz_anfitrion(&self) -> Valor {
        self.fija(R_ANFITRION)
    }

    /// `Object.prototype`.
    pub fn proto_objeto(&self) -> Valor {
        self.fija(R_OBJETO)
    }

    /// Una función del anfitrión con su número.
    pub fn crear_funcion_anfitrion(&mut self, id: u32) -> Valor {
        let proto = self.fija(R_FUNCION);
        self.mundo.crear_nativa(GRUPO_ANFITRION + id, proto)
    }

    /// El `name` de la función del anfitrión `id` (`addEventListener`...).
    pub fn nombrar_funcion_anfitrion(&mut self, id: u32, nombre: &str) {
        let a = self.mundo.cadenas.internar(nombre);
        self.nombres_nativas.insert(GRUPO_ANFITRION + id, a);
    }

    /// Un objeto del anfitrión con su número y su prototipo.
    pub fn crear_objeto_anfitrion(&mut self, dato: u32, proto: Valor) -> Valor {
        self.mundo.crear_anfitrion(dato, proto)
    }

    /// El número de un objeto del anfitrión.
    pub fn dato_anfitrion(&self, v: Valor) -> Option<u32> {
        self.mundo.anfitrion(v)
    }

    /// Un objeto vacío (`{}`).
    pub fn crear_objeto(&mut self) -> Valor {
        let proto = self.fija(R_OBJETO);
        self.mundo.crear_objeto(proto)
    }

    /// Un arreglo con estos valores (que no deben ser objetos movibles a medio crear).
    pub fn crear_arreglo(&mut self, valores: &[Valor]) -> Valor {
        let proto = self.fija(R_ARREGLO);
        self.mundo.crear_arreglo(valores, proto)
    }

    /// `obj.nombre = v`. Puede asignar memoria.
    pub fn definir(&mut self, obj: Valor, nombre: &str, v: Valor) {
        let clave = self.mundo.cadenas.internar(nombre);
        self.mundo.escribir(obj, clave, v);
    }

    /// `obj.nombre` (propia o heredada; `undefined` si no está).
    pub fn leer_nombre(&mut self, obj: Valor, nombre: &str) -> Valor {
        let clave = self.mundo.cadenas.internar(nombre);
        self.mundo.leer(obj, clave).unwrap_or(Valor::INDEFINIDO)
    }

    /// Una cadena.
    pub fn cadena(&mut self, texto: &str) -> Valor {
        self.mundo.cadena(texto)
    }

    /// Una excepción `TypeError` con este mensaje (para devolverla como `Err`).
    pub fn error_de_tipo(&mut self, mensaje: &str) -> Valor {
        self.error("TypeError", mensaje)
    }

    /// Llama a una función de JavaScript desde fuera (un evento, un temporizador).
    pub fn llamar_funcion(&mut self, f: Valor, este: Valor, args: &[Valor]) -> Result<Valor, ErrorJs> {
        self.llamar_valor(f, este, args)
    }

    /// Si `v` se puede llamar.
    pub fn es_funcion(&self, v: Valor) -> bool {
        matches!(self.mundo.clase(v), Some(quimera_objetos::Clase::Funcion | quimera_objetos::Clase::Nativa))
    }

    /// Mete `v` en la pila de raíces (el GC lo mantiene vivo y lo actualiza si se mueve).
    /// Devuelve su posición, para leerlo con [`Motor::de_pila`] y soltarlo con [`Motor::soltar_pila`].
    pub fn apilar(&mut self, v: Valor) -> usize {
        self.mundo.m.raiz(v);
        self.mundo.m.marca_raices() - 1
    }

    /// Lo que hay en la posición `i` de la pila de raíces.
    pub fn de_pila(&self, i: usize) -> Valor {
        self.pila(i)
    }

    /// Suelta la pila de raíces desde la posición `desde`.
    pub fn soltar_pila(&mut self, desde: usize) {
        self.mundo.m.soltar_raices(desde);
    }

    /// Las propiedades que el anfitrión puso en el objeto global y en los prototipos dejan de
    /// salir en `for-in`, como las de la biblioteca.
    pub fn fijar_biblioteca(&mut self) {
        self.fijar_claves_ocultas();
    }
}
