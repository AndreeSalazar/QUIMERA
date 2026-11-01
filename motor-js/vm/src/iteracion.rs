//! `for-in`, `for-of` y `delete`.
//!
//! `for-in` y `for-of` reciben un arreglo nuevo con lo que recorrer (claves o valores), que el
//! bucle compilado lee por índice. Las propiedades que la biblioteca pone en los prototipos
//! (`push`, `toString`...) no son enumerables: al preparar el motor se apunta cuántas claves
//! tiene cada objeto de la biblioteca y `for-in` se salta esas.

use crate::{Motor, N_FIJAS, R_ARREGLO};
use alloc::string::ToString;
use alloc::vec::Vec;
use quimera_objetos::{Cadenas, Clase, Valor};

/// Las ranuras internas (U+0001) y los campos `#privados` de las clases no se enumeran.
fn oculta(clave: &str) -> bool {
    clave.starts_with('\u{1}') || clave.starts_with('#')
}

impl Motor {
    /// Apunta las claves de la biblioteca (no enumerables). Se llama al terminar de prepararla.
    pub(crate) fn fijar_claves_ocultas(&mut self) {
        for i in 0..N_FIJAS {
            self.ocultas[i] = self.mundo.claves(self.pila(i)).len();
        }
    }

    /// Las claves propias de `obj` que se enumeran (`Object.keys`, `JSON.stringify`): sin las de
    /// la biblioteca ni las ranuras internas (las que empiezan por U+0001).
    pub(crate) fn claves_enumerables(&self, obj: Valor) -> Vec<u32> {
        let saltar = self.ocultas_de(obj);
        self.mundo.claves(obj).iter().skip(saltar).copied().filter(|&k| self.enumerable(obj, k)).collect()
    }

    /// Si la clave `k` de `obj` se enumera: no las ranuras internas, ni los `#privados`, ni el
    /// `prototype` de una función (que en los navegadores no es enumerable).
    fn enumerable(&self, obj: Valor, k: u32) -> bool {
        !oculta(self.mundo.cadenas.texto(k))
            && !(k == self.mundo.clave_prototype && self.mundo.clase(obj) == Some(Clase::Funcion))
    }

    /// Cuántas claves de `obj` son de la biblioteca.
    fn ocultas_de(&self, obj: Valor) -> usize {
        match (0..N_FIJAS).find(|&i| self.pila(i) == obj) {
            Some(i) => self.ocultas[i],
            None => self.mundo.forma(obj).and_then(|f| self.formas_ocultas.get(&f).copied()).unwrap_or(0),
        }
    }

    /// Las claves que recorre `for (k in obj)`: índices, propias y heredadas, sin repetir.
    pub(crate) fn iterar_claves(&mut self, obj: Valor) -> Valor {
        let mut claves: Vec<u32> = Vec::new();
        let indices = match (self.mundo.clase(obj), self.mundo.texto(obj)) {
            (Some(Clase::Arreglo), _) => self.mundo.longitud(obj).unwrap_or(0),
            (_, Some(s)) => s.chars().count() as u32,
            _ => 0,
        };
        for i in 0..indices {
            claves.push(self.mundo.cadenas.internar(&i.to_string()));
        }
        let mut actual = obj;
        for _ in 0..10_000 {
            if !self.mundo.es_objeto(actual) {
                break;
            }
            let mut saltar = self.ocultas_de(actual);
            // El `constructor` que crea `F.prototype` va primero y no es enumerable.
            let constructor = self.mundo.cadenas.internar("constructor");
            if saltar == 0 && self.mundo.claves(actual).first() == Some(&constructor) {
                saltar = 1;
            }
            for &k in self.mundo.claves(actual).iter().skip(saltar) {
                // Las claves que empiezan por U+0001 son ranuras internas de la biblioteca.
                if !claves.contains(&k) && self.enumerable(actual, k) {
                    claves.push(k);
                }
            }
            actual = self.mundo.prototipo(actual);
        }
        let valores: Vec<Valor> =
            claves.into_iter().map(|k| Valor::cadena(quimera_objetos::Ref::desde_u32(k))).collect();
        self.mundo.crear_arreglo(&valores, self.pila(R_ARREGLO))
    }

    /// Los valores que recorre `for (v of x)`: elementos de un arreglo, caracteres de una cadena,
    /// o lo que da `__quimera_lista` del preludio (Map, Set, iteradores, objetos con `next()`).
    pub(crate) fn iterar_valores(&mut self, x: Valor) -> Result<Valor, Valor> {
        if self.mundo.es_objeto(x) && self.mundo.clase(x) != Some(Clase::Arreglo) {
            let clave = self.mundo.cadenas.internar("__quimera_lista");
            let global = self.pila(crate::R_GLOBAL);
            if let Some(f) = self.mundo.leer(global, clave).filter(|f| self.mundo.clase(*f) == Some(Clase::Funcion)) {
                return self.llamar_interno(f, Valor::INDEFINIDO, &[x]);
            }
        }
        let valores: Vec<Valor> = if self.mundo.clase(x) == Some(Clase::Arreglo) {
            let n = self.mundo.longitud(x).unwrap_or(0);
            (0..n).map(|i| self.mundo.leer_elemento(x, i).unwrap_or(Valor::INDEFINIDO)).collect()
        } else if let Some(s) = self.mundo.texto(x) {
            let caracteres: Vec<alloc::string::String> = s.chars().map(|c| c.to_string()).collect();
            caracteres.iter().map(|c| self.mundo.cadena(c)).collect()
        } else {
            let que = self.texto(x);
            return Err(self.error("TypeError", &alloc::format!("{que} no es iterable")));
        };
        Ok(self.mundo.crear_arreglo(&valores, self.pila(R_ARREGLO)))
    }

    /// `delete obj[k]`: siempre `true` (QUIMERA no tiene propiedades no configurables).
    pub(crate) fn borrar(&mut self, obj: Valor, k: Valor) -> Result<Valor, Valor> {
        if matches!(obj.tipo(), quimera_objetos::Tipo::Indefinido | quimera_objetos::Tipo::Nulo) {
            return Err(self.error("TypeError", "no se puede borrar una propiedad de null o undefined"));
        }
        if !self.mundo.es_objeto(obj) {
            return Ok(Valor::VERDADERO);
        }
        if self.mundo.clase(obj) == Some(Clase::Arreglo)
            && let Some(i) = Self::indice(k)
        {
            if i < self.mundo.longitud(obj).unwrap_or(0) {
                self.mundo.escribir_elemento(obj, i, Valor::INDEFINIDO);
            }
            return Ok(Valor::VERDADERO);
        }
        let clave = match Cadenas::id_de_valor(k) {
            Some(id) => id,
            None => {
                let s = self.texto(k);
                self.mundo.cadenas.internar(&s)
            }
        };
        self.mundo.borrar(obj, clave);
        Ok(Valor::VERDADERO)
    }
}
