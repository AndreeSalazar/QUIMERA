//! Los errores que lanza la máquina, como objetos (`TypeError`, `RangeError`...) con `message`
//! y `stack`, y la traza de la pila de llamadas.
//!
//! La traza sigue el formato de V8 (`    at nombre (archivo:línea:columna)`), con una diferencia:
//! la posición es **donde empieza la función**, no la línea exacta que falló, porque el
//! bytecode no guarda la posición de cada instrucción. En código minificado (todo en una línea)
//! basta para encontrar la función.

use crate::{Motor, R_ERROR, R_GLOBAL};
use alloc::format;
use alloc::string::String;
use core::fmt::Write;
use quimera_objetos::Valor;

/// Cuántas llamadas lleva una traza, como V8 (`Error.stackTraceLimit`).
const LIMITE: usize = 10;

impl Motor {
    /// La pila de llamadas de JavaScript, de la más reciente a la más antigua.
    pub(crate) fn traza(&self) -> String {
        self.traza_desde(0)
    }

    /// La traza sin las `saltar` llamadas más recientes.
    pub(crate) fn traza_desde(&self, saltar: usize) -> String {
        let mut s = String::new();
        for marco in self.marcos.iter().rev().skip(saltar).take(LIMITE) {
            let Some(c) = self.funciones.get(marco.funcion as usize) else { continue };
            let origen = if c.origen.is_empty() { "<anónimo>" } else { &c.origen };
            let donde = format!("{origen}:{}:{}", c.f.linea, c.f.columna);
            let nombre = c.f.nombre.and_then(|a| c.atomos.get(a as usize)).map(|id| self.mundo.cadenas.texto(*id));
            let _ = match nombre {
                Some(n) if !n.is_empty() => write!(s, "\n    at {n} ({donde})"),
                _ => write!(s, "\n    at {donde}"),
            };
        }
        s
    }

    /// Las líneas `    at ...` del `stack` de un error lanzado (vacío si no es un error).
    pub(crate) fn traza_de(&self, e: Valor) -> String {
        let pila = self.mundo.cadenas.buscar("stack").and_then(|k| self.mundo.leer(e, k));
        match pila.and_then(|p| self.mundo.texto(p)) {
            Some(t) => t.find("\n    at ").map_or(String::new(), |i| String::from(&t[i..])),
            None => String::new(),
        }
    }

    /// Un error de JavaScript: un objeto de la clase `clase` (`TypeError`...) con su mensaje y
    /// su traza en `stack`.
    pub(crate) fn error(&mut self, clase: &str, mensaje: &str) -> Valor {
        let proto = self.prototipo_de_error(clase);
        self.objeto_error(proto, clase, Some(mensaje))
    }

    /// Un objeto de error con prototipo `proto`: `message` (si lo hay) y `stack`, que empieza
    /// como `toString` (`Nombre: mensaje`, o solo `Nombre`).
    pub(crate) fn objeto_error(&mut self, proto: Valor, nombre: &str, mensaje: Option<&str>) -> Valor {
        let cabecera = match mensaje {
            Some(m) if !m.is_empty() => format!("{nombre}: {m}"),
            _ => String::from(nombre),
        };
        let traza = format!("{cabecera}{}", self.traza());
        let marca = self.mundo.m.marca_raices();
        self.mundo.m.raiz(proto);
        let o = self.mundo.crear_objeto(self.pila(marca));
        self.mundo.m.raiz(o);
        let texto = self.mundo.cadena(&traza);
        self.mundo.m.raiz(texto);
        let k_pila = self.mundo.cadenas.internar("stack");
        let (o, t) = (self.pila(marca + 1), self.pila(marca + 2));
        self.mundo.escribir(o, k_pila, t);
        if let Some(m) = mensaje {
            let texto = self.mundo.cadena(m);
            let k_mensaje = self.mundo.cadenas.internar("message");
            let o = self.pila(marca + 1);
            self.mundo.escribir(o, k_mensaje, texto);
        }
        let o = self.pila(marca + 1);
        self.mundo.m.soltar_raices(marca);
        o
    }

    /// Un error de compilación o del anfitrión (`"SyntaxError: ..."`) como objeto de su clase.
    pub(crate) fn error_desde_texto(&mut self, texto: &str) -> Valor {
        match texto.split_once(": ") {
            Some((clase, resto)) if clase.ends_with("Error") && !clase.contains(' ') => {
                let clase = String::from(clase);
                self.error(&clase, resto)
            }
            _ => self.error("Error", texto),
        }
    }

    /// `clase.prototype` (de la global con ese nombre), o el de `Error` si no existe aún (los
    /// errores del propio preludio, antes de definir `TypeError`).
    fn prototipo_de_error(&self, clase: &str) -> Valor {
        let global = self.pila(R_GLOBAL);
        let proto = self
            .mundo
            .cadenas
            .buscar(clase)
            .and_then(|k| self.mundo.leer(global, k))
            .filter(|c| self.mundo.es_objeto(*c))
            .and_then(|c| self.mundo.leer(c, self.mundo.clave_prototype))
            .filter(|p| self.mundo.es_objeto(*p));
        proto.unwrap_or_else(|| self.pila(R_ERROR))
    }
}
