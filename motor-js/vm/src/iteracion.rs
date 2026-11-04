//! `for-in`, `for-of` y `delete`.
//!
//! `for-in` y `for-of` reciben un arreglo nuevo con lo que recorrer (claves o valores), que el
//! bucle compilado lee por índice. Las propiedades que la biblioteca pone en los prototipos
//! (`push`, `toString`...) no son enumerables: al preparar el motor se apunta cuántas claves
//! tiene cada objeto de la biblioteca y `for-in` se salta esas.

use crate::{Motor, N_FIJAS, R_ARREGLO};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use quimera_objetos::{Cadenas, Clase, Valor};

/// Las ranuras internas (U+0001), los símbolos (U+0002) y los campos `#privados` no se enumeran.
pub(crate) fn oculta(clave: &str) -> bool {
    clave.starts_with('\u{1}') || clave.starts_with('\u{2}') || clave.starts_with('#') || clave == "[[Valor]]"
}

/// La ranura oculta con las claves de un objeto que no se enumeran (`Object.defineProperty`
/// con `enumerable: false`, o sin decirlo al crearla): sus números, separados por comas. Solo la
/// tienen los objetos que la necesitan, así que a los demás no les cuesta nada.
pub(crate) const NO_ENUMERABLES: &str = "\u{1}noenum";

impl Motor {
    /// Las claves de `obj` marcadas como no enumerables.
    fn no_enumerables(&self, obj: Valor) -> Vec<u32> {
        let lista = self.mundo.cadenas.buscar(NO_ENUMERABLES).and_then(|k| self.mundo.leer_propia(obj, k));
        match lista.and_then(|l| self.mundo.texto(l)) {
            Some(t) => t.split(',').filter_map(|n| n.parse().ok()).collect(),
            None => Vec::new(),
        }
    }

    /// Si la clave propia `clave` de `obj` se enumera, según su marca.
    pub(crate) fn es_enumerable_propia(&self, obj: Valor, clave: u32) -> bool {
        !self.no_enumerables(obj).contains(&clave)
    }

    /// Marca la clave `clave` del objeto en la posición `pos` de la pila como enumerable o no.
    pub(crate) fn marcar_enumerable(&mut self, pos: usize, clave: u32, enumerable: bool) {
        let mut lista = self.no_enumerables(self.pila(pos));
        if lista.contains(&clave) != enumerable {
            return;
        }
        if enumerable {
            lista.retain(|k| *k != clave);
        } else {
            lista.push(clave);
        }
        let texto: Vec<String> = lista.iter().map(|k| k.to_string()).collect();
        let v = self.mundo.cadena(&texto.join(","));
        let k = self.mundo.cadenas.internar(NO_ENUMERABLES);
        self.mundo.escribir(self.pila(pos), k, v);
    }

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
        let no = self.no_enumerables(obj);
        let claves = self.mundo.claves(obj).iter().skip(saltar);
        claves
            .map(|&k| k & !crate::accesores::ACCESOR)
            .filter(|&k| self.enumerable(obj, k) && !no.contains(&k))
            .collect()
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
            (Some(Clase::Arreglo | Clase::Tipado), _) => self.mundo.longitud(obj).unwrap_or(0),
            (_, Some(s)) => s.chars().count() as u32,
            _ => 0,
        };
        let presentes: Vec<u32> = (0..indices).filter(|&i| !self.mundo.es_hueco(obj, i)).collect();
        for i in presentes {
            claves.push(self.mundo.cadenas.internar(&i.to_string()));
        }
        let mut tapadas: Vec<u32> = Vec::new();
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
            // Una propia no enumerable tampoco deja ver la del prototipo con ese nombre.
            tapadas.extend(self.no_enumerables(actual));
            for k in self.mundo.claves(actual).iter().skip(saltar).map(|&k| k & !crate::accesores::ACCESOR) {
                // Las claves que empiezan por U+0001 son ranuras internas de la biblioteca.
                if !claves.contains(&k) && !tapadas.contains(&k) && self.enumerable(actual, k) {
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
        let directo = matches!(self.mundo.clase(x), Some(Clase::Arreglo | Clase::Tipado));
        if self.mundo.es_objeto(x) && !directo {
            let clave = self.mundo.cadenas.internar("__quimera_lista");
            let global = self.pila(crate::R_GLOBAL);
            if let Some(f) = self.mundo.leer(global, clave).filter(|f| self.mundo.clase(*f) == Some(Clase::Funcion)) {
                return self.llamar_interno(f, Valor::INDEFINIDO, &[x]);
            }
        }
        let valores: Vec<Valor> = if directo {
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
                self.mundo.escribir_elemento(obj, i, Valor::HUECO);
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
        if self.es_proxy(obj) {
            return self.proxy_borrar(obj, clave);
        }
        self.mundo.borrar(obj, clave);
        if clave & crate::accesores::ACCESOR == 0 {
            self.mundo.borrar(obj, clave | crate::accesores::ACCESOR);
        }
        Ok(Valor::VERDADERO)
    }
}
