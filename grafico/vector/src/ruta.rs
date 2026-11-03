//! Rutas: el atributo `d` (SVG 1.1, §8.3), las formas básicas y `transform`, aplanadas en
//! polígonos (las curvas se cortan en segmentos de menos de un cuarto de píxel de error).

use crate::mates::{atan2, coseno, raiz, seno, techo};
use crate::xml::Elemento;
use alloc::vec::Vec;
use core::f32::consts::PI;

/// Un punto.
pub type Punto = (f32, f32);

/// Una transformación afín `[a b c d e f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matriz(pub [f32; 6]);

impl Matriz {
    pub const IDENTIDAD: Matriz = Matriz([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self` seguida de... no: primero `otra` y luego `self` (como `transform` dentro de un grupo).
    pub fn por(&self, otra: &Matriz) -> Matriz {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = otra.0;
        Matriz([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }

    pub fn aplicar(&self, (x, y): Punto) -> Punto {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// Cuánto agranda (para decidir en cuántos trozos cortar las curvas).
    pub fn escala(&self) -> f32 {
        let [a, b, c, d, ..] = self.0;
        raiz((a * a + b * b).max(c * c + d * d))
    }
}

/// Los números de una lista SVG (`1.5.5` son 1.5 y .5; `1-2`, 1 y -2; las comas cuentan como blanco).
pub fn numeros(s: &str) -> Vec<f32> {
    let mut v = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match leer_numero(b, i) {
            Some((n, fin)) => {
                v.push(n);
                i = fin;
            }
            None => i += 1,
        }
    }
    v
}

/// Un número desde `i` (saltando blancos y comas): su valor y dónde acaba.
fn leer_numero(b: &[u8], mut i: usize) -> Option<(f32, usize)> {
    while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
        i += 1;
    }
    let inicio = i;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let mut punto = false;
    let mut digitos = false;
    while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !punto)) {
        punto |= b[i] == b'.';
        digitos |= b[i].is_ascii_digit();
        i += 1;
    }
    if !digitos {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'-' || b[j] == b'+') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            i = j;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    core::str::from_utf8(&b[inicio..i]).ok()?.parse().ok().map(|n| (n, i))
}

/// Una bandera de arco (`0` o `1`) desde `i`, saltando blancos y comas.
fn leer_bandera(b: &[u8], mut i: usize) -> Option<(f32, usize)> {
    while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
        i += 1;
    }
    match b.get(i)? {
        b'0' => Some((0.0, i + 1)),
        b'1' => Some((1.0, i + 1)),
        _ => None,
    }
}

/// `transform="translate(…) rotate(…) …"` (se aplican de izquierda a derecha).
pub fn transformacion(s: &str) -> Matriz {
    let mut m = Matriz::IDENTIDAD;
    for parte in s.split(')') {
        let Some((nombre, args)) = parte.split_once('(') else { continue };
        let n = numeros(args);
        let a = |i: usize| n.get(i).copied().unwrap_or(0.0);
        let t = match nombre.trim().trim_start_matches(',').trim() {
            "matrix" if n.len() >= 6 => Matriz([a(0), a(1), a(2), a(3), a(4), a(5)]),
            "translate" => Matriz([1.0, 0.0, 0.0, 1.0, a(0), a(1)]),
            "scale" => Matriz([a(0), 0.0, 0.0, if n.len() > 1 { a(1) } else { a(0) }, 0.0, 0.0]),
            "rotate" => {
                let (s, c) = (seno(a(0) * PI / 180.0), coseno(a(0) * PI / 180.0));
                let r = Matriz([c, s, -s, c, 0.0, 0.0]);
                let (cx, cy) = (a(1), a(2));
                Matriz([1.0, 0.0, 0.0, 1.0, cx, cy]).por(&r).por(&Matriz([1.0, 0.0, 0.0, 1.0, -cx, -cy]))
            }
            "skewX" => Matriz([1.0, 0.0, tan(a(0)), 1.0, 0.0, 0.0]),
            "skewY" => Matriz([1.0, tan(a(0)), 0.0, 1.0, 0.0, 0.0]),
            _ => continue,
        };
        m = m.por(&t);
    }
    m
}

fn cuadrado(x: f32) -> f32 {
    x * x
}

fn tan(grados: f32) -> f32 {
    let r = grados * PI / 180.0;
    seno(r) / coseno(r)
}

/// Quien va juntando los polígonos de una ruta ya transformados.
pub struct Trazador {
    pub m: Matriz,
    /// Error máximo al aplanar, en unidades de la ruta.
    tolerancia: f32,
    pub poligonos: Vec<Vec<Punto>>,
    actual: Vec<Punto>,
}

impl Trazador {
    pub fn nuevo(m: Matriz) -> Trazador {
        let tolerancia = 0.25 / m.escala().max(1e-6);
        Trazador { m, tolerancia, poligonos: Vec::new(), actual: Vec::new() }
    }

    pub fn mover(&mut self, p: Punto) {
        self.cerrar();
        self.actual.push(self.m.aplicar(p));
    }

    pub fn linea(&mut self, p: Punto) {
        self.actual.push(self.m.aplicar(p));
    }

    pub fn cerrar(&mut self) {
        if self.actual.len() > 2 {
            self.poligonos.push(core::mem::take(&mut self.actual));
        }
        self.actual.clear();
    }

    /// Cuántos trozos para una curva de puntos de control `pts` (por su largo aproximado).
    fn trozos(&self, pts: &[Punto]) -> usize {
        let largo: f32 = pts.windows(2).map(|w| raiz(cuadrado(w[1].0 - w[0].0) + cuadrado(w[1].1 - w[0].1))).sum();
        (techo(raiz(largo / self.tolerancia) * 0.5) as usize).clamp(1, 200)
    }

    /// Bézier cúbica desde `p0`.
    pub fn cubica(&mut self, p0: Punto, c1: Punto, c2: Punto, p: Punto) {
        let n = self.trozos(&[p0, c1, c2, p]);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.linea((a * p0.0 + b * c1.0 + c * c2.0 + d * p.0, a * p0.1 + b * c1.1 + c * c2.1 + d * p.1));
        }
    }

    /// Bézier cuadrática desde `p0`.
    pub fn cuadratica(&mut self, p0: Punto, c: Punto, p: Punto) {
        let n = self.trozos(&[p0, c, p]);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, d) = (u * u, 2.0 * u * t, t * t);
            self.linea((a * p0.0 + b * c.0 + d * p.0, a * p0.1 + b * c.1 + d * p.1));
        }
    }

    /// Un arco elíptico de `p0` a `p` (SVG 1.1, apéndice F.6.5: de extremos a centro).
    pub fn arco(&mut self, p0: Punto, (rx, ry): (f32, f32), giro: f32, (grande, horario): (bool, bool), p: Punto) {
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 || p0 == p {
            return self.linea(p);
        }
        let (sp, cp) = (seno(giro * PI / 180.0), coseno(giro * PI / 180.0));
        let (dx, dy) = ((p0.0 - p.0) / 2.0, (p0.1 - p.1) / 2.0);
        let (x1, y1) = (cp * dx + sp * dy, -sp * dx + cp * dy);
        let l = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if l > 1.0 {
            (rx, ry) = (rx * raiz(l), ry * raiz(l));
        }
        let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let mut k = if den > 0.0 { raiz(num / den) } else { 0.0 };
        if grande == horario {
            k = -k;
        }
        let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
        let (cx, cy) = (cp * cx1 - sp * cy1 + (p0.0 + p.0) / 2.0, sp * cx1 + cp * cy1 + (p0.1 + p.1) / 2.0);
        let t1 = atan2((y1 - cy1) / ry, (x1 - cx1) / rx);
        let mut dt = atan2((-y1 - cy1) / ry, (-x1 - cx1) / rx) - t1;
        if horario && dt < 0.0 {
            dt += 2.0 * PI;
        } else if !horario && dt > 0.0 {
            dt -= 2.0 * PI;
        }
        let n = self.trozos(&[p0, p]).max((dt.abs() / (PI / 16.0)) as usize).clamp(1, 200);
        for i in 1..=n {
            let t = t1 + dt * i as f32 / n as f32;
            let (ex, ey) = (rx * coseno(t), ry * seno(t));
            self.linea((cp * ex - sp * ey + cx, sp * ex + cp * ey + cy));
        }
    }
}

/// Por dónde va una ruta: el punto actual, el inicio del subcamino y el último control.
struct Pluma {
    actual: Punto,
    inicio: Punto,
    control: Option<Punto>,
}

/// Recorre `d` y lo traza.
pub fn trazar_d(d: &str, t: &mut Trazador) {
    let b = d.as_bytes();
    let (mut i, mut orden) = (0, b'M');
    let mut pluma = Pluma { actual: (0.0, 0.0), inicio: (0.0, 0.0), control: None };
    loop {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        if b[i].is_ascii_alphabetic() {
            orden = b[i];
            i += 1;
            if orden == b'Z' || orden == b'z' {
                t.cerrar();
                pluma.actual = pluma.inicio;
                t.mover(pluma.actual);
                pluma.control = None;
                continue;
            }
        }
        let n = match orden.to_ascii_uppercase() {
            b'H' | b'V' => 1,
            b'M' | b'L' | b'T' => 2,
            b'S' | b'Q' => 4,
            b'C' => 6,
            b'A' => 7,
            _ => break,
        };
        let mut v = [0.0f32; 7];
        let arco = orden.eq_ignore_ascii_case(&b'A');
        for (k, x) in v.iter_mut().enumerate().take(n) {
            // Las banderas de un arco son un solo carácter, y pueden ir pegadas: `a2 2 0 100 4`.
            let leido = if arco && (k == 3 || k == 4) { leer_bandera(b, i) } else { leer_numero(b, i) };
            let Some((num, fin)) = leido else {
                t.cerrar();
                return;
            };
            *x = num;
            i = fin;
        }
        orden = una_orden(orden, &v, &mut pluma, t);
    }
    t.cerrar();
}

/// Ejecuta una orden con sus números; devuelve la orden que sigue (tras `M` vienen `L`).
fn una_orden(orden: u8, v: &[f32; 7], pluma: &mut Pluma, t: &mut Trazador) -> u8 {
    let rel = orden.is_ascii_lowercase();
    let actual = pluma.actual;
    let base = if rel { actual } else { (0.0, 0.0) };
    let p = |k: usize| (base.0 + v[k], base.1 + v[k + 1]);
    let reflejo = |c: Option<Punto>| c.map_or(actual, |c| (2.0 * actual.0 - c.0, 2.0 * actual.1 - c.1));
    let mut siguiente = orden;
    let (nuevo, ctrl) = match orden.to_ascii_uppercase() {
        b'M' => {
            t.mover(p(0));
            pluma.inicio = p(0);
            siguiente = if rel { b'l' } else { b'L' };
            (p(0), None)
        }
        b'L' => (p(0), None),
        b'H' => ((if rel { actual.0 + v[0] } else { v[0] }, actual.1), None),
        b'V' => ((actual.0, if rel { actual.1 + v[0] } else { v[0] }), None),
        b'C' => {
            t.cubica(actual, p(0), p(2), p(4));
            (p(4), Some(p(2)))
        }
        b'S' => {
            t.cubica(actual, reflejo(pluma.control), p(0), p(2));
            (p(2), Some(p(0)))
        }
        b'Q' => {
            t.cuadratica(actual, p(0), p(2));
            (p(2), Some(p(0)))
        }
        b'T' => {
            let c = reflejo(pluma.control);
            t.cuadratica(actual, c, p(0));
            (p(0), Some(c))
        }
        _ => {
            t.arco(actual, (v[0], v[1]), v[2], (v[3] != 0.0, v[4] != 0.0), p(5));
            (p(5), None)
        }
    };
    if matches!(orden.to_ascii_uppercase(), b'L' | b'H' | b'V') {
        t.linea(nuevo);
    }
    pluma.actual = nuevo;
    pluma.control = ctrl;
    siguiente
}

/// Las formas básicas como rutas: `rect`, `circle`, `ellipse`, `polygon`, `polyline`, `line`.
pub fn trazar_forma(el: &Elemento, t: &mut Trazador) {
    let n = |k: &str| el.atributo(k).and_then(|v| numeros(v).first().copied()).unwrap_or(0.0);
    match el.nombre.as_str() {
        "rect" => {
            let (x, y, w, h) = (n("x"), n("y"), n("width"), n("height"));
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let (mut rx, mut ry) = (n("rx"), n("ry"));
            if el.atributo("ry").is_none() {
                ry = rx;
            }
            if el.atributo("rx").is_none() {
                rx = ry;
            }
            let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
            if rx <= 0.0 || ry <= 0.0 {
                t.mover((x, y));
                [(x + w, y), (x + w, y + h), (x, y + h)].into_iter().for_each(|p| t.linea(p));
            } else {
                t.mover((x + rx, y));
                t.linea((x + w - rx, y));
                t.arco((x + w - rx, y), (rx, ry), 0.0, (false, true), (x + w, y + ry));
                t.linea((x + w, y + h - ry));
                t.arco((x + w, y + h - ry), (rx, ry), 0.0, (false, true), (x + w - rx, y + h));
                t.linea((x + rx, y + h));
                t.arco((x + rx, y + h), (rx, ry), 0.0, (false, true), (x, y + h - ry));
                t.linea((x, y + ry));
                t.arco((x, y + ry), (rx, ry), 0.0, (false, true), (x + rx, y));
            }
            t.cerrar();
        }
        "circle" | "ellipse" => {
            let (cx, cy) = (n("cx"), n("cy"));
            let (rx, ry) = if el.nombre == "circle" { (n("r"), n("r")) } else { (n("rx"), n("ry")) };
            if rx <= 0.0 || ry <= 0.0 {
                return;
            }
            t.mover((cx + rx, cy));
            t.arco((cx + rx, cy), (rx, ry), 0.0, (false, true), (cx - rx, cy));
            t.arco((cx - rx, cy), (rx, ry), 0.0, (false, true), (cx + rx, cy));
            t.cerrar();
        }
        "polygon" | "polyline" => {
            let v = numeros(el.atributo("points").unwrap_or(""));
            for (k, par) in v.chunks_exact(2).enumerate() {
                if k == 0 { t.mover((par[0], par[1])) } else { t.linea((par[0], par[1])) }
            }
            t.cerrar();
        }
        _ => {}
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn numeros_pegados() {
        assert_eq!(numeros("1.5.5-2,3e1 -.25"), [1.5, 0.5, -2.0, 30.0, -0.25]);
    }

    #[test]
    fn rutas_relativas_y_absolutas() {
        let mut t = Trazador::nuevo(Matriz::IDENTIDAD);
        trazar_d("M10 10h5v5H10zm20 0l5 0 0 5z", &mut t);
        assert_eq!(t.poligonos.len(), 2);
        assert_eq!(t.poligonos[0], [(10.0, 10.0), (15.0, 10.0), (15.0, 15.0), (10.0, 15.0)]);
        assert_eq!(t.poligonos[1], [(30.0, 10.0), (35.0, 10.0), (35.0, 15.0)]);
    }

    #[test]
    fn banderas_de_arco_pegadas() {
        // Un punto de la lista de YouTube: dos arcos con las banderas pegadas a la x.
        let mut t = Trazador::nuevo(Matriz::IDENTIDAD);
        trazar_d("M12 4a2 2 0 100 4 2 2 0 000-4Z", &mut t);
        let p = &t.poligonos[0];
        let (ymin, ymax) = p.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
        let (xmin, xmax) = p.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
        assert!((ymin - 4.0).abs() < 0.01 && (ymax - 8.0).abs() < 0.01, "{ymin} {ymax}");
        assert!((xmin - 10.0).abs() < 0.05 && (xmax - 14.0).abs() < 0.05, "{xmin} {xmax}");
    }

    #[test]
    fn transformaciones() {
        let m = transformacion("translate(10 20) scale(2)");
        assert_eq!(m.aplicar((1.0, 1.0)), (12.0, 22.0));
        let r = transformacion("rotate(90)").aplicar((1.0, 0.0));
        assert!((r.0).abs() < 1e-4 && (r.1 - 1.0).abs() < 1e-4);
    }

    #[test]
    fn un_circulo_cierra() {
        let mut t = Trazador::nuevo(Matriz::IDENTIDAD);
        let el = Elemento {
            nombre: "circle".into(),
            atributos: alloc::vec![("cx".into(), "10".into()), ("cy".into(), "10".into()), ("r".into(), "5".into())],
            hijos: Vec::new(),
        };
        trazar_forma(&el, &mut t);
        let p = &t.poligonos[0];
        assert!(p.len() > 8);
        assert!(p.iter().all(|(x, y)| (cuadrado(x - 10.0) + cuadrado(y - 10.0) - 25.0).abs() < 0.5));
    }
}
