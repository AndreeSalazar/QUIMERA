//! Cadenas internadas: cada texto distinto tiene un número, que es también su clave de propiedad.
//!
//! Comparar dos cadenas o buscar una propiedad es comparar números. Por ahora las cadenas
//! no se liberan; `quimera-cadenas` (nivel 0) las llevará al GC con UTF-16 como JavaScript.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_valor::{Ref, Valor};

/// La tabla de cadenas.
#[derive(Debug, Clone, Default)]
pub struct Cadenas {
    lista: Vec<String>,
    indice: BTreeMap<String, u32>,
}

impl Cadenas {
    /// El número de `texto` (lo crea si no existe).
    pub fn internar(&mut self, texto: &str) -> u32 {
        if let Some(&n) = self.indice.get(texto) {
            return n;
        }
        let n = self.lista.len() as u32;
        self.lista.push(String::from(texto));
        self.indice.insert(String::from(texto), n);
        n
    }

    /// El número de `texto` si ya existe.
    pub fn buscar(&self, texto: &str) -> Option<u32> {
        self.indice.get(texto).copied()
    }

    /// El texto de un número.
    pub fn texto(&self, n: u32) -> &str {
        self.lista.get(n as usize).map_or("", String::as_str)
    }

    /// Un valor de cadena con este texto.
    pub fn valor(&mut self, texto: &str) -> Valor {
        let n = self.internar(texto);
        Valor::cadena(Ref::desde_u32(n))
    }

    /// El texto de un valor de cadena.
    pub fn de_valor(&self, v: Valor) -> Option<&str> {
        match v.tipo() {
            quimera_valor::Tipo::Cadena => Some(self.texto(v.ref_del_monton()?.numero() as u32)),
            _ => None,
        }
    }

    /// El número de cadena de un valor de cadena.
    pub fn id_de_valor(v: Valor) -> Option<u32> {
        match v.tipo() {
            quimera_valor::Tipo::Cadena => Some(v.ref_del_monton()?.numero() as u32),
            _ => None,
        }
    }

    /// Cuántas cadenas hay.
    pub fn total(&self) -> usize {
        self.lista.len()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn cada_texto_un_numero() {
        let mut c = Cadenas::default();
        let a = c.internar("hola");
        assert_eq!(c.internar("hola"), a);
        assert_ne!(c.internar("adiós"), a);
        let v = c.valor("hola");
        assert_eq!(c.de_valor(v), Some("hola"));
        assert_eq!(Cadenas::id_de_valor(v), Some(a));
        assert_eq!(c.de_valor(Valor::entero(1)), None);
    }
}
