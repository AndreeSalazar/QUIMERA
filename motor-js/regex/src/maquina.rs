//! La máquina: ejecuta el programa sobre el texto (UTF-8, posiciones en bytes) con vuelta atrás.
//!
//! No usa recursión de Rust salvo para `(?=...)`, cuyo anidamiento lo limita el patrón: las
//! alternativas pendientes y lo que hay que deshacer van en una pila propia (`Memoria`), que
//! se reutiliza entre búsquedas para no pedir memoria cada vez.

use crate::clases::{es_fin_de_linea, es_palabra, plegar};
use crate::programa::{Op, Programa};
use alloc::vec::Vec;

/// Ranura sin valor (grupo que no participó).
pub(crate) const NADA: usize = usize::MAX;

#[derive(Debug, Clone, Copy)]
enum Marca {
    /// Una alternativa pendiente: volver a `pc` en `pos`.
    Volver { pc: u32, pos: usize },
    /// Deshacer una ranura de captura.
    Ranura { i: u32, viejo: usize },
    /// Deshacer una marca de posición.
    Registro { i: u32, viejo: usize },
}

/// Memoria de trabajo de la máquina; guárdala y reutilízala entre búsquedas.
#[derive(Debug, Default, Clone)]
pub struct Memoria {
    pila: Vec<Marca>,
    pub(crate) ranuras: Vec<usize>,
    registros: Vec<usize>,
}

impl Memoria {
    /// Dónde encajó el grupo `g` (0 = todo), en bytes, si participó.
    pub fn captura(&self, g: u32) -> Option<(usize, usize)> {
        let (a, b) = (*self.ranuras.get(2 * g as usize)?, *self.ranuras.get(2 * g as usize + 1)?);
        (a != NADA && b != NADA).then_some((a, b))
    }

    pub(crate) fn preparar(&mut self, grupos: u32, registros: u32) {
        self.ranuras.clear();
        self.ranuras.resize(2 * (grupos as usize + 1), NADA);
        self.registros.clear();
        self.registros.resize(registros as usize, NADA);
        self.pila.clear();
    }

    fn ranura(&mut self, i: u32, v: usize) {
        let viejo = self.ranuras[i as usize];
        self.pila.push(Marca::Ranura { i, viejo });
        self.ranuras[i as usize] = v;
    }

    /// Deshace todo hasta dejar la pila en `base`.
    fn deshacer(&mut self, base: usize) {
        while self.pila.len() > base {
            match self.pila.pop() {
                Some(Marca::Ranura { i, viejo }) => self.ranuras[i as usize] = viejo,
                Some(Marca::Registro { i, viejo }) => self.registros[i as usize] = viejo,
                _ => {}
            }
        }
    }

    /// Tras un `(?=...)` que encajó: se olvidan sus alternativas pero no cómo deshacer sus capturas.
    fn olvidar_alternativas(&mut self, base: usize) {
        let mut i = base;
        for j in base..self.pila.len() {
            if !matches!(self.pila[j], Marca::Volver { .. }) {
                self.pila[i] = self.pila[j];
                i += 1;
            }
        }
        self.pila.truncate(i);
    }
}

/// El carácter que empieza en `pos` y cuántos bytes ocupa.
#[inline]
pub(crate) fn siguiente(t: &[u8], pos: usize) -> Option<(char, usize)> {
    let b = *t.get(pos)?;
    if b < 0x80 {
        return Some((b as char, 1));
    }
    let (n, inicial) = match b {
        0xc0..=0xdf => (2, u32::from(b & 0x1f)),
        0xe0..=0xef => (3, u32::from(b & 0x0f)),
        _ => (4, u32::from(b & 0x07)),
    };
    let mut c = inicial;
    for k in 1..n {
        c = (c << 6) | u32::from(*t.get(pos + k)? & 0x3f);
    }
    char::from_u32(c).map(|c| (c, n))
}

/// El carácter que termina en `pos`.
fn anterior(t: &[u8], pos: usize) -> Option<char> {
    if pos == 0 {
        return None;
    }
    let mut i = pos - 1;
    while i > 0 && t[i] & 0xc0 == 0x80 {
        i -= 1;
    }
    siguiente(t, i).map(|(c, _)| c)
}

/// Busca desde `inicio` un encaje que empiece justo ahí.
pub(crate) fn encajar(p: &Programa, t: &[u8], inicio: usize, m: &mut Memoria) -> bool {
    m.pila.clear();
    m.ranuras.fill(NADA);
    m.registros.fill(NADA);
    correr(p, t, 0, inicio, m, 0)
}

/// Ejecuta desde `pc`; `false` cuando ya no queda alternativa por encima de `base`.
fn correr(p: &Programa, t: &[u8], mut pc: usize, mut pos: usize, m: &mut Memoria, base: usize) -> bool {
    loop {
        let sigue = match p.ops[pc] {
            Op::Dividir(a, b) => {
                m.pila.push(Marca::Volver { pc: b, pos });
                pc = a as usize;
                continue;
            }
            Op::Saltar(a) => {
                pc = a as usize;
                continue;
            }
            Op::Guardar(i) => {
                m.ranura(i, pos);
                true
            }
            Op::Limpiar(desde, hasta) => {
                for i in 2 * desde..=2 * hasta + 1 {
                    if m.ranuras[i as usize] != NADA {
                        m.ranura(i, NADA);
                    }
                }
                true
            }
            Op::MarcarPosicion(r) => {
                let viejo = m.registros[r as usize];
                m.pila.push(Marca::Registro { i: r, viejo });
                m.registros[r as usize] = pos;
                true
            }
            Op::ExigirAvance(r) => m.registros[r as usize] != pos,
            Op::Mirar { positiva, fin } => {
                let b = m.pila.len();
                let encaja = correr(p, t, pc + 1, pos, m, b);
                if encaja && positiva {
                    m.olvidar_alternativas(b);
                } else if encaja {
                    m.deshacer(b);
                }
                if encaja == positiva {
                    pc = fin as usize + 1;
                    continue;
                }
                false
            }
            Op::Exito => return true,
            op => match consumir(p, op, t, pos, m) {
                Some(nueva) => {
                    pos = nueva;
                    true
                }
                None => false,
            },
        };
        if sigue {
            pc += 1;
            continue;
        }
        // Falla: volver a la última alternativa, deshaciendo por el camino.
        loop {
            if m.pila.len() <= base {
                return false;
            }
            match m.pila.pop() {
                Some(Marca::Volver { pc: a, pos: b }) => {
                    pc = a as usize;
                    pos = b;
                    break;
                }
                Some(Marca::Ranura { i, viejo }) => m.ranuras[i as usize] = viejo,
                Some(Marca::Registro { i, viejo }) => m.registros[i as usize] = viejo,
                None => return false,
            }
        }
    }
}

/// Las operaciones que miran el texto: la nueva posición si encajan.
#[inline]
fn consumir(p: &Programa, op: Op, t: &[u8], pos: usize, m: &Memoria) -> Option<usize> {
    match op {
        Op::Caracter(c) => {
            if c.is_ascii() {
                return (t.get(pos) == Some(&(c as u8))).then_some(pos + 1);
            }
            let (x, n) = siguiente(t, pos)?;
            (x == c).then_some(pos + n)
        }
        Op::CaracterSinCaso(c) => {
            let (x, n) = siguiente(t, pos)?;
            (plegar(x) == c).then_some(pos + n)
        }
        Op::Punto { todo } => {
            let (x, n) = siguiente(t, pos)?;
            (todo || !es_fin_de_linea(x)).then_some(pos + n)
        }
        Op::Clase(k) => {
            let (x, n) = siguiente(t, pos)?;
            p.clases[k as usize].contiene(x).then_some(pos + n)
        }
        Op::Inicio { multilinea } => {
            (pos == 0 || multilinea && anterior(t, pos).is_some_and(es_fin_de_linea)).then_some(pos)
        }
        Op::Fin { multilinea } => {
            (pos == t.len() || multilinea && siguiente(t, pos).is_some_and(|(c, _)| es_fin_de_linea(c))).then_some(pos)
        }
        Op::Limite(si) => {
            let antes = anterior(t, pos).is_some_and(es_palabra);
            let despues = siguiente(t, pos).is_some_and(|(c, _)| es_palabra(c));
            ((antes != despues) == si).then_some(pos)
        }
        Op::Referencia(g) => referencia(p, t, pos, m, g),
        _ => Some(pos),
    }
}

/// `\1`: el mismo texto que capturó el grupo (vacío si no participó).
fn referencia(p: &Programa, t: &[u8], pos: usize, m: &Memoria, g: u32) -> Option<usize> {
    let Some((a, b)) = m.captura(g) else { return Some(pos) };
    let trozo = &t[a..b];
    if !p.sin_caso {
        return t.get(pos..pos + trozo.len()).filter(|x| *x == trozo).map(|_| pos + trozo.len());
    }
    let (mut i, mut j) = (0, pos);
    while i < trozo.len() {
        let (x, n) = siguiente(trozo, i)?;
        let (y, k) = siguiente(t, j)?;
        if plegar(x) != plegar(y) {
            return None;
        }
        i += n;
        j += k;
    }
    Some(j)
}
