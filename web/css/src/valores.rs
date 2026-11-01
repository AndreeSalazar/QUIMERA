//! Valores: colores (`red`, `#0af`, `rgb()`, `hsl()`) y longitudes (`12px`, `1.5em`, `50%`).
//!
//! Las unidades absolutas se pasan a píxeles aquí (`1pt` = 4/3 px); las relativas (`em`, `%`,
//! `vw`) se quedan como están hasta que el estilo sepa a qué se refieren.

use crate::colores::NOMBRES;
use crate::tokenizador::Token;
use alloc::vec::Vec;

/// Un color sRGB con opacidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    /// Rojo.
    pub r: u8,
    /// Verde.
    pub g: u8,
    /// Azul.
    pub b: u8,
    /// Opacidad (255 = opaco).
    pub a: u8,
}

impl Color {
    /// Un color opaco.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    /// Negro.
    pub const NEGRO: Color = Color::rgb(0, 0, 0);
    /// Blanco.
    pub const BLANCO: Color = Color::rgb(255, 255, 255);
    /// Transparente.
    pub const TRANSPARENTE: Color = Color { r: 0, g: 0, b: 0, a: 0 };
}

/// Una longitud.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Longitud {
    /// Píxeles CSS (las demás unidades absolutas ya convertidas).
    Px(f32),
    /// Relativa al tamaño de letra del elemento.
    Em(f32),
    /// Relativa al tamaño de letra de la raíz.
    Rem(f32),
    /// Porcentaje de algo que decide la propiedad.
    Porcentaje(f32),
    /// Porcentaje del ancho de la ventana.
    Vw(f32),
    /// Porcentaje del alto de la ventana.
    Vh(f32),
    /// `auto`.
    Auto,
}

/// Los tokens de un valor sin blancos.
fn sin_blancos(t: &[Token]) -> Vec<&Token> {
    t.iter().filter(|x| **x != Token::Blanco).collect()
}

/// Un color, si el valor entero lo es.
pub fn color(valor: &[Token]) -> Option<Color> {
    let t = sin_blancos(valor);
    match t.as_slice() {
        [Token::Ident(n)] => nombrado(n),
        [Token::Almohadilla(h)] => hexadecimal(h),
        // `light-dark(claro, oscuro)`: QUIMERA pinta en modo claro, así que vale el primero.
        [Token::Funcion(f), ..] if f == "light-dark" => {
            let dentro = &valor[valor.iter().position(|x| matches!(x, Token::Funcion(_)))? + 1..];
            let mut nivel = 0i32;
            let fin = dentro.iter().position(|x| {
                match x {
                    Token::Funcion(_) | Token::Abre(_) => nivel += 1,
                    Token::Cierra(_) => nivel -= 1,
                    _ => {}
                }
                *x == Token::Coma && nivel == 0
            })?;
            color(&dentro[..fin])
        }
        [Token::Funcion(f), resto @ ..] if resto.last() == Some(&&Token::Cierra(')')) => {
            let args: Vec<&Token> = resto[..resto.len() - 1]
                .iter()
                .copied()
                .filter(|x| !matches!(x, Token::Coma | Token::Delim('/')))
                .collect();
            match f.as_str() {
                "rgb" | "rgba" => funcion_rgb(&args),
                "hsl" | "hsla" => funcion_hsl(&args),
                _ => None,
            }
        }
        _ => None,
    }
}

fn nombrado(n: &str) -> Option<Color> {
    let n = n.to_ascii_lowercase();
    if n == "transparent" {
        return Some(Color::TRANSPARENTE);
    }
    let i = NOMBRES.binary_search_by(|(x, _)| (*x).cmp(n.as_str())).ok()?;
    let v = NOMBRES[i].1;
    Some(Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`.
fn hexadecimal(h: &str) -> Option<Color> {
    let cifras: Vec<u8> = h.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
    let (r, g, b, a) = match cifras.as_slice() {
        [r, g, b] => (r * 17, g * 17, b * 17, 255),
        [r, g, b, a] => (r * 17, g * 17, b * 17, a * 17),
        [r1, r2, g1, g2, b1, b2] => (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255),
        [r1, r2, g1, g2, b1, b2, a1, a2] => (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2),
        _ => return None,
    };
    Some(Color { r, g, b, a })
}

fn a_byte(x: f64) -> u8 {
    if x.is_nan() {
        return 0;
    }
    (x.clamp(0.0, 255.0) + 0.5) as u8
}

/// La opacidad: `0.5` o `50%`.
fn alfa(t: Option<&&Token>) -> Option<u8> {
    Some(match t {
        None => 255,
        Some(Token::Numero(n)) => a_byte(n * 255.0),
        Some(Token::Porcentaje(p)) => a_byte(p * 255.0 / 100.0),
        _ => return None,
    })
}

/// `rgb(255, 0, 0)`, `rgb(100% 0% 0% / 50%)`.
fn funcion_rgb(a: &[&Token]) -> Option<Color> {
    if !(3..=4).contains(&a.len()) {
        return None;
    }
    let canal = |t: &Token| match t {
        Token::Numero(n) => Some(a_byte(*n)),
        Token::Porcentaje(p) => Some(a_byte(p * 255.0 / 100.0)),
        _ => None,
    };
    Some(Color { r: canal(a[0])?, g: canal(a[1])?, b: canal(a[2])?, a: alfa(a.get(3))? })
}

/// `hsl(120, 100%, 50%)`, `hsl(120deg 100% 50% / .5)`.
fn funcion_hsl(a: &[&Token]) -> Option<Color> {
    if !(3..=4).contains(&a.len()) {
        return None;
    }
    let tono = match a[0] {
        Token::Numero(n) => *n,
        Token::Dimension(n, u) if u == "deg" => *n,
        Token::Dimension(n, u) if u == "turn" => n * 360.0,
        _ => return None,
    };
    let (Token::Porcentaje(s), Token::Porcentaje(l)) = (a[1], a[2]) else { return None };
    let (h, s, l) = (((tono % 360.0) + 360.0) % 360.0 / 360.0, s / 100.0, l / 100.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let canal = |mut t: f64| {
        t = if t < 0.0 {
            t + 1.0
        } else if t > 1.0 {
            t - 1.0
        } else {
            t
        };
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        a_byte(v * 255.0)
    };
    Some(Color { r: canal(h + 1.0 / 3.0), g: canal(h), b: canal(h - 1.0 / 3.0), a: alfa(a.get(3))? })
}

/// Una longitud a partir de un token (`0` sin unidad vale; otros números sin unidad no).
pub fn longitud(t: &Token) -> Option<Longitud> {
    Some(match t {
        Token::Numero(n) if *n == 0.0 => Longitud::Px(0.0),
        Token::Porcentaje(p) => Longitud::Porcentaje(*p as f32),
        Token::Ident(a) if a.eq_ignore_ascii_case("auto") => Longitud::Auto,
        Token::Dimension(n, u) => {
            let n = *n as f32;
            match u.as_str() {
                "px" => Longitud::Px(n),
                "em" => Longitud::Em(n),
                "rem" => Longitud::Rem(n),
                "ex" | "ch" => Longitud::Em(n * 0.5),
                "vw" => Longitud::Vw(n),
                "vh" => Longitud::Vh(n),
                "pt" => Longitud::Px(n * 4.0 / 3.0),
                "pc" => Longitud::Px(n * 16.0),
                "in" => Longitud::Px(n * 96.0),
                "cm" => Longitud::Px(n * 96.0 / 2.54),
                "mm" => Longitud::Px(n * 96.0 / 25.4),
                "q" => Longitud::Px(n * 96.0 / 101.6),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// Las piezas de un valor separadas por blancos de nivel superior (`1px solid red` → 3 piezas;
/// `rgb(1, 2, 3)` es una sola).
pub fn piezas(valor: &[Token]) -> Vec<&[Token]> {
    let mut v = Vec::new();
    let (mut inicio, mut nivel) = (0, 0i32);
    for (i, t) in valor.iter().enumerate() {
        match t {
            Token::Funcion(_) | Token::Abre(_) => nivel += 1,
            Token::Cierra(_) => nivel -= 1,
            Token::Blanco if nivel == 0 => {
                if i > inicio {
                    v.push(&valor[inicio..i]);
                }
                inicio = i + 1;
            }
            _ => {}
        }
    }
    if valor.len() > inicio {
        v.push(&valor[inicio..]);
    }
    v
}
