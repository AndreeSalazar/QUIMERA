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

impl Motor {
    /// Apunta las claves de la biblioteca (no enumerables). Se llama al terminar de prepararla.
    pub(crate) fn fijar_claves_ocultas(&mut self) {
        for i in 0..N_FIJAS {
            self.ocultas[i] = self.mundo.claves(self.pila(i)).len();
        }
    }

    /// Cuántas claves de `obj` son de la biblioteca.
    fn ocultas_de(&self, obj: Valor) -> usize {
        (0..N_FIJAS).find(|&i| self.pila(i) == obj).map_or(0, |i| self.ocultas[i])
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
                if !claves.contains(&k) {
                    claves.push(k);
                }
            }
            actual = self.mundo.prototipo(actual);
        }
        let valores: Vec<Valor> =
            claves.into_iter().map(|k| Valor::cadena(quimera_objetos::Ref::desde_u32(k))).collect();
        self.mundo.crear_arreglo(&valores, self.pila(R_ARREGLO))
    }

    /// Los valores que recorre `for (v of x)`: elementos de un arreglo o caracteres de una cadena.
    pub(crate) fn iterar_valores(&mut self, x: Valor) -> Result<Valor, Valor> {
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
