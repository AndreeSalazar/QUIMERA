//! Bytes de verdad: `ArrayBuffer` y las vistas de los arreglos tipados.
//!
//! - Un **búfer** (`Clase::Bytes`) es `[tipo, forma, proto, props, palabras, largo]`: sus bytes
//!   van de cuatro en cuatro en los enteros de otra celda del GC (`palabras`), así que se liberan
//!   solos como cualquier objeto. `largo` es su tamaño en bytes.
//! - Una **vista** (`Clase::Tipado`: `Uint8Array`, `Float64Array`...) es
//!   `[tipo, forma, proto, props, búfer, largo, desplazamiento, especie]`: no copia nada, lee y
//!   escribe en los bytes de su búfer. `largo` (en elementos) está donde el de un arreglo.
//!
//! Los números se guardan en orden *little endian*, como en los navegadores de x86 y ARM.

use crate::mundo::{C_EXTRA, C_EXTRA2, C_TIPO, Clase, Mundo};
use alloc::vec::Vec;
use quimera_valor::Valor;

const V_DESDE: usize = 6;
const V_ESPECIE: usize = 7;

/// El tipo de elemento de una vista.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Especie {
    /// `Int8Array`
    Int8 = 0,
    /// `Uint8Array`
    Uint8 = 1,
    /// `Uint8ClampedArray`
    Uint8Recortado = 2,
    /// `Int16Array`
    Int16 = 3,
    /// `Uint16Array`
    Uint16 = 4,
    /// `Int32Array`
    Int32 = 5,
    /// `Uint32Array`
    Uint32 = 6,
    /// `Float32Array`
    Float32 = 7,
    /// `Float64Array`
    Float64 = 8,
}

impl Especie {
    /// La especie de su número (el de `#[repr(u8)]`).
    pub fn desde(n: u8) -> Option<Especie> {
        use Especie::*;
        Some(match n {
            0 => Int8,
            1 => Uint8,
            2 => Uint8Recortado,
            3 => Int16,
            4 => Uint16,
            5 => Int32,
            6 => Uint32,
            7 => Float32,
            8 => Float64,
            _ => return None,
        })
    }

    /// Bytes por elemento.
    pub fn tam(self) -> usize {
        match self {
            Especie::Int8 | Especie::Uint8 | Especie::Uint8Recortado => 1,
            Especie::Int16 | Especie::Uint16 => 2,
            Especie::Int32 | Especie::Uint32 | Especie::Float32 => 4,
            Especie::Float64 => 8,
        }
    }

    /// Un número de JavaScript como los bytes de un elemento (con el recorte y el redondeo de
    /// cada especie: `ToInt8`, `ToUint8Clamp`...).
    pub fn codificar(self, x: f64) -> [u8; 8] {
        let mut b = [0u8; 8];
        // `ToInt32` y compañía: truncar, y módulo 2^n (NaN e infinito dan 0).
        let modular = modulo_32;
        match self {
            Especie::Int8 | Especie::Uint8 => b[0] = modular(x) as u8,
            Especie::Uint8Recortado => {
                // Redondeo al par más cercano, entre 0 y 255.
                let r = if x.is_nan() { 0.0 } else { x.clamp(0.0, 255.0) };
                let f = f64::from(r as u32);
                let v = if r - f > 0.5 || (r - f == 0.5 && f % 2.0 != 0.0) { f + 1.0 } else { f };
                b[0] = v as u8;
            }
            Especie::Int16 | Especie::Uint16 => b[..2].copy_from_slice(&(modular(x) as u16).to_le_bytes()),
            Especie::Int32 | Especie::Uint32 => b[..4].copy_from_slice(&modular(x).to_le_bytes()),
            Especie::Float32 => b[..4].copy_from_slice(&(x as f32).to_le_bytes()),
            Especie::Float64 => b.copy_from_slice(&x.to_le_bytes()),
        }
        b
    }

    /// Los bytes de un elemento como número de JavaScript.
    pub fn decodificar(self, b: &[u8; 8]) -> f64 {
        match self {
            Especie::Int8 => f64::from(b[0] as i8),
            Especie::Uint8 | Especie::Uint8Recortado => f64::from(b[0]),
            Especie::Int16 => f64::from(i16::from_le_bytes([b[0], b[1]])),
            Especie::Uint16 => f64::from(u16::from_le_bytes([b[0], b[1]])),
            Especie::Int32 => f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            Especie::Uint32 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            Especie::Float32 => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            Especie::Float64 => f64::from_le_bytes(*b),
        }
    }
}

/// Lo que dice una vista de sí misma.
#[derive(Debug, Clone, Copy)]
pub struct Vista {
    /// Su búfer.
    pub bufer: Valor,
    /// Dónde empieza en él, en bytes.
    pub desde: usize,
    /// Cuántos elementos tiene.
    pub largo: usize,
    /// Su tipo de elemento.
    pub especie: Especie,
}

/// `ToUint32`: truncar y módulo 2^32 (NaN e infinito dan 0). Sin `std`: con enteros.
fn modulo_32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    if x.abs() < 9.0e15 {
        return (x as i64).rem_euclid(1 << 32) as u32;
    }
    // Tan grande ya es entero: el resto en coma flotante es exacto.
    let r = x % 4_294_967_296.0;
    (if r < 0.0 { r + 4_294_967_296.0 } else { r }) as u32
}

fn como_usize(v: Option<Valor>) -> Option<usize> {
    usize::try_from(v?.como_entero()?).ok()
}

impl Mundo {
    /// Un búfer de `n` bytes a cero.
    pub fn crear_bytes(&mut self, n: usize, proto: Valor) -> Option<Valor> {
        if n > i32::MAX as usize {
            return None;
        }
        let palabras = alloc::vec![Valor::entero(0); n.div_ceil(4).max(1)];
        let base = self.proteger(&[proto]);
        let celda = self.m.asignar(&palabras);
        let proto = self.recuperar(base, 0);
        self.m.soltar_raices(base);
        let tipo = Valor::entero(Clase::Bytes as i32);
        let forma = Valor::entero(crate::FORMA_VACIA as i32);
        Some(self.m.asignar(&[tipo, forma, proto, Valor::NULO, celda, Valor::entero(n as i32)]))
    }

    /// Un búfer con estos bytes.
    pub fn crear_bytes_con(&mut self, datos: &[u8], proto: Valor) -> Option<Valor> {
        let b = self.crear_bytes(datos.len(), proto)?;
        self.escribir_bytes(b, 0, datos);
        Some(b)
    }

    /// El tamaño en bytes de un búfer.
    pub fn bytes_largo(&self, b: Valor) -> Option<usize> {
        if self.clase(b)? != Clase::Bytes {
            return None;
        }
        como_usize(self.m.leer(b, C_EXTRA2))
    }

    fn palabra(&self, celda: Valor, i: usize) -> u32 {
        self.m.leer(celda, i / 4).and_then(|v| v.como_entero()).unwrap_or(0) as u32
    }

    /// `n` bytes desde `desde` (lo que se salga del búfer no sale).
    pub fn leer_bytes(&self, b: Valor, desde: usize, n: usize) -> Vec<u8> {
        let Some(largo) = self.bytes_largo(b) else { return Vec::new() };
        let Some(celda) = self.m.leer(b, C_EXTRA) else { return Vec::new() };
        let fin = desde.saturating_add(n).min(largo);
        let mut v = Vec::with_capacity(fin.saturating_sub(desde));
        let mut i = desde;
        while i < fin {
            let p = self.palabra(celda, i);
            // De la palabra, los bytes que tocan (las palabras van alineadas a 4).
            while i < fin {
                v.push((p >> (8 * (i % 4))) as u8);
                i += 1;
                if i.is_multiple_of(4) {
                    break;
                }
            }
        }
        v
    }

    /// Escribe `datos` desde `desde` (lo que se salga del búfer se descarta).
    pub fn escribir_bytes(&mut self, b: Valor, desde: usize, datos: &[u8]) {
        let Some(largo) = self.bytes_largo(b) else { return };
        let Some(celda) = self.m.leer(b, C_EXTRA) else { return };
        let fin = desde.saturating_add(datos.len()).min(largo);
        let mut i = desde;
        while i < fin {
            let mut p = self.palabra(celda, i);
            let indice = i / 4;
            while i < fin {
                let k = 8 * (i % 4);
                p = (p & !(0xff << k)) | (u32::from(datos[i - desde]) << k);
                i += 1;
                if i.is_multiple_of(4) {
                    break;
                }
            }
            self.m.escribir(celda, indice, Valor::entero(p as i32));
        }
    }

    /// Copia `n` bytes de un búfer a otro (o al mismo, aunque se pisen), como `memmove`.
    pub fn copiar_bytes(&mut self, origen: Valor, desde: usize, destino: Valor, a: usize, n: usize) {
        let datos = self.leer_bytes(origen, desde, n);
        self.escribir_bytes(destino, a, &datos);
    }

    /// Una vista de `largo` elementos de `especie` sobre `bufer`, desde el byte `desde`.
    pub fn crear_vista(
        &mut self,
        bufer: Valor,
        desde: usize,
        largo: usize,
        especie: Especie,
        proto: Valor,
    ) -> Option<Valor> {
        let total = self.bytes_largo(bufer)?;
        if desde.checked_add(largo.checked_mul(especie.tam())?)? > total || !desde.is_multiple_of(especie.tam()) {
            return None;
        }
        let tipo = Valor::entero(Clase::Tipado as i32);
        let forma = Valor::entero(crate::FORMA_VACIA as i32);
        let campos = [
            tipo,
            forma,
            proto,
            Valor::NULO,
            bufer,
            Valor::entero(largo as i32),
            Valor::entero(desde as i32),
            Valor::entero(especie as i32),
        ];
        Some(self.m.asignar(&campos))
    }

    /// Lo que dice una vista de sí misma (`None` si no es una vista).
    pub fn vista(&self, v: Valor) -> Option<Vista> {
        if self.clase(v)? != Clase::Tipado {
            return None;
        }
        Some(Vista {
            bufer: self.m.leer(v, C_EXTRA)?,
            largo: como_usize(self.m.leer(v, C_EXTRA2))?,
            desde: como_usize(self.m.leer(v, V_DESDE))?,
            especie: Especie::desde(u8::try_from(self.m.leer(v, V_ESPECIE)?.como_entero()?).ok()?)?,
        })
    }

    /// Si `v` es una vista (rápido: solo mira el tipo de la celda).
    pub fn es_vista(&self, v: Valor) -> bool {
        v.como_referencia().is_some()
            && self.m.leer(v, C_TIPO).and_then(|t| t.como_entero()) == Some(Clase::Tipado as i32)
    }

    /// El elemento `i` de una vista (`None` si se sale).
    pub fn leer_en_vista(&self, v: Valor, i: usize) -> Option<f64> {
        let x = self.vista(v)?;
        if i >= x.largo {
            return None;
        }
        let n = x.especie.tam();
        let mut b = [0u8; 8];
        let o = x.desde + i * n;
        if n == 1 {
            let celda = self.m.leer(x.bufer, C_EXTRA)?;
            b[0] = (self.palabra(celda, o) >> (8 * (o % 4))) as u8;
        } else {
            b[..n].copy_from_slice(&self.leer_bytes(x.bufer, o, n));
        }
        Some(x.especie.decodificar(&b))
    }

    /// Escribe el elemento `i` de una vista (fuera de ella no hace nada, como en JavaScript).
    pub fn escribir_en_vista(&mut self, v: Valor, i: usize, valor: f64) {
        let Some(x) = self.vista(v) else { return };
        if i >= x.largo {
            return;
        }
        let n = x.especie.tam();
        let b = x.especie.codificar(valor);
        self.escribir_bytes(x.bufer, x.desde + i * n, &b[..n]);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn bufer_vistas_y_especies() {
        let mut m = Mundo::default();
        let b = m.crear_bytes(10, Valor::NULO).unwrap_or(Valor::NULO);
        m.escribir_bytes(b, 3, &[1, 2, 3, 4, 5]);
        assert_eq!(m.leer_bytes(b, 0, 10), [0, 0, 0, 1, 2, 3, 4, 5, 0, 0]);
        assert_eq!(m.leer_bytes(b, 8, 5), [0, 0], "lo que se sale no sale");
        m.copiar_bytes(b, 3, b, 4, 5);
        assert_eq!(m.leer_bytes(b, 0, 10), [0, 0, 0, 1, 1, 2, 3, 4, 5, 0], "como memmove");
        // Dos vistas sobre los mismos bytes.
        let u8s = m.crear_vista(b, 2, 4, Especie::Uint8, Valor::NULO).unwrap_or(Valor::NULO);
        let u16s = m.crear_vista(b, 4, 2, Especie::Uint16, Valor::NULO).unwrap_or(Valor::NULO);
        assert_eq!(m.leer_en_vista(u8s, 2), Some(1.0));
        assert_eq!(m.leer_en_vista(u16s, 0), Some(f64::from(u16::from_le_bytes([1, 2]))));
        m.escribir_en_vista(u16s, 0, 65_535.0 + 2.0);
        assert_eq!(m.leer_en_vista(u8s, 2), Some(1.0), "65537 módulo 2^16 = 1");
        assert_eq!(m.leer_en_vista(u8s, 4), None);
        assert!(m.crear_vista(b, 1, 2, Especie::Uint16, Valor::NULO).is_none(), "desalineada");
        assert!(m.crear_vista(b, 0, 11, Especie::Uint8, Valor::NULO).is_none(), "se sale");
        // Recortes de cada especie.
        assert_eq!(Especie::Int8.decodificar(&Especie::Int8.codificar(200.0)), -56.0);
        assert_eq!(Especie::Uint8Recortado.decodificar(&Especie::Uint8Recortado.codificar(2.5)), 2.0);
        assert_eq!(Especie::Uint8Recortado.decodificar(&Especie::Uint8Recortado.codificar(300.0)), 255.0);
        assert_eq!(Especie::Uint32.decodificar(&Especie::Uint32.codificar(-1.0)), 4_294_967_295.0);
        assert_eq!(Especie::Float32.decodificar(&Especie::Float32.codificar(0.1)), f64::from(0.1f32));
    }
}
