//! Trigonometría sin `std`: `sin`, `cos`, `tan`, `atan`, `atan2`, `asin` y `acos`.
//!
//! Portadas de **fdlibm** (Sun Microsystems, 1993), la biblioteca que usan V8 y SpiderMonkey,
//! para dar los mismos resultados que Chrome: reducción de rango de Cody-Waite a [-π/4, π/4]
//! y los polinomios de `__kernel_sin` y `__kernel_cos`; `atan` con sus tablas y su polinomio.
//!
//! También `tan` (`__kernel_tan`), `asin`, `acos` y las hiperbólicas inversas. Falta una cosa:
//! para |x| mayor que unos 10⁹ la reducción de rango pierde precisión (fdlibm usa ahí
//! Payne-Hanek, que todavía no está).

// Las constantes van tal cual en fdlibm (con más decimales de los que caben, y algunas son π/4 o
// 2/π escritas a mano): así se puede comprobar línea a línea contra el original.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use crate::matematicas::raiz;

fn abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

/// El exponente sin sesgo de `x` (para decidir cuántas vueltas da la reducción).
fn exponente(x: f64) -> i32 {
    ((x.to_bits() >> 52) & 0x7ff) as i32
}

/// Redondeo al entero más cercano (para el cociente de la reducción; |x| no es enorme).
fn entero_cercano(x: f64) -> f64 {
    let t = (x as i64) as f64;
    let d = x - t;
    if d >= 0.5 {
        t + 1.0
    } else if d <= -0.5 {
        t - 1.0
    } else {
        t
    }
}

// π/2 en tres trozos, cada uno con ceros en los bits bajos (Cody-Waite, como fdlibm).
const INV_PIO2: f64 = 6.366_197_723_675_813_824_33e-1;
const PIO2_1: f64 = 1.570_796_326_734_125_614_17;
const PIO2_1T: f64 = 6.077_100_506_506_192_249_32e-11;
const PIO2_2: f64 = 6.077_100_506_303_965_976_60e-11;
const PIO2_2T: f64 = 2.022_266_248_795_950_631_54e-21;
const PIO2_3: f64 = 2.022_266_248_711_166_455_80e-21;
const PIO2_3T: f64 = 8.478_427_660_368_899_569_97e-32;

/// `x = n·π/2 + (y0 + y1)` con |y0 + y1| ≤ π/4: devuelve (n, y0, y1).
fn reducir(x: f64) -> (i64, f64, f64) {
    let n = entero_cercano(x * INV_PIO2);
    let mut r = x - n * PIO2_1;
    let mut w = n * PIO2_1T;
    let j = exponente(x);
    let mut y0 = r - w;
    if j - exponente(y0) > 16 {
        // Se perdieron bits: otra vuelta con el segundo trozo.
        let t = r;
        w = n * PIO2_2;
        r = t - w;
        w = n * PIO2_2T - ((t - r) - w);
        y0 = r - w;
        if j - exponente(y0) > 49 {
            let t = r;
            w = n * PIO2_3;
            r = t - w;
            w = n * PIO2_3T - ((t - r) - w);
            y0 = r - w;
        }
    }
    let y1 = (r - y0) - w;
    (n as i64, y0, y1)
}

/// sin(x + y) para |x + y| ≤ π/4 (`__kernel_sin`).
fn k_sin(x: f64, y: f64) -> f64 {
    const S1: f64 = -1.666_666_666_666_663_243_48e-1;
    const S2: f64 = 8.333_333_333_322_489_461_24e-3;
    const S3: f64 = -1.984_126_982_985_794_931_34e-4;
    const S4: f64 = 2.755_731_370_707_006_767_89e-6;
    const S5: f64 = -2.505_076_025_340_686_341_95e-8;
    const S6: f64 = 1.589_690_995_211_550_102_21e-10;
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if y == 0.0 { x + v * (S1 + z * r) } else { x - ((z * (0.5 * y - v * r) - y) - v * S1) }
}

/// cos(x + y) para |x + y| ≤ π/4 (`__kernel_cos`).
fn k_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.166_666_666_666_660_190_37e-2;
    const C2: f64 = -1.388_888_888_887_410_957_49e-3;
    const C3: f64 = 2.480_158_728_947_672_941_78e-5;
    const C4: f64 = -2.755_731_435_139_066_330_35e-7;
    const C5: f64 = 2.087_572_321_298_174_827_90e-9;
    const C6: f64 = -1.135_964_755_778_819_482_65e-11;
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

/// `Math.sin`.
pub fn seno(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if abs(x) <= core::f64::consts::FRAC_PI_4 {
        return if abs(x) < 7.450_580_596_923_828e-9 { x } else { k_sin(x, 0.0) };
    }
    let (n, y0, y1) = reducir(x);
    match n & 3 {
        0 => k_sin(y0, y1),
        1 => k_cos(y0, y1),
        2 => -k_sin(y0, y1),
        _ => -k_cos(y0, y1),
    }
}

/// `Math.cos`.
pub fn coseno(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if abs(x) <= core::f64::consts::FRAC_PI_4 {
        return k_cos(x, 0.0);
    }
    let (n, y0, y1) = reducir(x);
    match n & 3 {
        0 => k_cos(y0, y1),
        1 => -k_sin(y0, y1),
        2 => -k_cos(y0, y1),
        _ => k_sin(y0, y1),
    }
}

/// La palabra alta de un `f64` (los 32 bits del signo, el exponente y el principio de la mantisa).
fn alta(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

/// `x` con la palabra baja a cero.
fn sin_baja(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0xffff_ffff_0000_0000)
}

/// tan(x + y) para |x + y| ≤ π/4; con `iy = -1`, −1/tan (`__kernel_tan`).
fn k_tan(x: f64, y: f64, iy: i32) -> f64 {
    const T: [f64; 13] = [
        3.333_333_333_333_340_919_86e-1,
        1.333_333_333_332_012_426_99e-1,
        5.396_825_397_622_605_213_77e-2,
        2.186_948_829_485_954_245_99e-2,
        8.863_239_823_599_300_057_37e-3,
        3.592_079_107_591_312_353_56e-3,
        1.456_209_454_325_290_255_16e-3,
        5.880_412_408_202_640_968_74e-4,
        2.464_631_348_184_699_068_12e-4,
        7.817_944_429_395_570_923_00e-5,
        7.140_724_913_826_081_903_05e-5,
        -1.855_863_748_552_754_566_54e-5,
        2.590_730_518_636_337_128_84e-5,
    ];
    const PIO4: f64 = 7.853_981_633_974_482_789_99e-1;
    const PIO4_LO: f64 = 3.061_616_997_868_383_017_93e-17;
    let hx = alta(x);
    let ix = hx & 0x7fff_ffff;
    let (mut x, mut y) = (x, y);
    if ix < 0x3e30_0000 && (x as i64) == 0 {
        // |x| < 2⁻²⁸.
        if (x.to_bits() & 0x7fff_ffff_ffff_ffff) == 0 && iy == -1 {
            return 1.0 / abs(x);
        }
        if iy == 1 {
            return x;
        }
        let w = x + y;
        let z = sin_baja(w);
        let v = y - (z - x);
        let a = -1.0 / w;
        let t = sin_baja(a);
        let s = 1.0 + t * z;
        return t + a * (s + t * v);
    }
    let grande = ix >= 0x3fe5_9428; // |x| ≥ 0,6744
    if grande {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4_LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    let r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    let mut r = y + z * (s * (r + v) + y);
    r += T[0] * s;
    let w = x + r;
    if grande {
        let v = f64::from(iy);
        let signo = f64::from(1 - ((hx >> 30) & 2));
        return signo * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        return w;
    }
    let z = sin_baja(w);
    let v = r - (z - x);
    let a = -1.0 / w;
    let t = sin_baja(a);
    let s = 1.0 + t * z;
    t + a * (s + t * v)
}

/// `Math.tan`.
pub fn tangente(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if abs(x) <= core::f64::consts::FRAC_PI_4 {
        return k_tan(x, 0.0, 1);
    }
    let (n, y0, y1) = reducir(x);
    k_tan(y0, y1, if n & 1 == 0 { 1 } else { -1 })
}

/// `Math.atan` (`s_atan.c` de fdlibm).
pub fn arcotangente(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.636_476_090_008_060_935_15e-1,
        7.853_981_633_974_482_789_99e-1,
        9.827_937_232_473_290_540_82e-1,
        1.570_796_326_794_896_558_00,
    ];
    const ATANLO: [f64; 4] = [
        2.269_877_745_296_168_709_24e-17,
        3.061_616_997_868_383_017_93e-17,
        1.390_331_103_123_099_845_16e-17,
        6.123_233_995_736_766_035_87e-17,
    ];
    const AT: [f64; 11] = [
        3.333_333_333_333_293_180_27e-1,
        -1.999_999_999_987_648_324_76e-1,
        1.428_571_427_250_346_637_11e-1,
        -1.111_111_040_546_235_578_80e-1,
        9.090_887_133_436_506_561_96e-2,
        -7.691_876_205_044_829_994_95e-2,
        6.661_073_137_387_531_206_69e-2,
        -5.833_570_133_790_573_486_45e-2,
        4.976_877_994_615_932_360_17e-2,
        -3.653_157_274_421_691_552_70e-2,
        1.628_582_011_536_578_236_23e-2,
    ];
    if x.is_nan() {
        return x;
    }
    let negativo = x.is_sign_negative();
    let ax = abs(x);
    if ax >= 7.378_697_629_483_821e19 {
        let r = ATANHI[3] + ATANLO[3];
        return if negativo { -r } else { r };
    }
    let (id, x) = if ax < 0.4375 {
        if ax < 3.725_290_298_461_914e-9 {
            return x;
        }
        (None, x)
    } else if ax < 1.1875 {
        if ax < 0.6875 { (Some(0), (2.0 * ax - 1.0) / (2.0 + ax)) } else { (Some(1), (ax - 1.0) / (ax + 1.0)) }
    } else if ax < 2.4375 {
        (Some(2), (ax - 1.5) / (1.0 + 1.5 * ax))
    } else {
        (Some(3), -1.0 / ax)
    };
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    match id {
        None => x - x * (s1 + s2),
        Some(i) => {
            let z = ATANHI[i] - ((x * (s1 + s2) - ATANLO[i]) - x);
            if negativo { -z } else { z }
        }
    }
}

/// `Math.atan2(y, x)` (`e_atan2.c` de fdlibm, con sus casos especiales).
pub fn arcotangente2(y: f64, x: f64) -> f64 {
    const PI: f64 = core::f64::consts::PI;
    const PI_LO: f64 = 1.224_646_799_147_353_177_2e-16;
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == 1.0 {
        return arcotangente(y);
    }
    let m = usize::from(y.is_sign_negative()) + 2 * usize::from(x.is_sign_negative());
    if y == 0.0 {
        return match m {
            0 | 1 => y,
            2 => PI,
            _ => -PI,
        };
    }
    if x == 0.0 {
        return if y < 0.0 { -PI / 2.0 } else { PI / 2.0 };
    }
    if x.is_infinite() {
        return if y.is_infinite() {
            [PI / 4.0, -PI / 4.0, 3.0 * PI / 4.0, -3.0 * PI / 4.0][m]
        } else {
            [0.0, -0.0, PI, -PI][m]
        };
    }
    if y.is_infinite() {
        return if y < 0.0 { -PI / 2.0 } else { PI / 2.0 };
    }
    let k = exponente(y) - exponente(x);
    let z = if k > 60 {
        PI / 2.0 + 0.5 * PI_LO
    } else if x < 0.0 && k < -60 {
        0.0
    } else {
        arcotangente(abs(y / x))
    };
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

const PIO2_HI: f64 = 1.570_796_326_794_896_558_00;
const PIO2_LO: f64 = 6.123_233_995_736_766_035_87e-17;

/// El cociente racional de `e_asin.c` y `e_acos.c` en `t`.
fn racional(t: f64) -> f64 {
    const PS: [f64; 6] = [
        1.666_666_666_666_666_574_15e-1,
        -3.255_658_186_224_009_154_05e-1,
        2.012_125_321_348_629_258_81e-1,
        -4.005_553_450_067_941_140_27e-2,
        7.915_349_942_898_145_321_76e-4,
        3.479_331_075_960_211_675_70e-5,
    ];
    const QS: [f64; 4] = [
        -2.403_394_911_734_414_218_78,
        2.020_945_760_233_505_694_71,
        -6.882_839_716_054_532_930_30e-1,
        7.703_815_055_590_193_527_91e-2,
    ];
    let p = t * (PS[0] + t * (PS[1] + t * (PS[2] + t * (PS[3] + t * (PS[4] + t * PS[5])))));
    let q = 1.0 + t * (QS[0] + t * (QS[1] + t * (QS[2] + t * QS[3])));
    p / q
}

/// `Math.asin` (`e_asin.c` de fdlibm).
pub fn arcoseno(x: f64) -> f64 {
    const PIO4_HI: f64 = 7.853_981_633_974_482_789_99e-1;
    if x.is_nan() || abs(x) > 1.0 {
        return f64::NAN;
    }
    if abs(x) == 1.0 {
        return x * PIO2_HI + x * PIO2_LO;
    }
    if abs(x) < 0.5 {
        if abs(x) < 1.490_116_119_384_765_6e-8 {
            return x;
        }
        return x + x * racional(x * x);
    }
    let t = (1.0 - abs(x)) * 0.5;
    let s = raiz(t);
    let r = if abs(x) >= 0.975 {
        PIO2_HI - (2.0 * (s + s * racional(t)) - PIO2_LO)
    } else {
        let w = sin_baja(s);
        let c = (t - w * w) / (s + w);
        let p = 2.0 * s * racional(t) - (PIO2_LO - 2.0 * c);
        let q = PIO4_HI - 2.0 * w;
        PIO4_HI - (p - q)
    };
    if x > 0.0 { r } else { -r }
}

/// `Math.acos` (`e_acos.c` de fdlibm).
pub fn arcocoseno(x: f64) -> f64 {
    const PI: f64 = core::f64::consts::PI;
    if x.is_nan() || abs(x) > 1.0 {
        return f64::NAN;
    }
    if abs(x) == 1.0 {
        return if x > 0.0 { 0.0 } else { PI + 2.0 * PIO2_LO };
    }
    if abs(x) < 0.5 {
        if abs(x) <= 6.938_893_903_907_228e-18 {
            return PIO2_HI + PIO2_LO;
        }
        return PIO2_HI - (x - (PIO2_LO - x * racional(x * x)));
    }
    if x < 0.0 {
        let z = (1.0 + x) * 0.5;
        let s = raiz(z);
        let w = racional(z) * s - PIO2_LO;
        return PI - 2.0 * (s + w);
    }
    let z = (1.0 - x) * 0.5;
    let s = raiz(z);
    let df = sin_baja(s);
    let c = (z - df * df) / (s + df);
    let w = racional(z) * s + c;
    2.0 * (df + w)
}

/// ln(1 + x) sin perder los dígitos de una `x` pequeña (el truco de Kahan).
fn ln1p(x: f64) -> f64 {
    let u = 1.0 + x;
    if u == 1.0 { x } else { crate::matematicas::ln(u) * x / (u - 1.0) }
}

/// `Math.asinh` (como `s_asinh.c` de fdlibm).
pub fn arcoseno_h(x: f64) -> f64 {
    if !x.is_finite() || abs(x) < 3.725_290_298_461_914e-9 {
        return x;
    }
    let t = abs(x);
    let w = if t > 268_435_456.0 {
        crate::matematicas::ln(t) + core::f64::consts::LN_2
    } else if t > 2.0 {
        crate::matematicas::ln(2.0 * t + 1.0 / (raiz(x * x + 1.0) + t))
    } else {
        let t2 = x * x;
        ln1p(t + t2 / (1.0 + raiz(1.0 + t2)))
    };
    if x > 0.0 { w } else { -w }
}

/// `Math.acosh` (como `e_acosh.c` de fdlibm).
pub fn arcocoseno_h(x: f64) -> f64 {
    if x.is_nan() || x < 1.0 {
        return f64::NAN;
    }
    if x >= 268_435_456.0 {
        return if x.is_infinite() { x } else { crate::matematicas::ln(x) + core::f64::consts::LN_2 };
    }
    if x == 1.0 {
        return 0.0;
    }
    if x > 2.0 {
        return crate::matematicas::ln(2.0 * x - 1.0 / (x + raiz(x * x - 1.0)));
    }
    let t = x - 1.0;
    ln1p(t + raiz(2.0 * t + t * t))
}

/// `Math.atanh` (como `e_atanh.c` de fdlibm).
pub fn arcotangente_h(x: f64) -> f64 {
    if x.is_nan() || abs(x) > 1.0 {
        return f64::NAN;
    }
    if abs(x) == 1.0 {
        return x / 0.0;
    }
    if abs(x) < 3.725_290_298_461_914e-9 {
        return x;
    }
    let t = abs(x);
    let r = if t < 0.5 {
        let t2 = t + t;
        0.5 * ln1p(t2 + t2 * t / (1.0 - t))
    } else {
        0.5 * ln1p((t + t) / (1.0 - t))
    };
    if x > 0.0 { r } else { -r }
}

#[cfg(test)]
mod pruebas {
    extern crate std;
    use super::*;

    /// Iguales a `f64::sin` y compañía (la libm del sistema) salvo, como mucho, unos ulp.
    fn cerca(a: f64, b: f64, ulps: u64) -> bool {
        a == b || (a.is_nan() && b.is_nan()) || a.to_bits().abs_diff(b.to_bits()) <= ulps
    }

    #[test]
    fn como_la_libm_del_sistema() {
        let mut x = -50.0;
        while x < 50.0 {
            assert!(cerca(seno(x), x.sin(), 1), "sin {x}: {} {}", seno(x), x.sin());
            assert!(cerca(coseno(x), x.cos(), 1), "cos {x}: {} {}", coseno(x), x.cos());
            assert!(cerca(tangente(x), x.tan(), 1), "tan {x}: {} {}", tangente(x), x.tan());
            assert!(cerca(arcotangente(x), x.atan(), 1), "atan {x}");
            assert!(cerca(arcotangente2(x, 0.7 - x), x.atan2(0.7 - x), 1), "atan2 {x}");
            x += 0.0137;
        }
        let mut x = -1.0;
        while x <= 1.0 {
            assert!(cerca(arcoseno(x), x.asin(), 1), "asin {x}: {} {}", arcoseno(x), x.asin());
            assert!(cerca(arcocoseno(x), x.acos(), 1), "acos {x}: {} {}", arcocoseno(x), x.acos());
            // Con |x|: la `atanh` de Rust pierde dígitos con negativos cerca de -1 (resta números
            // casi iguales); la nuestra, como fdlibm, usa atanh(-x) = -atanh(x).
            let a = (x * 0.999).abs();
            assert!(
                cerca(arcotangente_h(a), a.atanh(), 4) && arcotangente_h(-a) == -arcotangente_h(a),
                "atanh {x}: {}",
                arcotangente_h(a).to_bits().abs_diff(a.atanh().to_bits())
            );
            assert!(cerca(arcoseno_h(x * 7.0), (x * 7.0).asinh(), 4), "asinh {x}");
            assert!(cerca(arcocoseno_h(1.0 + x * x * 9.0), (1.0 + x * x * 9.0).acosh(), 4), "acosh {x}");
            x += 0.001;
        }
        assert!(cerca(seno(1e6), 1e6f64.sin(), 2) && cerca(coseno(12345.678), 12345.678f64.cos(), 2));
        assert!(seno(f64::INFINITY).is_nan() && arcoseno(1.5).is_nan());
        assert_eq!(
            (arcotangente2(0.0, -1.0), arcotangente2(-0.0, 1.0).to_bits()),
            (core::f64::consts::PI, (-0.0f64).to_bits())
        );
    }
}
