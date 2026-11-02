//! `Proxy`: un objeto cuyas lecturas, escrituras, `in`, `delete` y `Object.keys` pasan por las
//! trampas de su manejador (`get`, `set`, `has`, `deleteProperty`, `ownKeys`). Sin la trampa,
//! la operación va al objetivo.
//!
//! Un proxy es un objeto del anfitrión con el número [`DATO_PROXY`]: las cachés en línea ya
//! excluyen a esos objetos (como a los nodos del DOM), así que cada acceso llega aquí. El
//! objetivo y el manejador van en dos ranuras ocultas del propio objeto. Su prototipo es el
//! del objetivo al crearlo (para `instanceof`).
//!
//! Falta: proxies de funciones (`apply`, `construct`), `Proxy.revocable` y las trampas de
//! descriptores y prototipos.

use crate::Motor;
use alloc::string::ToString;
use quimera_objetos::{Ref, Valor};

/// El número de anfitrión de un proxy (el DOM usa los bits bajos para sus nodos).
pub(crate) const DATO_PROXY: u32 = u32::MAX;
const OBJETIVO: &str = "\u{1}objetivo";
const MANEJADOR: &str = "\u{1}manejador";

impl Motor {
    /// `new Proxy(objetivo, manejador)`.
    pub(crate) fn crear_proxy(&mut self, objetivo: Valor, manejador: Valor) -> Result<Valor, Valor> {
        if !self.mundo.es_objeto(objetivo) || !self.mundo.es_objeto(manejador) {
            return Err(self.error("TypeError", "Proxy necesita un objetivo y un manejador que sean objetos"));
        }
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(objetivo);
        self.mundo.m.raiz(manejador);
        let proto = self.mundo.prototipo(objetivo);
        let p = self.mundo.crear_anfitrion(DATO_PROXY, proto);
        self.mundo.m.raiz(p);
        let (k_o, k_m) = (self.mundo.cadenas.internar(OBJETIVO), self.mundo.cadenas.internar(MANEJADOR));
        self.mundo.escribir(self.pila(marca + 2), k_o, self.pila(marca));
        self.mundo.escribir(self.pila(marca + 2), k_m, self.pila(marca + 1));
        let p = self.pila(marca + 2);
        self.mundo.m.soltar_raices(marca);
        Ok(p)
    }

    pub(crate) fn es_proxy(&self, v: Valor) -> bool {
        self.mundo.anfitrion(v) == Some(DATO_PROXY)
    }

    /// El objetivo y el manejador de un proxy.
    fn partes(&self, p: Valor) -> (Valor, Valor) {
        let leer = |n: &str| {
            let k = self.mundo.cadenas.buscar(n);
            k.and_then(|k| self.mundo.leer_propia(p, k)).unwrap_or(Valor::INDEFINIDO)
        };
        (leer(OBJETIVO), leer(MANEJADOR))
    }

    /// Una clave como valor de JavaScript (texto o símbolo), para pasarla a una trampa.
    fn clave_como_valor(&self, clave: u32) -> Valor {
        if self.mundo.cadenas.texto(clave).starts_with('\u{2}') {
            Valor::simbolo(Ref::desde_u32(clave))
        } else {
            Valor::cadena(Ref::desde_u32(clave))
        }
    }

    /// La clave para ir al objetivo sin trampa: un número si es un índice (`"1"` → 1), para que
    /// un arreglo encuentre su elemento.
    fn clave_de_respaldo(&self, clave: u32) -> Valor {
        let t = self.mundo.cadenas.texto(clave);
        let indice = t.parse::<u32>().ok().filter(|i| i.to_string() == t);
        match indice {
            Some(i) => Valor::numero(i as f64),
            None => self.clave_como_valor(clave),
        }
    }

    /// Prepara una operación: deja en la pila el proxy, su objetivo, su manejador y la trampa
    /// `nombre` (o `undefined`), y devuelve la marca para soltarlos.
    fn preparar(&mut self, p: Valor, nombre: &str) -> Result<usize, Valor> {
        let marca = self.mundo.m.marca_raices();
        let (o, h) = self.partes(p);
        for v in [p, o, h] {
            self.mundo.m.raiz(v);
        }
        let k = self.mundo.cadenas.internar(nombre);
        let trampa = match self.leer_valor(h, k) {
            Ok(t) => t,
            Err(e) => {
                self.mundo.m.soltar_raices(marca);
                return Err(e);
            }
        };
        let trampa = if self.es_funcion(trampa) { trampa } else { Valor::INDEFINIDO };
        self.mundo.m.raiz(trampa);
        Ok(marca)
    }

    /// Llama a la trampa preparada con `(objetivo, ...extra)`; `None` si no hay trampa.
    fn con_trampa(&mut self, marca: usize, extra: &[Valor]) -> Option<Result<Valor, Valor>> {
        let trampa = self.pila(marca + 3);
        if trampa == Valor::INDEFINIDO {
            return None;
        }
        let mut args = alloc::vec![self.pila(marca + 1)];
        args.extend_from_slice(extra);
        Some(self.llamar_interno(trampa, self.pila(marca + 2), &args))
    }

    /// `p.clave`.
    pub(crate) fn proxy_leer(&mut self, p: Valor, clave: u32) -> Result<Valor, Valor> {
        let marca = self.preparar(p, "get")?;
        let k = self.clave_como_valor(clave);
        let r = match self.con_trampa(marca, &[k, self.pila(marca)]) {
            Some(r) => r,
            None => {
                let k = self.clave_de_respaldo(clave);
                self.leer_indice(self.pila(marca + 1), k)
            }
        };
        self.mundo.m.soltar_raices(marca);
        r
    }

    /// `p.clave = v`.
    pub(crate) fn proxy_escribir(&mut self, p: Valor, clave: u32, v: Valor) -> Result<(), Valor> {
        let marca0 = self.mundo.m.marca_raices();
        self.mundo.m.raiz(v);
        let marca = self.preparar(p, "set")?;
        let k = self.clave_como_valor(clave);
        let v = self.pila(marca0);
        let r = match self.con_trampa(marca, &[k, v, self.pila(marca)]) {
            Some(r) => r.map(|_| ()),
            None => {
                let k = self.clave_de_respaldo(clave);
                let (o, v) = (self.pila(marca + 1), self.pila(marca0));
                self.escribir_indice(o, k, v)
            }
        };
        self.mundo.m.soltar_raices(marca0);
        r
    }

    /// `clave in p`.
    pub(crate) fn proxy_contiene(&mut self, p: Valor, clave: u32) -> Result<bool, Valor> {
        let marca = self.preparar(p, "has")?;
        let k = self.clave_como_valor(clave);
        let r = match self.con_trampa(marca, &[k]) {
            Some(r) => r.map(|v| crate::operaciones::a_booleano(&self.mundo, v)),
            None => self.contiene(k, self.pila(marca + 1)),
        };
        self.mundo.m.soltar_raices(marca);
        r
    }

    /// `delete p[clave]`.
    pub(crate) fn proxy_borrar(&mut self, p: Valor, clave: u32) -> Result<Valor, Valor> {
        let marca = self.preparar(p, "deleteProperty")?;
        let k = self.clave_como_valor(clave);
        let r = match self.con_trampa(marca, &[k]) {
            Some(r) => r.map(|v| Valor::booleano(crate::operaciones::a_booleano(&self.mundo, v))),
            None => self.borrar(self.pila(marca + 1), k),
        };
        self.mundo.m.soltar_raices(marca);
        r
    }

    /// Las claves de `Object.keys(p)`: lo que da `ownKeys` (solo los textos), o las del objetivo.
    /// Devuelve el arreglo, o `None` para que `Object.keys` mire el objetivo.
    pub(crate) fn proxy_claves(&mut self, p: Valor) -> Result<Result<Valor, Valor>, Valor> {
        let marca = self.preparar(p, "ownKeys")?;
        let r = match self.con_trampa(marca, &[]) {
            Some(r) => Ok(r?),
            None => Err(self.pila(marca + 1)),
        };
        self.mundo.m.soltar_raices(marca);
        Ok(r)
    }
}
