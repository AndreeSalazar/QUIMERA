//! Valor de JavaScript en 64 bits: el ladrillo de QUIMERA.
//!
//! Idea tomada de SpiderMonkey (`JS::Value`): un valor es un `u64` que se copia sin más.
//! Boa suma y resta un contador de referencias cada vez que copia un valor con un objeto
//! (el perfil lo midió en 8-13 % del tiempo); aquí el GC encuentra lo vivo por sus raíces,
//! así que copiar es gratis.
//!
//! # Codificación (NaN-boxing)
//!
//! - Un `f64` cualquiera se guarda tal cual. Todos los NaN se normalizan a
//!   `0x7FF8_0000_0000_0000`, así que el espacio de NaN negativos queda libre.
//! - El resto usa ese espacio: `0xFFF8_0000_0000_0000 | etiqueta << 47 | carga`, con una
//!   etiqueta de 3 bits (1..=7) y una carga de 47 bits.
//!
//! | Etiqueta | Tipo | Carga |
//! |---|---|---|
//! | 1 | `undefined` | 0 |
//! | 2 | `null` | 0 |
//! | 3 | booleano | 0 o 1 |
//! | 4 | entero de 32 bits | el `i32` en los 32 bits bajos |
//! | 5 | referencia a un objeto del GC | [`Ref`] |
//! | 6 | referencia a una cadena | [`Ref`] |
//! | 7 | referencia a un símbolo | [`Ref`] |
#![cfg_attr(not(test), no_std)]

use core::fmt;

const BASE: u64 = 0xFFF8_0000_0000_0000;
const DESPLAZAMIENTO: u32 = 47;
const MASCARA_CARGA: u64 = (1 << DESPLAZAMIENTO) - 1;
const NAN_CANONICO: u64 = 0x7FF8_0000_0000_0000;

const ETQ_INDEFINIDO: u64 = 1;
const ETQ_NULO: u64 = 2;
const ETQ_BOOLEANO: u64 = 3;
const ETQ_ENTERO: u64 = 4;
const ETQ_REFERENCIA: u64 = 5;
const ETQ_CADENA: u64 = 6;
const ETQ_SIMBOLO: u64 = 7;

const fn etiquetado(etiqueta: u64, carga: u64) -> u64 {
    BASE | (etiqueta << DESPLAZAMIENTO) | (carga & MASCARA_CARGA)
}

/// Referencia de 47 bits a algo que vive en el montón (objeto, cadena o símbolo).
///
/// QUIMERA no le da significado aquí: el GC decide cómo codifica zona e índice.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ref(u64);

impl Ref {
    /// La mayor referencia que cabe en un valor.
    pub const MAXIMA: u64 = MASCARA_CARGA;

    /// `None` si no cabe en 47 bits.
    pub const fn nueva(n: u64) -> Option<Ref> {
        if n <= Self::MAXIMA { Some(Ref(n)) } else { None }
    }

    /// Desde un `u32`: siempre cabe en 47 bits, así que no puede fallar.
    pub const fn desde_u32(n: u32) -> Ref {
        Ref(n as u64)
    }

    /// El número que guarda.
    pub const fn numero(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ref({:#x})", self.0)
    }
}

/// El tipo de un [`Valor`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tipo {
    /// Un número de coma flotante (`f64`).
    Numero,
    /// `undefined`.
    Indefinido,
    /// `null`.
    Nulo,
    /// `true` o `false`.
    Booleano,
    /// Un número entero que cabe en 32 bits (más rápido que un `f64` para índices y bucles).
    Entero,
    /// Un objeto del GC.
    Referencia,
    /// Una cadena.
    Cadena,
    /// Un símbolo.
    Simbolo,
}

/// Un valor de JavaScript en 64 bits. Es `Copy`: copiarlo no toca ningún contador.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Valor(u64);

const _: () = assert!(core::mem::size_of::<Valor>() == 8);

impl Valor {
    /// `undefined`.
    pub const INDEFINIDO: Valor = Valor(etiquetado(ETQ_INDEFINIDO, 0));
    /// `null`.
    pub const NULO: Valor = Valor(etiquetado(ETQ_NULO, 0));
    /// `true`.
    pub const VERDADERO: Valor = Valor(etiquetado(ETQ_BOOLEANO, 1));
    /// `false`.
    pub const FALSO: Valor = Valor(etiquetado(ETQ_BOOLEANO, 0));

    /// Un número. Cualquier NaN se guarda como el NaN canónico.
    pub const fn numero(n: f64) -> Valor {
        if n.is_nan() { Valor(NAN_CANONICO) } else { Valor(n.to_bits()) }
    }

    /// Un entero de 32 bits.
    pub const fn entero(n: i32) -> Valor {
        Valor(etiquetado(ETQ_ENTERO, n as u32 as u64))
    }

    /// Un booleano.
    pub const fn booleano(b: bool) -> Valor {
        if b { Self::VERDADERO } else { Self::FALSO }
    }

    /// Una referencia a un objeto del GC.
    pub const fn referencia(r: Ref) -> Valor {
        Valor(etiquetado(ETQ_REFERENCIA, r.0))
    }

    /// Una referencia a una cadena.
    pub const fn cadena(r: Ref) -> Valor {
        Valor(etiquetado(ETQ_CADENA, r.0))
    }

    /// Una referencia a un símbolo.
    pub const fn simbolo(r: Ref) -> Valor {
        Valor(etiquetado(ETQ_SIMBOLO, r.0))
    }

    const fn etiqueta(self) -> u64 {
        (self.0 >> DESPLAZAMIENTO) & 0b111
    }

    const fn es_etiquetado(self) -> bool {
        self.0 >= BASE
    }

    /// Su tipo.
    pub const fn tipo(self) -> Tipo {
        if !self.es_etiquetado() {
            return Tipo::Numero;
        }
        match self.etiqueta() {
            ETQ_INDEFINIDO => Tipo::Indefinido,
            ETQ_NULO => Tipo::Nulo,
            ETQ_BOOLEANO => Tipo::Booleano,
            ETQ_ENTERO => Tipo::Entero,
            ETQ_REFERENCIA => Tipo::Referencia,
            ETQ_CADENA => Tipo::Cadena,
            ETQ_SIMBOLO => Tipo::Simbolo,
            // `desde_bits` rechaza la etiqueta 0, así que aquí no se llega.
            _ => Tipo::Indefinido,
        }
    }

    /// El número que representa (también un entero, convertido a `f64`).
    pub const fn como_numero(self) -> Option<f64> {
        match self.tipo() {
            Tipo::Numero => Some(f64::from_bits(self.0)),
            Tipo::Entero => Some(self.0 as u32 as i32 as f64),
            _ => None,
        }
    }

    /// El entero, si el valor se guardó como entero.
    pub const fn como_entero(self) -> Option<i32> {
        match self.tipo() {
            Tipo::Entero => Some(self.0 as u32 as i32),
            _ => None,
        }
    }

    /// El booleano, si lo es.
    pub const fn como_booleano(self) -> Option<bool> {
        match self.tipo() {
            Tipo::Booleano => Some(self.0 & 1 == 1),
            _ => None,
        }
    }

    /// La referencia, si es un objeto del GC.
    pub const fn como_referencia(self) -> Option<Ref> {
        match self.tipo() {
            Tipo::Referencia => Some(Ref(self.0 & MASCARA_CARGA)),
            _ => None,
        }
    }

    /// La referencia, sea a un objeto, una cadena o un símbolo (lo que el GC debe seguir).
    pub const fn ref_del_monton(self) -> Option<Ref> {
        match self.tipo() {
            Tipo::Referencia | Tipo::Cadena | Tipo::Simbolo => Some(Ref(self.0 & MASCARA_CARGA)),
            _ => None,
        }
    }

    /// El mismo tipo de referencia con otro destino. El GC lo usa al mover objetos.
    pub const fn con_ref(self, r: Ref) -> Valor {
        match self.tipo() {
            Tipo::Referencia | Tipo::Cadena | Tipo::Simbolo => Valor(etiquetado(self.etiqueta(), r.0)),
            _ => self,
        }
    }

    /// Los 64 bits tal cual.
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Reconstruye un valor desde sus bits. `None` si no es una codificación válida:
    /// un NaN no canónico o la etiqueta 0, que no se usa.
    pub const fn desde_bits(bits: u64) -> Option<Valor> {
        let v = Valor(bits);
        if v.es_etiquetado() {
            if v.etiqueta() == 0 {
                return None;
            }
            return Some(v);
        }
        let n = f64::from_bits(bits);
        if n.is_nan() && bits != NAN_CANONICO { None } else { Some(v) }
    }

    /// `ToBoolean` de JavaScript para los valores que no necesitan mirar el montón.
    /// Las cadenas devuelven `None` (depende de si están vacías).
    pub fn verdad(self) -> Option<bool> {
        match self.tipo() {
            Tipo::Indefinido | Tipo::Nulo => Some(false),
            Tipo::Booleano => self.como_booleano(),
            Tipo::Entero => Some(self.como_entero() != Some(0)),
            Tipo::Numero => {
                let n = f64::from_bits(self.0);
                Some(!(n == 0.0 || n.is_nan()))
            }
            Tipo::Referencia | Tipo::Simbolo => Some(true),
            Tipo::Cadena => None,
        }
    }
}

impl fmt::Debug for Valor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.tipo() {
            Tipo::Numero => write!(f, "{}", f64::from_bits(self.0)),
            Tipo::Indefinido => f.write_str("undefined"),
            Tipo::Nulo => f.write_str("null"),
            Tipo::Booleano => write!(f, "{}", self.0 & 1 == 1),
            Tipo::Entero => write!(f, "{}i", self.0 as u32 as i32),
            Tipo::Referencia => write!(f, "objeto@{:#x}", self.0 & MASCARA_CARGA),
            Tipo::Cadena => write!(f, "cadena@{:#x}", self.0 & MASCARA_CARGA),
            Tipo::Simbolo => write!(f, "simbolo@{:#x}", self.0 & MASCARA_CARGA),
        }
    }
}

impl Default for Valor {
    fn default() -> Self {
        Self::INDEFINIDO
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn mide_8_bytes_y_es_copy() {
        fn es_copy<T: Copy>() {}
        es_copy::<Valor>();
        assert_eq!(core::mem::size_of::<Valor>(), 8);
    }

    #[test]
    fn los_numeros_vuelven_iguales() {
        for n in [0.0, -0.0, 1.5, -1.0, f64::MAX, f64::MIN, f64::MIN_POSITIVE, f64::INFINITY, f64::NEG_INFINITY, 1e-310]
        {
            let v = Valor::numero(n);
            assert_eq!(v.tipo(), Tipo::Numero, "{n}");
            assert_eq!(v.como_numero().unwrap().to_bits(), n.to_bits(), "{n}");
        }
    }

    #[test]
    fn todos_los_nan_son_el_mismo() {
        let raros = [f64::NAN, -f64::NAN, f64::from_bits(0x7FF0_0000_0000_0001), f64::from_bits(0xFFFF_FFFF_FFFF_FFFF)];
        for n in raros {
            let v = Valor::numero(n);
            assert_eq!(v.tipo(), Tipo::Numero);
            assert!(v.como_numero().unwrap().is_nan());
            assert_eq!(v.bits(), NAN_CANONICO);
        }
    }

    #[test]
    fn enteros_en_los_bordes() {
        for n in [0, 1, -1, i32::MAX, i32::MIN] {
            let v = Valor::entero(n);
            assert_eq!(v.tipo(), Tipo::Entero);
            assert_eq!(v.como_entero(), Some(n));
            assert_eq!(v.como_numero(), Some(n as f64));
        }
    }

    #[test]
    fn cada_tipo_es_distinto() {
        let r = Ref::nueva(0x1234).unwrap();
        let vs = [
            Valor::INDEFINIDO,
            Valor::NULO,
            Valor::VERDADERO,
            Valor::FALSO,
            Valor::entero(0),
            Valor::numero(0.0),
            Valor::referencia(r),
            Valor::cadena(r),
            Valor::simbolo(r),
        ];
        for (i, a) in vs.iter().enumerate() {
            for b in &vs[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(Valor::referencia(r).como_referencia(), Some(r));
        assert_eq!(Valor::cadena(r).como_referencia(), None, "una cadena no es un objeto");
        assert_eq!(Valor::cadena(r).ref_del_monton(), Some(r));
    }

    #[test]
    fn las_referencias_usan_47_bits() {
        let max = Ref::nueva(Ref::MAXIMA).unwrap();
        assert_eq!(Valor::referencia(max).como_referencia(), Some(max));
        assert!(Ref::nueva(Ref::MAXIMA + 1).is_none());
        assert_eq!(Ref::desde_u32(u32::MAX).numero(), u64::from(u32::MAX));
        let otra = Ref::nueva(7).unwrap();
        assert_eq!(Valor::simbolo(max).con_ref(otra), Valor::simbolo(otra), "mover conserva el tipo");
        assert_eq!(Valor::entero(3).con_ref(otra), Valor::entero(3), "un primitivo no cambia");
    }

    #[test]
    fn desde_bits_rechaza_lo_invalido() {
        for v in [Valor::NULO, Valor::numero(2.5), Valor::entero(-7), Valor::numero(f64::NAN)] {
            assert_eq!(Valor::desde_bits(v.bits()), Some(v));
        }
        assert_eq!(Valor::desde_bits(BASE), None, "etiqueta 0");
        assert_eq!(Valor::desde_bits(0x7FF0_0000_0000_0001), None, "NaN no canónico");
    }

    #[test]
    fn verdad_como_javascript() {
        assert_eq!(Valor::INDEFINIDO.verdad(), Some(false));
        assert_eq!(Valor::numero(0.0).verdad(), Some(false));
        assert_eq!(Valor::numero(-0.0).verdad(), Some(false));
        assert_eq!(Valor::numero(f64::NAN).verdad(), Some(false));
        assert_eq!(Valor::entero(0).verdad(), Some(false));
        assert_eq!(Valor::entero(5).verdad(), Some(true));
        assert_eq!(Valor::referencia(Ref::nueva(1).unwrap()).verdad(), Some(true));
        assert_eq!(Valor::cadena(Ref::nueva(1).unwrap()).verdad(), None);
    }
}
