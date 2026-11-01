//! Las piezas de bajo nivel de JPEG: el lector de bits (con el relleno `FF 00`), los códigos de
//! Huffman, la decodificación de un bloque en sus cuatro formas (secuencial, DC progresivo, AC
//! progresivo y sus refinamientos), la IDCT entera y el remuestreo del color.

use crate::Error;
use alloc::vec::Vec;

/// Orden en zigzag → posición en el bloque 8×8.
pub(crate) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
    47, 55, 62, 63,
];

/// Lector de bits del flujo comprimido: el bit más significativo primero; `FF 00` es un `FF`;
/// un marcador (`FF` y otro byte) corta el flujo (se leen ceros).
pub(crate) struct Bits<'a> {
    pub d: &'a [u8],
    pub i: usize,
    buf: u32,
    n: u32,
    /// Si se encontró un marcador.
    pub marcador: bool,
}

impl<'a> Bits<'a> {
    pub(crate) fn nuevo(d: &'a [u8], i: usize) -> Bits<'a> {
        Bits { d, i, buf: 0, n: 0, marcador: false }
    }

    fn llenar(&mut self) {
        // Rápido: si los cuatro bytes siguientes no tienen ningún 0xFF, se leen de una vez.
        if self.n == 0
            && !self.marcador
            && let Some(cuatro) = self.d.get(self.i..self.i + 4)
            && !cuatro.contains(&0xFF)
        {
            self.buf = u32::from_be_bytes([cuatro[0], cuatro[1], cuatro[2], cuatro[3]]);
            self.n = 32;
            self.i += 4;
            return;
        }
        while self.n <= 24 {
            let mut b = 0u32;
            if !self.marcador {
                match (self.d.get(self.i), self.d.get(self.i + 1)) {
                    (Some(0xFF), Some(0x00)) => {
                        b = 0xFF;
                        self.i += 2;
                    }
                    (Some(0xFF), _) | (None, _) => self.marcador = true,
                    (Some(&x), _) => {
                        b = u32::from(x);
                        self.i += 1;
                    }
                }
            }
            self.buf |= b << (24 - self.n);
            self.n += 8;
        }
    }

    fn ver(&mut self, n: u32) -> u32 {
        if self.n < n {
            self.llenar();
        }
        if self.n < n {
            // Con 32 bits ya cargados no cabe otro byte: se completa con lo que falte.
            self.llenar_resto();
        }
        self.buf >> (32 - n)
    }

    fn llenar_resto(&mut self) {
        while self.n <= 24 {
            let mut b = 0u32;
            if !self.marcador {
                match (self.d.get(self.i), self.d.get(self.i + 1)) {
                    (Some(0xFF), Some(0x00)) => {
                        b = 0xFF;
                        self.i += 2;
                    }
                    (Some(0xFF), _) | (None, _) => self.marcador = true,
                    (Some(&x), _) => {
                        b = u32::from(x);
                        self.i += 1;
                    }
                }
            }
            self.buf |= b << (24 - self.n);
            self.n += 8;
        }
    }

    fn consumir(&mut self, n: u32) {
        self.buf <<= n;
        self.n -= n;
    }

    pub(crate) fn bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.ver(n);
        self.consumir(n);
        v
    }

    pub(crate) fn bit(&mut self) -> bool {
        self.bits(1) == 1
    }

    /// `n` bits como valor con signo (la «extensión» de JPEG).
    pub(crate) fn extender(&mut self, n: u32) -> i32 {
        if n == 0 {
            return 0;
        }
        let v = self.bits(n) as i32;
        if v < 1 << (n - 1) { v - (1 << n) + 1 } else { v }
    }

    /// Tras un marcador de reinicio (RSTn): se vacía el lector y se salta el marcador.
    pub(crate) fn reiniciar(&mut self) {
        (self.buf, self.n, self.marcador) = (0, 0, false);
        while self.i + 1 < self.d.len() && !(self.d[self.i] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.i + 1])) {
            self.i += 1;
        }
        self.i = (self.i + 2).min(self.d.len());
    }
}

/// Bits de la tabla rápida de Huffman (los códigos más largos van por el camino lento).
const RAPIDO: usize = 11;

/// Un código de Huffman de JPEG con tabla rápida de 11 bits.
#[derive(Clone, Default)]
pub(crate) struct Huffman {
    rapido: Vec<u16>,
    /// Para cada longitud: (código máximo + 1, desplazamiento en `valores`).
    maximo: [i32; 18],
    desplazamiento: [i32; 17],
    valores: Vec<u8>,
}

impl Huffman {
    pub(crate) fn nuevo(cuentas: &[u8; 16], valores: &[u8]) -> Huffman {
        let mut h = Huffman {
            rapido: alloc::vec![0; 1 << RAPIDO],
            maximo: [0; 18],
            desplazamiento: [0; 17],
            valores: valores.to_vec(),
        };
        let (mut codigo, mut k) = (0i32, 0i32);
        for l in 1..=16usize {
            h.desplazamiento[l] = k - codigo;
            for _ in 0..cuentas[l - 1] {
                if l <= RAPIDO {
                    let primero = (codigo << (RAPIDO - l)) as usize;
                    for j in 0..(1usize << (RAPIDO - l)) {
                        if let (Some(r), Some(&v)) = (h.rapido.get_mut(primero + j), valores.get(k as usize)) {
                            *r = (u16::from(v) << 8) | l as u16;
                        }
                    }
                }
                codigo += 1;
                k += 1;
            }
            h.maximo[l] = codigo << (16 - l);
            codigo <<= 1;
        }
        h.maximo[17] = i32::MAX;
        h
    }

    pub(crate) fn decodificar(&self, b: &mut Bits<'_>) -> Result<u8, Error> {
        let e = self.rapido[b.ver(RAPIDO as u32) as usize];
        if e != 0 {
            b.consumir(u32::from(e & 0xFF));
            return Ok((e >> 8) as u8);
        }
        let v = b.ver(16) as i32;
        let l =
            (RAPIDO + 1..=17).find(|&l| v < self.maximo[l]).ok_or(Error::Datos("código de Huffman JPEG no válido"))?;
        if l > 16 {
            return Err(Error::Datos("código de Huffman JPEG no válido"));
        }
        b.consumir(l as u32);
        let codigo = v >> (16 - l);
        self.valores
            .get((codigo + self.desplazamiento[l]) as usize)
            .copied()
            .ok_or(Error::Datos("código JPEG fuera de la tabla"))
    }
}

/// Estado de un escaneo progresivo: banda espectral, aproximación sucesiva y bloques vacíos.
pub(crate) struct Escaneo {
    pub inicio: usize,
    pub fin: usize,
    pub alto: u8,
    pub bajo: u8,
    pub eob: u32,
}

/// Bloque de un escaneo secuencial (baseline): DC y los 63 AC.
pub(crate) fn bloque_secuencial(
    b: &mut Bits<'_>,
    bloque: &mut [i16; 64],
    dc: &Huffman,
    ac: &Huffman,
    pred: &mut i32,
) -> Result<(), Error> {
    let t = dc.decodificar(b)?;
    *pred += b.extender(u32::from(t));
    bloque[0] = *pred as i16;
    let mut k = 1;
    while k < 64 {
        let rs = ac.decodificar(b)?;
        let (r, s) = (usize::from(rs >> 4), u32::from(rs & 15));
        if s == 0 {
            if r != 15 {
                break; // fin de bloque
            }
            k += 16;
            continue;
        }
        k += r;
        if k > 63 {
            break;
        }
        bloque[ZIGZAG[k]] = b.extender(s) as i16;
        k += 1;
    }
    Ok(())
}

/// DC de un escaneo progresivo: primera pasada o refinamiento de un bit.
pub(crate) fn bloque_dc(
    b: &mut Bits<'_>,
    bloque: &mut [i16; 64],
    dc: &Huffman,
    pred: &mut i32,
    e: &Escaneo,
) -> Result<(), Error> {
    if e.alto == 0 {
        let t = dc.decodificar(b)?;
        *pred += b.extender(u32::from(t));
        bloque[0] = (*pred * (1 << e.bajo)) as i16;
    } else if b.bit() {
        bloque[0] |= 1 << e.bajo;
    }
    Ok(())
}

/// AC de un escaneo progresivo, primera pasada (con series de bloques vacíos «EOB»).
pub(crate) fn bloque_ac_primero(
    b: &mut Bits<'_>,
    bloque: &mut [i16; 64],
    ac: &Huffman,
    e: &mut Escaneo,
) -> Result<(), Error> {
    if e.eob > 0 {
        e.eob -= 1;
        return Ok(());
    }
    let mut k = e.inicio;
    while k <= e.fin {
        let rs = ac.decodificar(b)?;
        let (r, s) = (u32::from(rs >> 4), u32::from(rs & 15));
        if s == 0 {
            if r < 15 {
                e.eob = (1 << r) - 1 + b.bits(r);
                break;
            }
            k += 16;
            continue;
        }
        k += r as usize;
        if k > 63 {
            break;
        }
        bloque[ZIGZAG[k]] = (b.extender(s) * (1 << e.bajo)) as i16;
        k += 1;
    }
    Ok(())
}

/// Corrige un coeficiente ya no nulo con un bit de refinamiento.
fn refinar(b: &mut Bits<'_>, p: &mut i16, bit: i16) {
    if b.bit() && *p & bit == 0 {
        if *p > 0 { *p += bit } else { *p -= bit }
    }
}

/// AC de un escaneo progresivo, refinamiento (el caso más enrevesado de JPEG).
pub(crate) fn bloque_ac_refinar(
    b: &mut Bits<'_>,
    bloque: &mut [i16; 64],
    ac: &Huffman,
    e: &mut Escaneo,
) -> Result<(), Error> {
    let bit = 1i16 << e.bajo;
    let mut k = e.inicio;
    if e.eob == 0 {
        while k <= e.fin {
            let rs = ac.decodificar(b)?;
            let (mut r, s) = (i32::from(rs >> 4), rs & 15);
            let mut valor = 0i16;
            if s == 0 {
                if r < 15 {
                    e.eob = (1 << r) + b.bits(r as u32);
                    break;
                }
            } else {
                valor = if b.bit() { bit } else { -bit };
            }
            while k <= e.fin {
                let p = &mut bloque[ZIGZAG[k]];
                k += 1;
                if *p != 0 {
                    refinar(b, p, bit);
                } else {
                    if r == 0 {
                        *p = valor;
                        break;
                    }
                    r -= 1;
                }
            }
        }
    }
    if e.eob > 0 {
        while k <= e.fin {
            let p = &mut bloque[ZIGZAG[k]];
            if *p != 0 {
                refinar(b, p, bit);
            }
            k += 1;
        }
        e.eob -= 1;
    }
    Ok(())
}

/// IDCT entera 8×8 (la de stb_image, la misma que usan jpeg-decoder y otros): coeficientes
/// ya cuantizados → 64 muestras de 0 a 255.
pub(crate) fn idct(bloque: &[i16; 64], cuant: &[u16; 64], salida: &mut [u8], paso: usize) {
    // Solo DC (muy común): el resultado es un único valor, el mismo que daría la IDCT entera.
    if bloque[1..].iter().all(|&c| c == 0) {
        let dc = i32::from(bloque[0]) * i32::from(cuant[0]) * 4 * 4096;
        let v = ((dc + 65536 + (128 << 17)) >> 17).clamp(0, 255) as u8;
        for fila in 0..8 {
            salida[fila * paso..fila * paso + 8].fill(v);
        }
        return;
    }
    let f = |x: f32| (x * 4096.0 + 0.5) as i32;
    let mut v = [0i32; 64];
    let d: [i32; 64] = core::array::from_fn(|i| i32::from(bloque[i]) * i32::from(cuant[i]));
    for i in 0..8 {
        let c = |k: usize| d[i + k * 8];
        if (1..8).all(|k| c(k) == 0) {
            for k in 0..8 {
                v[i + k * 8] = c(0) * 4;
            }
            continue;
        }
        let [x0, x1, x2, x3, t0, t1, t2, t3] = idct_1d([c(0), c(1), c(2), c(3), c(4), c(5), c(6), c(7)], f);
        let (x0, x1, x2, x3) = (x0 + 512, x1 + 512, x2 + 512, x3 + 512);
        for (k, val) in [
            (0, x0 + t3),
            (7, x0 - t3),
            (1, x1 + t2),
            (6, x1 - t2),
            (2, x2 + t1),
            (5, x2 - t1),
            (3, x3 + t0),
            (4, x3 - t0),
        ] {
            v[i + k * 8] = val >> 10;
        }
    }
    for i in 0..8 {
        let fila = &v[i * 8..i * 8 + 8];
        let [x0, x1, x2, x3, t0, t1, t2, t3] =
            idct_1d([fila[0], fila[1], fila[2], fila[3], fila[4], fila[5], fila[6], fila[7]], f);
        let extra = 65536 + (128 << 17);
        let (x0, x1, x2, x3) = (x0 + extra, x1 + extra, x2 + extra, x3 + extra);
        let o = &mut salida[i * paso..i * paso + 8];
        for (k, val) in [
            (0, x0 + t3),
            (7, x0 - t3),
            (1, x1 + t2),
            (6, x1 - t2),
            (2, x2 + t1),
            (5, x2 - t1),
            (3, x3 + t0),
            (4, x3 - t0),
        ] {
            o[k] = (val >> 17).clamp(0, 255) as u8;
        }
    }
}

fn idct_1d(s: [i32; 8], f: impl Fn(f32) -> i32) -> [i32; 8] {
    let p1 = (s[2] + s[6]) * f(0.541_196_1);
    let t2 = p1 + s[6] * f(-1.847_759_1);
    let t3 = p1 + s[2] * f(0.765_366_85);
    let t0 = (s[0] + s[4]) * 4096;
    let t1 = (s[0] - s[4]) * 4096;
    let (x0, x3, x1, x2) = (t0 + t3, t0 - t3, t1 + t2, t1 - t2);
    let (t0, t1, t2, t3) = (s[7], s[5], s[3], s[1]);
    let (p3, p4, p1, p2) = (t0 + t2, t1 + t3, t0 + t3, t1 + t2);
    let p5 = (p3 + p4) * f(1.175_875_6);
    let t0 = t0 * f(0.298_631_33);
    let t1 = t1 * f(2.053_12);
    let t2 = t2 * f(3.072_711);
    let t3 = t3 * f(1.501_321_1);
    let p1 = p5 + p1 * f(-0.899_976_2);
    let p2 = p5 + p2 * f(-2.562_915_3);
    let p3 = p3 * f(-1.961_570_6);
    let p4 = p4 * f(-0.390_180_64);
    [x0, x1, x2, x3, t0 + p1 + p3, t1 + p2 + p4, t2 + p2 + p3, t3 + p1 + p4]
}

/// Remuestrea un plano de `w × h` muestras a `(w·fx) × (h·fy)`: el «triángulo» de libjpeg para
/// 2×1 y 2×2 (como stb_image y jpeg-decoder), y repetición para el resto.
pub(crate) fn remuestrear(plano: &[u8], (w, h): (usize, usize), paso: usize, (fx, fy): (usize, usize)) -> Vec<u8> {
    let (sw, sh) = (w * fx, h * fy);
    let mut s = alloc::vec![0u8; sw * sh];
    let fila = |y: usize| &plano[y.min(h - 1) * paso..y.min(h - 1) * paso + w];
    for y in 0..sh {
        let salida = &mut s[y * sw..(y + 1) * sw];
        match (fx, fy) {
            (2, 1) => h2v1(fila(y), salida),
            (2, 2) => {
                let (cerca, lejos) = if y % 2 == 0 { (y / 2, (y / 2).saturating_sub(1)) } else { (y / 2, y / 2 + 1) };
                h2v2(fila(cerca), fila(lejos), salida);
            }
            _ => {
                let origen = fila(y / fy);
                for (x, p) in salida.iter_mut().enumerate() {
                    *p = origen[x / fx];
                }
            }
        }
    }
    s
}

fn h2v1(e: &[u8], s: &mut [u8]) {
    let w = e.len();
    if w == 1 {
        s[0] = e[0];
        s[1] = e[0];
        return;
    }
    let e = |i: usize| u16::from(e[i]);
    s[0] = e(0) as u8;
    s[1] = ((e(0) * 3 + e(1) + 2) >> 2) as u8;
    for i in 1..w - 1 {
        let m = 3 * e(i) + 2;
        s[i * 2] = ((m + e(i - 1)) >> 2) as u8;
        s[i * 2 + 1] = ((m + e(i + 1)) >> 2) as u8;
    }
    s[(w - 1) * 2] = ((e(w - 1) * 3 + e(w - 2) + 2) >> 2) as u8;
    s[(w - 1) * 2 + 1] = e(w - 1) as u8;
}

fn h2v2(cerca: &[u8], lejos: &[u8], s: &mut [u8]) {
    let w = cerca.len();
    let t = |i: usize| 3 * u32::from(cerca[i]) + u32::from(lejos[i]);
    if w == 1 {
        let v = ((t(0) + 2) >> 2) as u8;
        s[0] = v;
        s[1] = v;
        return;
    }
    let mut t1 = t(0);
    s[0] = ((t1 + 2) >> 2) as u8;
    for i in 1..w {
        let t0 = t1;
        t1 = t(i);
        s[i * 2 - 1] = ((3 * t0 + t1 + 8) >> 4) as u8;
        s[i * 2] = ((3 * t1 + t0 + 8) >> 4) as u8;
    }
    s[w * 2 - 1] = ((t1 + 2) >> 2) as u8;
}
