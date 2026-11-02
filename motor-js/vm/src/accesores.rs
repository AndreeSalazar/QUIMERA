//! Accesores: propiedades con `get` y `set` (`get x() {}` en clases y objetos,
//! `Object.defineProperty(o, k, {get, set})`).
//!
//! - **Dónde van.** En la forma del objeto, con la clave marcada (`clave | ACCESOR`), y en su
//!   hueco un arreglo `[get, set]`. Así una propiedad de datos `x` y un accesor `x` no se
//!   confunden, y la forma cambia al añadir un accesor: las guardas de las cachés lo notan.
//! - **Las cachés nunca los guardan.** La de primer nivel solo encuentra claves sin marcar. Leer
//!   o escribir mira si hay un accesor en la cadena *antes* de aprender nada. Los stubs de
//!   «añadir propiedad» guardan las formas de la cadena de prototipos: un `set` nuevo en uno de
//!   ellos cambia su forma y el stub falla solo. (Antes, definir un accesor vaciaba todas las
//!   cachés, y un módulo con miles de `export` —cada uno es un getter— tardaba segundos.)
//! - **Solo se buscan si pueden existir.** Un filtro de bits apunta las claves que tienen algún
//!   accesor en algún objeto (`filtro_accesores`): leer `x.foo` solo busca un accesor si alguien
//!   definió uno llamado `foo` (o una clave que cae en el mismo bit). Antes era un solo
//!   interruptor, y el primer getter de la biblioteca o del DOM hacía buscar en cada acceso que
//!   no acertaba la caché: un 5 % menos en V8.

use crate::Motor;
use quimera_objetos::Valor;

/// El bit que marca la clave de un accesor en una forma.
pub(crate) const ACCESOR: u32 = 1 << 31;

/// Palabras de 64 bits del filtro de claves con accesor (4096 bits).
pub(crate) const N_FILTRO: usize = 64;

impl Motor {
    /// El par `[get, set]` del accesor `clave` en `obj` o su cadena de prototipos. `None` si no
    /// hay, o si antes aparece una propiedad de datos con ese nombre (la tapa).
    pub(crate) fn buscar_accesor(&self, obj: Valor, clave: u32) -> Option<Valor> {
        if clave & ACCESOR != 0 || !self.quizas_accesor(clave) {
            return None;
        }
        let mut actual = obj;
        for _ in 0..10_000 {
            let forma = self.mundo.forma(actual)?;
            if self.mundo.formas.buscar(forma, clave).is_some() {
                return None;
            }
            if let Some(h) = self.mundo.formas.buscar(forma, clave | ACCESOR) {
                return self.mundo.leer_hueco(actual, h);
            }
            actual = self.mundo.prototipo(actual);
            if !self.mundo.es_objeto(actual) {
                return None;
            }
        }
        None
    }

    /// El par `[get, set]` de un accesor propio de `obj`.
    pub(crate) fn accesor_propio(&self, obj: Valor, clave: u32) -> Option<Valor> {
        let h = self.mundo.formas.buscar(self.mundo.forma(obj)?, clave | ACCESOR)?;
        self.mundo.leer_hueco(obj, h)
    }

    /// Llama al `get` del par con `this` = `este` (`undefined` si no tiene).
    pub(crate) fn llamar_get(&mut self, par: Valor, este: Valor) -> Result<Valor, Valor> {
        let g = self.mundo.leer_elemento(par, 0).unwrap_or(Valor::INDEFINIDO);
        if self.es_funcion(g) { self.llamar_interno(g, este, &[]) } else { Ok(Valor::INDEFINIDO) }
    }

    /// Llama al `set` del par (sin `set`, la escritura no hace nada).
    pub(crate) fn llamar_set(&mut self, par: Valor, este: Valor, v: Valor) -> Result<(), Valor> {
        let s = self.mundo.leer_elemento(par, 1).unwrap_or(Valor::INDEFINIDO);
        if self.es_funcion(s) {
            self.llamar_interno(s, este, &[v])?;
        }
        Ok(())
    }

    /// `obj.clave` siguiendo los accesores (para las nativas que leen propiedades).
    pub(crate) fn leer_valor(&mut self, obj: Valor, clave: u32) -> Result<Valor, Valor> {
        match self.buscar_accesor(obj, clave) {
            Some(par) => self.llamar_get(par, obj),
            None => Ok(self.mundo.leer(obj, clave).unwrap_or(Valor::INDEFINIDO)),
        }
    }

    /// Define (o completa) el accesor `clave` de `obj`. Un `get` o `set` que no viene (`None`)
    /// conserva el que hubiera; si había una propiedad de datos con ese nombre, se quita.
    pub(crate) fn definir_accesor(&mut self, obj: Valor, clave: u32, get: Option<Valor>, set: Option<Valor>) {
        let marca = self.mundo.m.marca_raices();
        let (dado_get, dado_set) = (get.is_some(), set.is_some());
        for v in [obj, get.unwrap_or(Valor::INDEFINIDO), set.unwrap_or(Valor::INDEFINIDO)] {
            self.mundo.m.raiz(v);
        }
        let (mut get, mut set) = (self.pila(marca + 1), self.pila(marca + 2));
        // Lo que no viene en el descriptor se conserva (`get x` y `set x` llegan en dos llamadas);
        // lo que viene, aunque sea `undefined`, sustituye (un decorador que borra el getter).
        if let Some(previo) = self.accesor_propio(obj, clave) {
            if !dado_get {
                get = self.mundo.leer_elemento(previo, 0).unwrap_or(Valor::INDEFINIDO);
            }
            if !dado_set {
                set = self.mundo.leer_elemento(previo, 1).unwrap_or(Valor::INDEFINIDO);
            }
        }
        self.mundo.m.pila_mut()[marca + 1] = get;
        self.mundo.m.pila_mut()[marca + 2] = set;
        let par = self.mundo.crear_arreglo(&[self.pila(marca + 1), self.pila(marca + 2)], self.pila(crate::R_ARREGLO));
        let obj = self.pila(marca);
        // Si ya había algo con ese nombre (datos, o el hueco que deja un objeto literal), el
        // accesor ocupa su sitio: el orden de las claves es el del código.
        let en_su_sitio = self.mundo.cambiar_clave(obj, clave, clave | ACCESOR, par)
            || self.mundo.cambiar_clave(obj, clave | ACCESOR, clave | ACCESOR, par);
        if !en_su_sitio {
            self.mundo.escribir(obj, clave | ACCESOR, par);
        }
        self.mundo.m.soltar_raices(marca);
        let bit = clave as usize % (64 * N_FILTRO);
        self.filtro_accesores[bit / 64] |= 1 << (bit % 64);
        self.epoca_accesores = self.epoca_accesores.wrapping_add(1);
    }

    /// Si `clave` puede tener un accesor en algún objeto (el filtro de bits; `false` es seguro).
    #[inline]
    fn quizas_accesor(&self, clave: u32) -> bool {
        let bit = clave as usize % (64 * N_FILTRO);
        self.filtro_accesores[bit / 64] & (1 << (bit % 64)) != 0
    }
}
