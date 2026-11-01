//! Cadenas: cada texto vivo tiene un número único, así que comparar dos cadenas es comparar
//! números y una cadena puede ser clave de propiedad sin más.
//!
//! Hay dos clases:
//! - **átomos**: claves de propiedades y textos del código. No se liberan nunca (las formas y
//!   el bytecode guardan su número).
//! - **dinámicas**: lo que crea el programa al ejecutarse (`a + b`, `replace`...). Se liberan
//!   tras una recolección mayor si nadie las alcanzó: el GC anota qué cadenas vio al marcar
//!   y `barrer` libera el resto.
//!
//! Como el texto es único, una dinámica nunca coincide con una clave de una forma: para leer
//! una propiedad basta su número. Para **escribirla** se convierte en átomo (`clave_para_escribir`).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_valor::{Ref, Valor};

/// Lo que cuesta guardar un texto: dos copias (lista e índice) y su entrada.
pub(crate) fn coste(texto: &str) -> usize {
    2 * texto.len() + 64
}

#[derive(Debug, Clone, Default)]
struct Entrada {
    texto: String,
    atomo: bool,
    /// Recolecciones mayores que había cuando se creó (o se volvió a pedir): una recolección
    /// posterior que no la vio prueba que está muerta; una anterior, no.
    epoca: u32,
    viva: bool,
}

/// La tabla de cadenas.
#[derive(Debug, Clone, Default)]
pub struct Cadenas {
    lista: Vec<Entrada>,
    indice: BTreeMap<String, u32>,
    libres: Vec<u32>,
    /// Recolecciones mayores vistas (la época de las cadenas nuevas).
    epoca: u32,
    liberadas: u64,
    /// Memoria aproximada de las cadenas vivas.
    bytes: usize,
}

impl Cadenas {
    fn crear(&mut self, texto: &str, atomo: bool) -> u32 {
        if let Some(&n) = self.indice.get(texto) {
            let e = &mut self.lista[n as usize];
            e.atomo |= atomo;
            e.epoca = self.epoca;
            return n;
        }
        let entrada = Entrada { texto: String::from(texto), atomo, epoca: self.epoca, viva: true };
        self.bytes += coste(texto);
        let n = match self.libres.pop() {
            Some(n) => {
                self.lista[n as usize] = entrada;
                n
            }
            None => {
                self.lista.push(entrada);
                self.lista.len() as u32 - 1
            }
        };
        self.indice.insert(String::from(texto), n);
        n
    }

    /// El número de `texto` como átomo (clave de propiedad o texto del código): no se libera.
    pub fn internar(&mut self, texto: &str) -> u32 {
        self.crear(texto, true)
    }

    /// El número de `texto` si existe.
    pub fn buscar(&self, texto: &str) -> Option<u32> {
        self.indice.get(texto).copied()
    }

    /// El texto de un número (`""` si ya se liberó).
    pub fn texto(&self, n: u32) -> &str {
        self.lista.get(n as usize).map_or("", |e| e.texto.as_str())
    }

    /// Un valor de cadena con este texto (dinámica salvo que ya fuera un átomo).
    pub fn valor(&mut self, texto: &str) -> Valor {
        let n = self.crear(texto, false);
        Valor::cadena(Ref::desde_u32(n))
    }

    /// El texto de un valor de cadena.
    pub fn de_valor(&self, v: Valor) -> Option<&str> {
        match v.tipo() {
            quimera_valor::Tipo::Cadena => Some(self.texto(v.ref_del_monton()?.numero() as u32)),
            _ => None,
        }
    }

    /// El número de cadena de un valor de cadena. Sirve para **leer** propiedades; para
    /// escribirlas hace falta [`Cadenas::clave_para_escribir`].
    pub fn id_de_valor(v: Valor) -> Option<u32> {
        match v.tipo() {
            // Un símbolo es una cadena interna única (empieza por U+0002): también es una clave.
            quimera_valor::Tipo::Cadena | quimera_valor::Tipo::Simbolo => Some(v.ref_del_monton()?.numero() as u32),
            _ => None,
        }
    }

    /// El número de un valor de cadena convertido en átomo, para guardarlo como clave.
    pub fn clave_para_escribir(&mut self, v: Valor) -> Option<u32> {
        let n = Self::id_de_valor(v)?;
        let e = self.lista.get_mut(n as usize)?;
        e.atomo = true;
        Some(n)
    }

    /// Época actual: cuántas recolecciones mayores se han barrido.
    pub fn epoca(&self) -> u32 {
        self.epoca
    }

    /// Tras la recolección mayor número `epoca`, que vio las cadenas marcadas en `vistas`
    /// (un bit por número): libera las dinámicas creadas antes de ella que no vio.
    pub fn barrer(&mut self, vistas: &[u64], epoca: u32) {
        let vista = |n: usize| vistas.get(n / 64).is_some_and(|b| b & (1 << (n % 64)) != 0);
        for (n, e) in self.lista.iter_mut().enumerate() {
            if e.viva && !e.atomo && e.epoca < epoca && !vista(n) {
                self.indice.remove(e.texto.as_str());
                self.bytes -= coste(&e.texto);
                *e = Entrada::default();
                self.libres.push(n as u32);
                self.liberadas += 1;
            }
        }
        self.epoca = epoca;
    }

    /// Cuántas cadenas hay vivas.
    pub fn total(&self) -> usize {
        self.indice.len()
    }

    /// Memoria aproximada que ocupan las cadenas vivas.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Cuántas se han liberado desde el principio.
    pub fn liberadas(&self) -> u64 {
        self.liberadas
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
        assert_eq!(Cadenas::id_de_valor(v), Some(a), "un texto que ya es átomo usa el átomo");
        assert_eq!(c.de_valor(Valor::entero(1)), None);
    }

    #[test]
    fn las_dinamicas_que_nadie_vio_se_liberan_y_su_numero_se_reutiliza() {
        let mut c = Cadenas::default();
        let clave = c.internar("clave");
        let viva = Cadenas::id_de_valor(c.valor("viva")).unwrap();
        let muerta = Cadenas::id_de_valor(c.valor("muerta")).unwrap();
        let escrita = c.valor("escrita");
        let escrita = c.clave_para_escribir(escrita).unwrap();
        let mut vistas = [0u64; 1];
        vistas[0] |= 1 << viva;
        c.barrer(&vistas, 1);
        assert_eq!(
            (c.texto(clave), c.texto(viva), c.texto(muerta), c.texto(escrita)),
            ("clave", "viva", "", "escrita")
        );
        assert_eq!(c.buscar("muerta"), None);
        assert_eq!(c.liberadas(), 1);
        let nueva = Cadenas::id_de_valor(c.valor("otra")).unwrap();
        assert_eq!(nueva, muerta, "el número libre se reutiliza");
        // Creada después de la recolección: la siguiente barrida con esas marcas no la toca.
        c.barrer(&vistas, 1);
        assert_eq!(c.texto(nueva), "otra");
    }
}
