//! Posiciones en el código fuente y errores.

use alloc::string::String;
use core::fmt;

/// Una posición: línea y columna (desde 1; la columna cuenta bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Pos {
    /// Línea.
    pub linea: u32,
    /// Columna.
    pub columna: u32,
}

/// Un error de sintaxis o de compilación, con dónde ocurrió.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    /// Dónde.
    pub pos: Pos,
    /// Qué pasa.
    pub mensaje: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.pos.linea, self.pos.columna, self.mensaje)
    }
}

/// Resultado del frontend.
pub type Resultado<T> = Result<T, Error>;

/// Atajo para crear un error.
pub fn error<T>(pos: Pos, mensaje: impl Into<String>) -> Resultado<T> {
    Err(Error { pos, mensaje: mensaje.into() })
}
