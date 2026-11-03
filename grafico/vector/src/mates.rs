//! Lo poco de matemáticas que hace falta sin `std` (`no_std` no trae `sqrt`, `sin`, `floor`...).
//! La precisión basta para dibujar: los errores quedan muy por debajo de un píxel.

use core::f32::consts::PI;

/// Mayor entero que no pasa de `x`.
pub fn suelo(x: f32) -> f32 {
    let t = x as i64 as f32;
    if t > x { t - 1.0 } else { t }
}

/// Menor entero que no baja de `x`.
pub fn techo(x: f32) -> f32 {
    let t = x as i64 as f32;
    if t < x { t + 1.0 } else { t }
}

/// Raíz cuadrada (Newton desde una estimación con los bits del número).
pub fn raiz(x: f32) -> f32 {
    if x <= 0.0 || !x.is_finite() {
        return if x == f32::INFINITY { x } else { 0.0 };
    }
    let mut r = f32::from_bits((x.to_bits() >> 1) + 0x1fbd_1df5);
    for _ in 0..4 {
        r = 0.5 * (r + x / r);
    }
    r
}

/// Seno (reduce a [-π, π] y usa la serie de Taylor hasta x¹¹).
pub fn seno(x: f32) -> f32 {
    let mut x = x % (2.0 * PI);
    if x > PI {
        x -= 2.0 * PI;
    } else if x < -PI {
        x += 2.0 * PI;
    }
    // sin(x) = sin(π - x): queda en [-π/2, π/2], donde la serie converge rápido.
    if x > PI / 2.0 {
        x = PI - x;
    } else if x < -PI / 2.0 {
        x = -PI - x;
    }
    let x2 = x * x;
    x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0 * (1.0 - x2 / 110.0)))))
}

/// Coseno.
pub fn coseno(x: f32) -> f32 {
    seno(x + PI / 2.0)
}

/// Arcotangente de y/x en el cuadrante correcto.
pub fn atan2(y: f32, x: f32) -> f32 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    let (ax, ay) = (x.abs(), y.abs());
    // atan en [0, 1] con un polinomio minimax (error < 2e-6 rad).
    let (a, invertido) = if ax >= ay { (ay / ax, false) } else { (ax / ay, true) };
    let s = a * a;
    let p = ((((-0.011_721_2 * s + 0.052_653_32) * s - 0.116_432_87) * s + 0.193_543_46) * s - 0.332_623_47) * s;
    let mut r = a * (0.999_977_26 + p);
    if invertido {
        r = PI / 2.0 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 { -r } else { r }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn como_la_biblioteca_estandar() {
        for i in -50..50 {
            let x = i as f32 * 0.37;
            assert!((seno(x) - x.sin()).abs() < 1e-4, "seno({x})");
            assert!((coseno(x) - x.cos()).abs() < 1e-4, "coseno({x})");
            assert!((raiz(x.abs()) - x.abs().sqrt()).abs() < 1e-4 * x.abs().max(1.0), "raiz({x})");
            assert_eq!(suelo(x), x.floor());
            assert_eq!(techo(x), x.ceil());
            for j in -3..3 {
                let y = j as f32 * 0.9;
                assert!((atan2(y, x) - y.atan2(x)).abs() < 1e-4, "atan2({y}, {x})");
            }
        }
    }
}
