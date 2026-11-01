//! Clases de caracteres (`[a-z]`, `\d`, `\s`...): rangos ordenados y un mapa de bits para ASCII.

use alloc::vec::Vec;

/// Un conjunto de caracteres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Clase {
    /// Rangos inclusivos, ordenados y sin solaparse.
    rangos: Vec<(u32, u32)>,
    negada: bool,
    /// Pertenencia de los 128 caracteres ASCII (sin aplicar `negada`).
    ascii: [u64; 2],
}

/// Los espacios de `\s` en JavaScript (espacios y fines de línea de Unicode).
const ESPACIOS: &[(u32, u32)] = &[
    (0x09, 0x0d),
    (0x20, 0x20),
    (0xa0, 0xa0),
    (0x1680, 0x1680),
    (0x2000, 0x200a),
    (0x2028, 0x2029),
    (0x202f, 0x202f),
    (0x205f, 0x205f),
    (0x3000, 0x3000),
    (0xfeff, 0xfeff),
];
const DIGITOS: &[(u32, u32)] = &[(0x30, 0x39)];
const PALABRA: &[(u32, u32)] = &[(0x30, 0x39), (0x41, 0x5a), (0x5f, 0x5f), (0x61, 0x7a)];

/// Las clases con nombre: `\d`, `\w`, `\s` (y sus negadas en mayúscula).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Abreviada {
    Digito,
    Palabra,
    Espacio,
}

impl Abreviada {
    fn rangos(self) -> &'static [(u32, u32)] {
        match self {
            Abreviada::Digito => DIGITOS,
            Abreviada::Palabra => PALABRA,
            Abreviada::Espacio => ESPACIOS,
        }
    }
}

/// Lo que se va juntando mientras se lee `[...]`.
#[derive(Debug, Default, Clone)]
pub(crate) struct Constructor {
    rangos: Vec<(u32, u32)>,
}

impl Constructor {
    pub(crate) fn rango(&mut self, a: u32, b: u32) {
        self.rangos.push((a.min(b), a.max(b)));
    }

    pub(crate) fn abreviada(&mut self, a: Abreviada, negada: bool) {
        if negada {
            self.rangos.extend(complemento(a.rangos()));
        } else {
            self.rangos.extend_from_slice(a.rangos());
        }
    }

    /// Añade las otras formas (mayúscula/minúscula) de cada carácter, para la bandera `i`.
    /// Los rangos enormes (`Ĩ-￿`) ya contienen casi todo y se dejan como están.
    pub(crate) fn sin_caso(&mut self) {
        let mut extra = Vec::new();
        for &(a, b) in &self.rangos {
            if b - a > 2000 {
                continue;
            }
            for c in (a..=b).filter_map(char::from_u32) {
                for otro in [plegar(c), mayuscula(c)] {
                    if otro != c {
                        extra.push((otro as u32, otro as u32));
                    }
                }
            }
        }
        self.rangos.extend(extra);
    }

    pub(crate) fn terminar(mut self, negada: bool) -> Clase {
        self.rangos.sort_unstable();
        let mut unidos: Vec<(u32, u32)> = Vec::with_capacity(self.rangos.len());
        for (a, b) in self.rangos {
            match unidos.last_mut() {
                Some(u) if a <= u.1.saturating_add(1) => u.1 = u.1.max(b),
                _ => unidos.push((a, b)),
            }
        }
        let mut ascii = [0u64; 2];
        for &(a, b) in &unidos {
            for c in a..=b.min(127) {
                ascii[(c / 64) as usize] |= 1 << (c % 64);
            }
        }
        Clase { rangos: unidos, negada, ascii }
    }
}

/// Los caracteres que no están en `rangos` (ordenados y sin solaparse).
fn complemento(rangos: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut v = Vec::new();
    let mut desde = 0u32;
    for &(a, b) in rangos {
        if a > desde {
            v.push((desde, a - 1));
        }
        desde = b + 1;
    }
    if desde <= 0x10_ffff {
        v.push((desde, 0x10_ffff));
    }
    v
}

impl Clase {
    /// Si `c` pertenece a la clase.
    #[inline]
    pub(crate) fn contiene(&self, c: char) -> bool {
        let n = c as u32;
        let dentro = if n < 128 {
            self.ascii[(n / 64) as usize] & (1 << (n % 64)) != 0
        } else {
            self.rangos
                .binary_search_by(|&(a, b)| {
                    if b < n {
                        core::cmp::Ordering::Less
                    } else if a > n {
                        core::cmp::Ordering::Greater
                    } else {
                        core::cmp::Ordering::Equal
                    }
                })
                .is_ok()
        };
        dentro != self.negada
    }
}

/// La forma canónica para comparar sin distinguir mayúsculas (la minúscula simple).
#[inline]
pub(crate) fn plegar(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut m = c.to_lowercase();
    match (m.next(), m.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

fn mayuscula(c: char) -> char {
    let mut m = c.to_uppercase();
    match (m.next(), m.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

/// Un carácter de palabra para `\b`.
#[inline]
pub(crate) fn es_palabra(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Fin de línea para `.`, `^` y `$` en modo multilínea.
#[inline]
pub(crate) fn es_fin_de_linea(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}
