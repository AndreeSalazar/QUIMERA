//! `calc()`, `min()`, `max()` y `clamp()` en las longitudes.
//!
//! Cada término es una combinación lineal `px + pct %` (las unidades relativas a la letra o a la
//! ventana ya pasan a píxeles aquí; el porcentaje queda para la maquetación), o un número sin
//! unidad para multiplicar y dividir.

use crate::propiedades::Contexto;
use crate::valores::Medida;
use alloc::vec::Vec;
use quimera_css::{Longitud, Token, longitud};

#[derive(Clone, Copy)]
struct Lineal {
    px: f32,
    pct: f32,
    /// Un número sin unidad (solo vale para `*` y `/`).
    numero: bool,
}

impl Lineal {
    fn por(self, k: f32) -> Lineal {
        Lineal { px: self.px * k, pct: self.pct * k, numero: self.numero }
    }
}

/// La medida de un valor `calc(...)` (o `min()`, `max()`, `clamp()`), si lo es y es válido.
pub(crate) fn medida_calc(v: &[Token], c: &Contexto) -> Option<Medida> {
    let t: Vec<&Token> = v.iter().filter(|t| **t != Token::Blanco).collect();
    if !matches!(t.first(), Some(Token::Funcion(_))) {
        return None;
    }
    let mut l = Lector { t, i: 0, c };
    let r = l.factor()?;
    if l.i != l.t.len() || r.numero {
        return None;
    }
    Some(if r.pct == 0.0 {
        Medida::Px(r.px)
    } else if r.px == 0.0 {
        Medida::Porcentaje(r.pct)
    } else {
        Medida::Mixta(r.px, r.pct)
    })
}

struct Lector<'a> {
    t: Vec<&'a Token>,
    i: usize,
    c: &'a Contexto,
}

impl Lector<'_> {
    fn ver(&self) -> Option<&Token> {
        self.t.get(self.i).copied()
    }

    fn suma(&mut self) -> Option<Lineal> {
        let mut a = self.producto()?;
        loop {
            let signo = match self.ver() {
                Some(Token::Delim('+')) => 1.0,
                Some(Token::Delim('-')) => -1.0,
                // `100% -20px` sin espacio tras el signo: el número ya lleva el suyo.
                Some(Token::Dimension(n, _) | Token::Numero(n) | Token::Porcentaje(n)) if *n < 0.0 => {
                    let b = self.producto()?;
                    a = sumar(a, b)?;
                    continue;
                }
                _ => return Some(a),
            };
            self.i += 1;
            let b = self.producto()?.por(signo);
            a = sumar(a, b)?;
        }
    }

    fn producto(&mut self) -> Option<Lineal> {
        let mut a = self.factor()?;
        loop {
            match self.ver() {
                Some(Token::Delim('*')) => {
                    self.i += 1;
                    let b = self.factor()?;
                    a = match (a.numero, b.numero) {
                        (_, true) => a.por(b.px),
                        (true, false) => b.por(a.px),
                        _ => return None,
                    };
                }
                Some(Token::Delim('/')) => {
                    self.i += 1;
                    let b = self.factor()?;
                    if !b.numero || b.px == 0.0 {
                        return None;
                    }
                    a = a.por(1.0 / b.px);
                }
                _ => return Some(a),
            }
        }
    }

    fn factor(&mut self) -> Option<Lineal> {
        let t = self.ver()?.clone();
        self.i += 1;
        match t {
            Token::Numero(n) => Some(Lineal { px: n as f32, pct: 0.0, numero: true }),
            Token::Abre('(') => self.cerrar_tras_suma(),
            Token::Funcion(f) => match f.to_ascii_lowercase().as_str() {
                "calc" => self.cerrar_tras_suma(),
                "min" | "max" => {
                    let args = self.argumentos()?;
                    let menor = f.eq_ignore_ascii_case("min");
                    extremo(&args, menor)
                }
                "clamp" => {
                    let args = self.argumentos()?;
                    let [bajo, medio, alto] = args[..] else { return None };
                    let tope = extremo(&[medio, alto], true)?;
                    extremo(&[bajo, tope], false)
                }
                _ => None,
            },
            otro => {
                let (px, pct) = match longitud(&otro)? {
                    Longitud::Px(x) => (x, 0.0),
                    Longitud::Em(x) => (x * self.c.letra, 0.0),
                    Longitud::Rem(x) => (x * self.c.letra_raiz, 0.0),
                    Longitud::Vw(x) => (x * self.c.ventana.0 / 100.0, 0.0),
                    Longitud::Vh(x) => (x * self.c.ventana.1 / 100.0, 0.0),
                    Longitud::Porcentaje(p) => (0.0, p),
                    Longitud::Auto => return None,
                };
                Some(Lineal { px, pct, numero: false })
            }
        }
    }

    fn cerrar_tras_suma(&mut self) -> Option<Lineal> {
        let r = self.suma()?;
        (self.ver()? == &Token::Cierra(')')).then_some(())?;
        self.i += 1;
        Some(r)
    }

    /// `a, b, c)`: los argumentos de `min()`, `max()` o `clamp()`.
    fn argumentos(&mut self) -> Option<Vec<Lineal>> {
        let mut v = Vec::new();
        loop {
            v.push(self.suma()?);
            match self.ver()? {
                Token::Coma => self.i += 1,
                Token::Cierra(')') => {
                    self.i += 1;
                    return Some(v);
                }
                _ => return None,
            }
        }
    }
}

fn sumar(a: Lineal, b: Lineal) -> Option<Lineal> {
    // Un número y una longitud no se suman (salvo el 0, que vale por las dos cosas).
    if a.numero != b.numero && !(a.numero && a.px == 0.0) && !(b.numero && b.px == 0.0) {
        return None;
    }
    Some(Lineal { px: a.px + b.px, pct: a.pct + b.pct, numero: a.numero && b.numero })
}

/// El menor (o mayor) de los términos, si se pueden comparar sin saber el contenedor: todos en
/// píxeles o todos en porcentaje.
fn extremo(v: &[Lineal], menor: bool) -> Option<Lineal> {
    let elegir = |a: f32, b: f32| if menor { a.min(b) } else { a.max(b) };
    let primero = *v.first()?;
    if v.iter().all(|x| x.pct == 0.0) {
        let px = v.iter().map(|x| x.px).reduce(elegir)?;
        return Some(Lineal { px, ..primero });
    }
    if v.iter().all(|x| x.px == 0.0 && !x.numero) {
        let pct = v.iter().map(|x| x.pct).reduce(elegir)?;
        return Some(Lineal { pct, ..primero });
    }
    None
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use quimera_css::tokens;

    fn m(s: &str) -> Option<Medida> {
        let c = Contexto { letra: 10.0, letra_padre: 10.0, letra_raiz: 16.0, ventana: (1000.0, 800.0) };
        medida_calc(&tokens(s), &c)
    }

    #[test]
    fn calc_con_unidades_y_operaciones() {
        assert_eq!(m("calc(10px + 2em)"), Some(Medida::Px(30.0)));
        assert_eq!(m("calc(100vh - 56px*2)"), Some(Medida::Px(688.0)));
        assert_eq!(m("calc((1rem + 4px) / 2)"), Some(Medida::Px(10.0)));
        assert_eq!(m("calc(50%)"), Some(Medida::Porcentaje(50.0)));
        assert_eq!(m("calc(100% - 20px)"), Some(Medida::Mixta(-20.0, 100.0)));
        assert_eq!(m("calc(2 * calc(3px + 1px))"), Some(Medida::Px(8.0)));
        assert_eq!(m("min(100px, 20vw)"), Some(Medida::Px(100.0)));
        assert_eq!(m("clamp(10px, 50vw, 300px)"), Some(Medida::Px(300.0)));
        assert_eq!(m("calc(10px + 2)"), None, "longitud y número no se suman");
        assert_eq!(m("calc(3)"), None, "un número solo no es una longitud");
        assert_eq!(m("10px"), None);
    }
}
