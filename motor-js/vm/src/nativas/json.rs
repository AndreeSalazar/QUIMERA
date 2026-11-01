//! `JSON.parse` y `JSON.stringify`, en Rust: las páginas grandes traen megas de JSON (YouTube
//! lleva sus datos iniciales en uno), y escrito en JavaScript sería muchas veces más lento.
//!
//! - **`parse`** baja por el texto con recursión (hasta 512 niveles). Cada objeto o arreglo a
//!   medio llenar está en la pila de raíces: crear sus hijos puede mover objetos.
//! - **`stringify`** recorre las propiedades propias en su orden (los índices primero), llama a
//!   `toJSON` si existe, salta `undefined` y las funciones (en arreglos son `null`), y avisa de
//!   los ciclos. Acepta la sangría (`space`, número o texto) y un `replacer` que sea un arreglo
//!   de nombres; un `replacer` función, todavía no.

use super::{arg, definir_nativa, nuevo_objeto};
use crate::llamadas::Llamada;
use crate::{Motor, R_ARREGLO, R_GLOBAL, R_OBJETO, operaciones as op};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_objetos::{Clase, Tipo, Valor};

const PARSE: u32 = 600;
const STRINGIFY: u32 = 601;
const PROFUNDIDAD: usize = 512;

pub(super) fn preparar(m: &mut Motor) {
    let json = nuevo_objeto(m, R_GLOBAL, "JSON");
    definir_nativa(m, json, "parse", PARSE);
    definir_nativa(m, json, "stringify", STRINGIFY);
}

pub(super) fn llamar(m: &mut Motor, id: u32, ll: &Llamada) -> Result<Valor, Valor> {
    match id {
        PARSE => {
            let texto = m.texto(arg(m, ll, 0));
            let mut l = Lector { b: texto.as_bytes(), i: 0 };
            let v = l.valor(m, 0)?;
            l.blancos();
            if l.i < l.b.len() {
                return Err(l.error(m));
            }
            Ok(v)
        }
        _ => {
            let sangria = match arg(m, ll, 2) {
                v if v.como_numero().is_some() => " ".repeat(op::a_numero(&m.mundo, v).clamp(0.0, 10.0) as usize),
                v if v.tipo() == Tipo::Cadena => m.texto(v).chars().take(10).collect(),
                _ => String::new(),
            };
            let lista = arg(m, ll, 1);
            let permitidas = (m.mundo.clase(lista) == Some(Clase::Arreglo)).then(|| {
                let n = m.mundo.longitud(lista).unwrap_or(0);
                (0..n).map(|i| m.texto(m.mundo.leer_elemento(lista, i).unwrap_or(Valor::INDEFINIDO))).collect()
            });
            let mut e = Escritor { sangria, permitidas, salida: String::new(), pila: Vec::new() };
            let marca = m.mundo.m.marca_raices();
            let v = arg(m, ll, 0);
            m.mundo.m.raiz(v);
            let escrito = e.valor(m, marca, "");
            m.mundo.m.soltar_raices(marca);
            Ok(if escrito? { m.mundo.cadena(&e.salida) } else { Valor::INDEFINIDO })
        }
    }
}

struct Lector<'a> {
    b: &'a [u8],
    i: usize,
}

impl Lector<'_> {
    fn error(&self, m: &mut Motor) -> Valor {
        m.error("SyntaxError", &alloc::format!("JSON no válido en la posición {}", self.i))
    }

    fn blancos(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn literal(&mut self, m: &mut Motor, palabra: &str, v: Valor) -> Result<Valor, Valor> {
        if self.b[self.i..].starts_with(palabra.as_bytes()) {
            self.i += palabra.len();
            Ok(v)
        } else {
            Err(self.error(m))
        }
    }

    fn valor(&mut self, m: &mut Motor, nivel: usize) -> Result<Valor, Valor> {
        self.blancos();
        if nivel > PROFUNDIDAD {
            return Err(m.error("SyntaxError", "JSON demasiado anidado"));
        }
        match self.b.get(self.i) {
            Some(b'{') => self.objeto(m, nivel),
            Some(b'[') => self.arreglo(m, nivel),
            Some(b'"') => {
                let s = self.cadena(m)?;
                Ok(m.mundo.cadena(&s))
            }
            Some(b't') => self.literal(m, "true", Valor::VERDADERO),
            Some(b'f') => self.literal(m, "false", Valor::FALSO),
            Some(b'n') => self.literal(m, "null", Valor::NULO),
            Some(b'-' | b'0'..=b'9') => self.numero(m),
            _ => Err(self.error(m)),
        }
    }

    fn numero(&mut self, m: &mut Motor) -> Result<Valor, Valor> {
        let inicio = self.i;
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        let digitos = |l: &mut Self| {
            let d = l.i;
            while l.b.get(l.i).is_some_and(u8::is_ascii_digit) {
                l.i += 1;
            }
            l.i > d
        };
        let primera = self.i;
        let mut ok = digitos(self);
        // `01` no es JSON: un cero inicial va solo.
        if self.b.get(primera) == Some(&b'0') && self.i - primera > 1 {
            ok = false;
        }
        if self.b.get(self.i) == Some(&b'.') {
            self.i += 1;
            ok &= digitos(self);
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            ok &= digitos(self);
        }
        let texto = core::str::from_utf8(&self.b[inicio..self.i]).unwrap_or("x");
        match texto.parse::<f64>() {
            Ok(n) if ok => Ok(op::numero(n)),
            _ => Err(self.error(m)),
        }
    }

    fn hex4(&mut self, m: &mut Motor) -> Result<u32, Valor> {
        let h = self.b.get(self.i..self.i + 4).and_then(|h| core::str::from_utf8(h).ok());
        match h.and_then(|h| u32::from_str_radix(h, 16).ok()) {
            Some(n) => {
                self.i += 4;
                Ok(n)
            }
            None => Err(self.error(m)),
        }
    }

    fn cadena(&mut self, m: &mut Motor) -> Result<String, Valor> {
        self.i += 1;
        let mut s = String::new();
        loop {
            let inicio = self.i;
            while self.b.get(self.i).is_some_and(|&c| c != b'"' && c != b'\\' && c >= 0x20) {
                self.i += 1;
            }
            s.push_str(core::str::from_utf8(&self.b[inicio..self.i]).unwrap_or(""));
            match self.b.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(s);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let c = self.b.get(self.i).copied();
                    self.i += 1;
                    match c {
                        Some(b'"') => s.push('"'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'/') => s.push('/'),
                        Some(b'b') => s.push('\u{8}'),
                        Some(b'f') => s.push('\u{c}'),
                        Some(b'n') => s.push('\n'),
                        Some(b'r') => s.push('\r'),
                        Some(b't') => s.push('\t'),
                        Some(b'u') => {
                            let mut n = self.hex4(m)?;
                            // Pares sustitutos: `😀` es un solo carácter.
                            if (0xD800..0xDC00).contains(&n) && self.b[self.i..].starts_with(b"\\u") {
                                let guardado = self.i;
                                self.i += 2;
                                let bajo = self.hex4(m)?;
                                if (0xDC00..0xE000).contains(&bajo) {
                                    n = 0x10000 + ((n - 0xD800) << 10) + (bajo - 0xDC00);
                                } else {
                                    self.i = guardado;
                                }
                            }
                            s.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.error(m)),
                    }
                }
                _ => return Err(self.error(m)),
            }
        }
    }

    fn arreglo(&mut self, m: &mut Motor, nivel: usize) -> Result<Valor, Valor> {
        self.i += 1;
        let a = m.mundo.crear_arreglo(&[], m.pila(R_ARREGLO));
        let marca = m.mundo.m.marca_raices();
        m.mundo.m.raiz(a);
        let r = self.elementos(m, nivel, marca);
        let a = m.pila(marca);
        m.mundo.m.soltar_raices(marca);
        r.map(|()| a)
    }

    fn elementos(&mut self, m: &mut Motor, nivel: usize, marca: usize) -> Result<(), Valor> {
        self.blancos();
        if self.b.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(());
        }
        loop {
            let v = self.valor(m, nivel + 1)?;
            m.mundo.empujar(m.pila(marca), v);
            self.blancos();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => return Err(self.error(m)),
            }
        }
    }

    fn objeto(&mut self, m: &mut Motor, nivel: usize) -> Result<Valor, Valor> {
        self.i += 1;
        let o = m.mundo.crear_objeto(m.pila(R_OBJETO));
        let marca = m.mundo.m.marca_raices();
        m.mundo.m.raiz(o);
        let r = self.propiedades(m, nivel, marca);
        let o = m.pila(marca);
        m.mundo.m.soltar_raices(marca);
        r.map(|()| o)
    }

    fn propiedades(&mut self, m: &mut Motor, nivel: usize, marca: usize) -> Result<(), Valor> {
        self.blancos();
        if self.b.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.blancos();
            if self.b.get(self.i) != Some(&b'"') {
                return Err(self.error(m));
            }
            let k = self.cadena(m)?;
            self.blancos();
            if self.b.get(self.i) != Some(&b':') {
                return Err(self.error(m));
            }
            self.i += 1;
            let v = self.valor(m, nivel + 1)?;
            let clave = m.mundo.cadenas.internar(&k);
            m.mundo.escribir(m.pila(marca), clave, v);
            self.blancos();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(());
                }
                _ => return Err(self.error(m)),
            }
        }
    }
}

/// Una cadena entre comillas, con los escapes de JSON.
pub(crate) fn citar(s: &str, salida: &mut String) {
    salida.push('"');
    for c in s.chars() {
        match c {
            '"' => salida.push_str("\\\""),
            '\\' => salida.push_str("\\\\"),
            '\n' => salida.push_str("\\n"),
            '\r' => salida.push_str("\\r"),
            '\t' => salida.push_str("\\t"),
            '\u{8}' => salida.push_str("\\b"),
            '\u{c}' => salida.push_str("\\f"),
            c if (c as u32) < 0x20 => salida.push_str(&alloc::format!("\\u{:04x}", c as u32)),
            c => salida.push(c),
        }
    }
    salida.push('"');
}

struct Escritor {
    sangria: String,
    permitidas: Option<Vec<String>>,
    salida: String,
    /// Los objetos que se están escribiendo (posiciones en la pila), para ver los ciclos.
    pila: Vec<usize>,
}

impl Escritor {
    fn salto(&mut self) {
        if !self.sangria.is_empty() {
            self.salida.push('\n');
            for _ in 0..self.pila.len() {
                let s = self.sangria.clone();
                self.salida.push_str(&s);
            }
        }
    }

    /// Escribe el valor que está en la pila en `pos`. `false` si no se escribe (`undefined`,
    /// funciones).
    fn valor(&mut self, m: &mut Motor, pos: usize, clave: &str) -> Result<bool, Valor> {
        let mut v = m.pila(pos);
        if m.mundo.es_objeto(v) {
            let to_json = m.mundo.cadenas.internar("toJSON");
            if let Some(f) = m.mundo.leer(v, to_json).filter(|f| m.es_funcion(*f)) {
                let k = m.mundo.cadena(clave);
                v = m.llamar_interno(f, v, &[k])?;
                m.mundo.m.pila_mut()[pos] = v;
            }
        }
        if let Some(p) = op::valor_primitivo(&m.mundo, v).filter(|_| m.mundo.es_objeto(v)) {
            v = p; // `new Number(1)`, `new String("a")`
        }
        match v.tipo() {
            Tipo::Nulo => self.salida.push_str("null"),
            Tipo::Booleano => self.salida.push_str(if v == Valor::VERDADERO { "true" } else { "false" }),
            Tipo::Numero | Tipo::Entero => {
                let n = op::a_numero(&m.mundo, v);
                self.salida.push_str(&if n.is_finite() { op::numero_a_texto(n) } else { String::from("null") });
            }
            Tipo::Cadena => citar(&m.texto(v), &mut self.salida),
            Tipo::Referencia if m.es_funcion(v) => return Ok(false),
            Tipo::Referencia if m.mundo.es_objeto(v) => {
                if self.pila.iter().any(|&p| m.pila(p) == v) {
                    return Err(m.error("TypeError", "JSON.stringify: estructura circular"));
                }
                self.pila.push(pos);
                let r =
                    if m.mundo.clase(v) == Some(Clase::Arreglo) { self.arreglo(m, pos) } else { self.objeto(m, pos) };
                self.pila.pop();
                r?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn arreglo(&mut self, m: &mut Motor, pos: usize) -> Result<(), Valor> {
        let n = m.mundo.longitud(m.pila(pos)).unwrap_or(0);
        self.salida.push('[');
        for i in 0..n {
            if i > 0 {
                self.salida.push(',');
            }
            self.salto();
            let e = m.mundo.leer_elemento(m.pila(pos), i).unwrap_or(Valor::INDEFINIDO);
            m.mundo.m.raiz(e);
            let p = m.mundo.m.marca_raices() - 1;
            let escrito = self.valor(m, p, &alloc::format!("{i}"));
            m.mundo.m.soltar_raices(p);
            if !escrito? {
                self.salida.push_str("null");
            }
        }
        if n > 0 {
            self.pila.pop();
            self.salto();
            self.pila.push(pos);
        }
        self.salida.push(']');
        Ok(())
    }

    fn objeto(&mut self, m: &mut Motor, pos: usize) -> Result<(), Valor> {
        let claves = m.claves_enumerables(m.pila(pos));
        self.salida.push('{');
        let mut alguna = false;
        for k in claves {
            let nombre = String::from(m.mundo.cadenas.texto(k));
            if self.permitidas.as_ref().is_some_and(|p| !p.contains(&nombre)) {
                continue;
            }
            let v = m.leer_valor(m.pila(pos), k)?;
            m.mundo.m.raiz(v);
            let p = m.mundo.m.marca_raices() - 1;
            let antes = self.salida.len();
            if alguna {
                self.salida.push(',');
            }
            self.salto();
            citar(&nombre, &mut self.salida);
            self.salida.push(':');
            if !self.sangria.is_empty() {
                self.salida.push(' ');
            }
            let escrito = self.valor(m, p, &nombre);
            m.mundo.m.soltar_raices(p);
            if escrito? {
                alguna = true;
            } else {
                self.salida.truncate(antes);
            }
        }
        if alguna {
            self.pila.pop();
            self.salto();
            self.pila.push(pos);
        }
        self.salida.push('}');
        Ok(())
    }
}
