//! DEFLATE (RFC 1951) y zlib (RFC 1950): la compresión de PNG (y de `gzip`, las fuentes WOFF...).
//!
//! Bloques almacenados, con Huffman fijo y con Huffman dinámico. Cada código se decodifica con
//! una tabla de 11 bits (casi todos caben) y, si no, código a código como el `puff` de zlib.

use crate::Error;
use alloc::vec::Vec;

/// Lector de bits: DEFLATE empaqueta desde el bit menos significativo.
struct Bits<'a> {
    d: &'a [u8],
    i: usize,
    buf: u64,
    n: u32,
}

impl Bits<'_> {
    fn llenar(&mut self) {
        // Rápido: ocho bytes de golpe (se cuentan solo los que caben enteros en el búfer).
        if let Some(ocho) = self.d.get(self.i..self.i + 8) {
            let v = u64::from_le_bytes([ocho[0], ocho[1], ocho[2], ocho[3], ocho[4], ocho[5], ocho[6], ocho[7]]);
            self.buf |= v << self.n;
            let bytes = (63 - self.n) / 8;
            self.i += bytes as usize;
            self.n += bytes * 8;
            return;
        }
        while self.n <= 56 {
            let b = self.d.get(self.i).copied().unwrap_or(0);
            self.i += 1;
            self.buf |= u64::from(b) << self.n;
            self.n += 8;
        }
    }

    fn ver(&mut self, n: u32) -> u32 {
        if self.n < n {
            self.llenar();
        }
        (self.buf & ((1u64 << n) - 1)) as u32
    }

    fn tomar(&mut self, n: u32) -> u32 {
        let v = self.ver(n);
        self.buf >>= n;
        self.n -= n;
        v
    }

    /// Si se leyó más allá del final (con margen para lo leído por adelantado).
    fn agotado(&self) -> bool {
        self.i > self.d.len() + 8 && (self.i - self.d.len()) * 8 > self.n as usize + 64
    }

    /// Descarta los bits hasta el siguiente byte.
    fn alinear(&mut self) {
        let sobra = self.n % 8;
        self.tomar(sobra);
    }
}

/// Un código de Huffman canónico.
struct Huffman {
    /// Tabla de 11 bits: (símbolo << 4) | longitud; 0 = el código es más largo.
    rapido: Vec<u16>,
    cuenta: [u16; 16],
    simbolos: Vec<u16>,
}

const RAPIDO: u32 = 11;

impl Huffman {
    fn nuevo(longitudes: &[u8]) -> Result<Huffman, Error> {
        let mut cuenta = [0u16; 16];
        for &l in longitudes {
            cuenta[l as usize] += 1;
        }
        cuenta[0] = 0;
        let mut desplazamiento = [0u16; 16];
        for l in 1..16 {
            desplazamiento[l] = desplazamiento[l - 1] + cuenta[l - 1];
        }
        let mut simbolos = alloc::vec![0u16; longitudes.len()];
        for (s, &l) in longitudes.iter().enumerate() {
            if l != 0 {
                simbolos[desplazamiento[l as usize] as usize] = s as u16;
                desplazamiento[l as usize] += 1;
            }
        }
        let mut rapido = alloc::vec![0u16; 1 << RAPIDO];
        let mut codigo = 0u32;
        let mut k = 0usize;
        for l in 1..=15u32 {
            for _ in 0..cuenta[l as usize] {
                if l <= RAPIDO {
                    // El código va al revés en el flujo (bit menos significativo primero).
                    let invertido = codigo.reverse_bits() >> (32 - l);
                    let mut j = invertido;
                    while j < (1 << RAPIDO) {
                        rapido[j as usize] = (simbolos[k] << 4) | l as u16;
                        j += 1 << l;
                    }
                }
                codigo += 1;
                k += 1;
            }
            codigo <<= 1;
        }
        Ok(Huffman { rapido, cuenta, simbolos })
    }

    fn decodificar(&self, b: &mut Bits<'_>) -> Result<u16, Error> {
        let e = self.rapido[b.ver(RAPIDO) as usize];
        if e != 0 {
            b.tomar(u32::from(e & 15));
            return Ok(e >> 4);
        }
        // Código largo: bit a bit (como puff).
        let (mut codigo, mut primero, mut indice) = (0i32, 0i32, 0i32);
        for l in 1..16 {
            codigo |= b.tomar(1) as i32;
            let c = i32::from(self.cuenta[l]);
            if codigo - c < primero {
                return Ok(self.simbolos[(indice + codigo - primero) as usize]);
            }
            indice += c;
            primero = (primero + c) << 1;
            codigo <<= 1;
        }
        Err(Error::Datos("código de Huffman no válido"))
    }
}

const BASE_LONGITUD: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const EXTRA_LONGITUD: [u8; 29] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const BASE_DISTANCIA: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const EXTRA_DISTANCIA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// Orden de las longitudes del código de longitudes en un bloque dinámico.
const ORDEN: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Datos zlib (cabecera de 2 bytes, DEFLATE y suma Adler-32) → los bytes originales.
pub fn zlib(datos: &[u8], capacidad: usize) -> Result<Vec<u8>, Error> {
    let [cmf, flg, ..] = *datos else { return Err(Error::Datos("zlib demasiado corto")) };
    if cmf & 0x0F != 8 || (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err(Error::Datos("cabecera zlib no válida"));
    }
    if flg & 0x20 != 0 {
        return Err(Error::NoSoportado("zlib con diccionario"));
    }
    let mut salida = Vec::with_capacity(capacidad);
    inflar(&datos[2..], &mut salida)?;
    Ok(salida)
}

/// DEFLATE crudo → añade los bytes a `salida`.
pub fn inflar(datos: &[u8], salida: &mut Vec<u8>) -> Result<(), Error> {
    inflar_contando(datos, salida).map(|_| ())
}

/// Como [`inflar`], y devuelve cuántos bytes de `datos` ocupaba el DEFLATE (lo que sigue es,
/// en gzip, su pie).
pub(crate) fn inflar_contando(datos: &[u8], salida: &mut Vec<u8>) -> Result<usize, Error> {
    let mut b = Bits { d: datos, i: 0, buf: 0, n: 0 };
    loop {
        let ultimo = b.tomar(1) == 1;
        match b.tomar(2) {
            0 => almacenado(&mut b, salida)?,
            1 => {
                let (lit, dist) = fijos()?;
                comprimido(&mut b, salida, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = dinamicos(&mut b)?;
                comprimido(&mut b, salida, &lit, &dist)?;
            }
            _ => return Err(Error::Datos("tipo de bloque DEFLATE no válido")),
        }
        if b.agotado() {
            return Err(Error::Datos("DEFLATE cortado"));
        }
        if ultimo {
            // Los bytes enteros que quedan en el búfer se leyeron por adelantado.
            return Ok(b.i - (b.n / 8) as usize);
        }
    }
}

fn almacenado(b: &mut Bits<'_>, salida: &mut Vec<u8>) -> Result<(), Error> {
    b.alinear();
    let largo = b.tomar(16);
    let complemento = b.tomar(16);
    if largo != !complemento & 0xFFFF {
        return Err(Error::Datos("bloque almacenado con longitud incoherente"));
    }
    for _ in 0..largo {
        salida.push(b.tomar(8) as u8);
    }
    Ok(())
}

fn fijos() -> Result<(Huffman, Huffman), Error> {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    Ok((Huffman::nuevo(&l)?, Huffman::nuevo(&[5u8; 30])?))
}

fn dinamicos(b: &mut Bits<'_>) -> Result<(Huffman, Huffman), Error> {
    let nlit = b.tomar(5) as usize + 257;
    let ndist = b.tomar(5) as usize + 1;
    let ncod = b.tomar(4) as usize + 4;
    let mut lc = [0u8; 19];
    for &o in ORDEN.iter().take(ncod) {
        lc[o] = b.tomar(3) as u8;
    }
    let codigos = Huffman::nuevo(&lc)?;
    let mut longitudes = alloc::vec![0u8; nlit + ndist];
    let mut i = 0;
    while i < nlit + ndist {
        let s = codigos.decodificar(b)?;
        let (valor, veces) = match s {
            0..=15 => (s as u8, 1),
            16 => {
                let previo = *longitudes.get(i.wrapping_sub(1)).ok_or(Error::Datos("repetición sin anterior"))?;
                (previo, 3 + b.tomar(2) as usize)
            }
            17 => (0, 3 + b.tomar(3) as usize),
            _ => (0, 11 + b.tomar(7) as usize),
        };
        if i + veces > longitudes.len() {
            return Err(Error::Datos("demasiadas longitudes"));
        }
        longitudes[i..i + veces].fill(valor);
        i += veces;
    }
    Ok((Huffman::nuevo(&longitudes[..nlit])?, Huffman::nuevo(&longitudes[nlit..])?))
}

fn comprimido(b: &mut Bits<'_>, salida: &mut Vec<u8>, lit: &Huffman, dist: &Huffman) -> Result<(), Error> {
    loop {
        let s = lit.decodificar(b)?;
        match s {
            0..=255 => salida.push(s as u8),
            256 => return Ok(()),
            _ => {
                let k = (s - 257) as usize;
                if k >= 29 {
                    return Err(Error::Datos("longitud no válida"));
                }
                let largo = BASE_LONGITUD[k] as usize + b.tomar(u32::from(EXTRA_LONGITUD[k])) as usize;
                let d = dist.decodificar(b)? as usize;
                if d >= 30 {
                    return Err(Error::Datos("distancia no válida"));
                }
                let distancia = BASE_DISTANCIA[d] as usize + b.tomar(u32::from(EXTRA_DISTANCIA[d])) as usize;
                if distancia > salida.len() {
                    return Err(Error::Datos("distancia antes del principio"));
                }
                let inicio = salida.len() - distancia;
                if distancia >= largo {
                    salida.extend_from_within(inicio..inicio + largo);
                } else {
                    // Las copias que se solapan (distancia < largo) repiten lo recién copiado.
                    for j in 0..largo {
                        let x = salida[inicio + j];
                        salida.push(x);
                    }
                }
            }
        }
        if b.agotado() {
            return Err(Error::Datos("DEFLATE cortado"));
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn almacenado_y_fijo() {
        // "hola" en un bloque almacenado.
        let mut v = Vec::new();
        inflar(&[0x01, 4, 0, 0xFB, 0xFF, b'h', b'o', b'l', b'a'], &mut v).unwrap();
        assert_eq!(v, b"hola");
        // zlib de "aaaaaaaaaa" con Huffman fijo y una repetición (generado con zlib.compress).
        let z = [0x78, 0x9C, 0x4B, 0x4C, 0x84, 0x01, 0x00, 0x14, 0xE1, 0x03, 0xCB];
        assert_eq!(zlib(&z, 16).unwrap(), b"aaaaaaaaaa");
        assert!(zlib(&[0x78, 0x9C, 0xFF], 4).is_err());
    }
}
