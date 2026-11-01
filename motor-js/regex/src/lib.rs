//! Expresiones regulares de JavaScript para QUIMERA, sin dependencias y `no_std`.
//!
//! Como irregexp (SpiderMonkey, V8): el patrón se analiza a un árbol (`patron`), se compila a
//! un programa (`programa`) y una máquina con vuelta atrás lo ejecuta (`maquina`). La vuelta
//! atrás usa una pila propia, no la de Rust: un texto largo no la desborda.
//!
//! Atajos de búsqueda:
//! - un patrón que es solo texto (`/zbmvyyn/`) se busca con `str::find`, sin máquina;
//! - un prefijo fijo (`/ag\s5/` empieza siempre por `"ag"`) salta de aparición en aparición;
//! - `^` sin bandera `m` solo se prueba al principio.
//!
//! El texto es UTF-8 y las posiciones son bytes; quien necesite índices UTF-16 (JavaScript)
//! los convierte.
#![no_std]

extern crate alloc;

mod clases;
mod maquina;
mod patron;
mod programa;

pub use maquina::Memoria;

use alloc::string::String;
use alloc::vec::Vec;
use programa::{Op, Opciones, Programa};

/// Un patrón o unas banderas no válidos (`SyntaxError` en JavaScript).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// Qué falla.
    pub mensaje: String,
}

/// Las banderas (`gimsuyd`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Banderas {
    /// `g`: todas las apariciones (`lastIndex`).
    pub global: bool,
    /// `i`: sin distinguir mayúsculas.
    pub sin_caso: bool,
    /// `m`: `^` y `$` en cada línea.
    pub multilinea: bool,
    /// `s`: `.` también encaja con los fines de línea.
    pub punto_todo: bool,
    /// `u`: modo Unicode.
    pub unicode: bool,
    /// `y`: solo encaja justo en `lastIndex`.
    pub pegajosa: bool,
    /// `d`: índices de los grupos.
    pub indices: bool,
}

impl Banderas {
    /// Lee `"gi"` y similares. Una bandera repetida o desconocida es un error.
    pub fn leer(texto: &str) -> Result<Banderas, Error> {
        let mut b = Banderas::default();
        for c in texto.chars() {
            let campo = match c {
                'g' => &mut b.global,
                'i' => &mut b.sin_caso,
                'm' => &mut b.multilinea,
                's' => &mut b.punto_todo,
                'u' => &mut b.unicode,
                'y' => &mut b.pegajosa,
                'd' => &mut b.indices,
                _ => return Err(Error { mensaje: alloc::format!("bandera '{c}' no válida") }),
            };
            if *campo {
                return Err(Error { mensaje: alloc::format!("bandera '{c}' repetida") });
            }
            *campo = true;
        }
        Ok(b)
    }

    /// Las banderas como texto, en el orden canónico (`dgimsuy`).
    pub fn texto(&self) -> String {
        let pares = [
            (self.indices, 'd'),
            (self.global, 'g'),
            (self.sin_caso, 'i'),
            (self.multilinea, 'm'),
            (self.punto_todo, 's'),
            (self.unicode, 'u'),
            (self.pegajosa, 'y'),
        ];
        pares.iter().filter(|(si, _)| *si).map(|(_, c)| *c).collect()
    }
}

/// Una expresión regular compilada.
#[derive(Debug, Clone)]
pub struct Regex {
    programa: Programa,
    grupos: u32,
    banderas: Banderas,
    /// Texto fijo con el que empieza todo encaje (sin bandera `i`).
    prefijo: String,
    /// El patrón es solo `prefijo`: se busca sin máquina.
    solo_texto: bool,
    /// Empieza por `^` sin `m`: solo puede encajar en la posición 0.
    anclada: bool,
}

impl Regex {
    /// Compila `patron` con estas banderas.
    pub fn nueva(patron: &str, banderas: &str) -> Result<Regex, Error> {
        let banderas = Banderas::leer(banderas)?;
        let caracteres: Vec<char> = patron.chars().collect();
        let mut lector = patron::Lector::nuevo(&caracteres, banderas.sin_caso, banderas.unicode);
        let arbol = lector.todo()?;
        let opciones =
            Opciones { sin_caso: banderas.sin_caso, multilinea: banderas.multilinea, punto_todo: banderas.punto_todo };
        let mut programa = programa::compilar(&arbol, &opciones)?;
        programa.sin_caso = banderas.sin_caso;
        let prefijo: String = programa.ops[1..]
            .iter()
            .map_while(|o| match o {
                Op::Caracter(c) => Some(*c),
                _ => None,
            })
            .collect();
        let n = prefijo.chars().count();
        let solo_texto = programa.ops.len() == n + 3;
        let anclada = matches!(programa.ops.get(1), Some(Op::Inicio { multilinea: false }));
        Ok(Regex { programa, grupos: lector.total_grupos, banderas, prefijo, solo_texto, anclada })
    }

    /// Cuántos grupos captura (sin contar el encaje entero).
    pub fn grupos(&self) -> u32 {
        self.grupos
    }

    /// Sus banderas.
    pub fn banderas(&self) -> Banderas {
        self.banderas
    }

    /// Busca el primer encaje que empiece en `desde` o después (en `desde` exacto con la
    /// bandera `y`). Si lo hay, sus grupos quedan en `m` (`Memoria::captura`).
    pub fn buscar(&self, texto: &str, desde: usize, m: &mut Memoria) -> bool {
        m.preparar(self.grupos, self.programa.registros);
        let t = texto.as_bytes();
        if desde > t.len() || !texto.is_char_boundary(desde) {
            return false;
        }
        if self.banderas.pegajosa || self.anclada {
            let posible = !self.anclada || desde == 0;
            return posible && self.probar(t, desde, m);
        }
        let mut pos = desde;
        loop {
            if !self.prefijo.is_empty() {
                let Some(i) = texto[pos..].find(self.prefijo.as_str()) else { return false };
                pos += i;
                if self.solo_texto {
                    m.ranuras[0] = pos;
                    m.ranuras[1] = pos + self.prefijo.len();
                    return true;
                }
            }
            if maquina::encajar(&self.programa, t, pos, m) {
                return true;
            }
            match maquina::siguiente(t, pos) {
                Some((_, n)) => pos += n,
                None => return false,
            }
        }
    }

    fn probar(&self, t: &[u8], pos: usize, m: &mut Memoria) -> bool {
        maquina::encajar(&self.programa, t, pos, m)
    }
}
