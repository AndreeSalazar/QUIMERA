//! Matemáticas sin `std`: `floor`, `ceil`, `round`, `sqrt`, `ln`, `exp` y `pow`.
//!
//! `core` no trae estas funciones (están en la biblioteca del sistema). Se calculan con
//! reducción de rango y series: `ln` con la serie de `atanh` sobre la mantisa y `exp` con
//! Taylor sobre un resto de `|r| ≤ ln(2)/2`. La precisión ronda el último bit de un `f64`.

const LN2: f64 = core::f64::consts::LN_2;
/// ln 2 partido en dos (Cody-Waite, como fdlibm): `LN2_ALTO` tiene ceros en los bits bajos, así
/// que `k · LN2_ALTO` es exacto y el error de redondear ln 2 queda solo en `k · LN2_BAJO`.
const LN2_ALTO: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN2_BAJO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);

fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

/// `Math.floor`.
pub fn suelo(x: f64) -> f64 {
    if !x.is_finite() || abs(x) >= 4.5e15 {
        return x;
    }
    let t = (x as i64) as f64;
    if t > x { t - 1.0 } else { t }
}

/// `Math.ceil`.
pub fn techo(x: f64) -> f64 {
    -suelo(-x)
}

/// `Math.round`: hacia +∞ en los empates (`Math.round(-2.5)` es -2).
pub fn redondear(x: f64) -> f64 {
    if !x.is_finite() || abs(x) >= 4.5e15 {
        return x;
    }
    let r = suelo(x + 0.5);
    // -0.4 redondea a -0 en JavaScript.
    if r == 0.0 && x < 0.0 { -0.0 } else { r }
}

/// `Math.sqrt`: Newton desde una buena aproximación de los bits.
pub fn raiz(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    let mut y = f64::from_bits((x.to_bits() >> 1) + (1023u64 << 51));
    for _ in 0..6 {
        y = 0.5 * (y + x / y);
    }
    y
}

/// `Math.log` (logaritmo natural).
pub fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    // x = m · 2^e con m en [1, 2); los subnormales se escalan primero.
    let (x, ajuste) = if x < f64::MIN_POSITIVE { (x * (1u64 << 54) as f64, -54i64) } else { (x, 0) };
    let bits = x.to_bits();
    let mut e = ((bits >> 52) & 0x7ff) as i64 - 1023 + ajuste;
    let mut m = f64::from_bits((bits & ((1u64 << 52) - 1)) | (1023u64 << 52));
    // Centrar la mantisa en [√½, √2) para que la serie converja más rápido.
    if m > core::f64::consts::SQRT_2 {
        m /= 2.0;
        e += 1;
    }
    // ln(m) = 2·atanh(t) con t = (m-1)/(m+1), |t| < 0,172.
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let (mut termino, mut suma) = (t, 0.0);
    for k in 0..30 {
        suma += termino / (2 * k + 1) as f64;
        termino *= t2;
    }
    let e = e as f64;
    e * LN2_ALTO + (2.0 * suma + e * LN2_BAJO)
}

/// `Math.exp`.
pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 709.8 {
        return f64::INFINITY;
    }
    if x < -745.2 {
        return 0.0;
    }
    // x = k·ln2 + r con |r| ≤ ln2/2; exp(x) = 2^k · exp(r).
    let k = suelo(x / LN2 + 0.5);
    let r = (x - k * LN2_ALTO) - k * LN2_BAJO;
    let (mut termino, mut suma) = (1.0, 1.0);
    for n in 1..25 {
        termino *= r / n as f64;
        suma += termino;
    }
    escalar(suma, k as i64)
}

/// `x · 2^k` sin perder los subnormales.
fn escalar(mut x: f64, mut k: i64) -> f64 {
    while k > 1000 {
        x *= f64::from_bits((1023u64 + 1000) << 52);
        k -= 1000;
    }
    while k < -1000 {
        x *= f64::from_bits((1023u64 - 1000) << 52);
        k += 1000;
    }
    x * f64::from_bits(((1023 + k) as u64) << 52)
}

/// `x ** y` / `Math.pow`, según ECMAScript (Number::exponentiate).
pub fn potencia(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return 1.0;
    }
    if x.is_nan() || (abs(x) == 1.0 && y.is_infinite()) {
        return f64::NAN;
    }
    let entero = y > -2147483648.0 && y < 2147483648.0 && (y as i32) as f64 == y;
    if entero {
        let (mut base, mut e, mut r) = (x, (y as i64).unsigned_abs(), 1.0f64);
        while e > 0 {
            if e & 1 == 1 {
                r *= base;
            }
            base *= base;
            e >>= 1;
        }
        return if y < 0.0 { 1.0 / r } else { r };
    }
    if x < 0.0 && y.is_finite() {
        return f64::NAN; // base negativa y exponente no entero
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    exp(y * ln(x))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn cerca(a: f64, b: f64) -> bool {
        a == b || abs(a - b) <= 4.0 * f64::EPSILON * abs(b)
    }

    #[test]
    fn logaritmos_y_exponenciales_al_ultimo_bit() {
        // El error de ln(x), aunque sea de un bit, exp lo multiplica por |ln x|: la tolerancia crece con él.
        for x in [1e-300, 1e-10, 0.1, 0.5, 1.0, 2.0, core::f64::consts::E, 10.0, 12345.678, 1e300] {
            let tolerancia = 4.0 * f64::EPSILON * (abs(ln(x)) + 1.0) * x;
            assert!(abs(exp(ln(x)) - x) <= tolerancia, "exp(ln({x})) = {}", exp(ln(x)));
        }
        assert!(cerca(ln(10.0), core::f64::consts::LN_10));
        assert!(cerca(ln(1e-300), -300.0 * core::f64::consts::LN_10));
        assert!(cerca(LN2_ALTO + LN2_BAJO, LN2));
        assert!(cerca(ln(core::f64::consts::E), 1.0));
        assert!(cerca(ln(1024.0), 10.0 * LN2));
        assert!(cerca(exp(1.0), core::f64::consts::E));
        assert_eq!((ln(0.0), ln(-1.0).is_nan(), exp(-1000.0)), (f64::NEG_INFINITY, true, 0.0));
    }

    #[test]
    fn potencias_como_javascript() {
        assert_eq!(potencia(2.0, 10.0), 1024.0);
        assert!(cerca(potencia(2.0, 0.5), core::f64::consts::SQRT_2));
        assert!(cerca(potencia(core::f64::consts::E, 2.5), exp(2.5)));
        assert!(potencia(-8.0, 1.0 / 3.0).is_nan());
        assert!(potencia(1.0, f64::INFINITY).is_nan());
        assert_eq!(potencia(f64::NAN, 0.0), 1.0);
    }

    #[test]
    fn redondeos_y_raiz() {
        assert_eq!((suelo(-1.5), techo(-1.5), redondear(2.5), redondear(-2.5)), (-2.0, -1.0, 3.0, -2.0));
        assert!(redondear(-0.4).is_sign_negative());
        assert_eq!(raiz(144.0), 12.0);
        assert!(cerca(raiz(2.0), core::f64::consts::SQRT_2));
    }
}
