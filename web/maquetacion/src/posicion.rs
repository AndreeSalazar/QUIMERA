//! Posicionamiento (CSS 2.1 §9.3, §10.3.7, §10.6.4) y capas de `z-index` (apéndice E).
//!
//! - **`relative`**: se maqueta en el flujo y después se mueve (`left`/`top`, o `right`/`bottom`)
//!   con todo lo suyo. **`sticky`**, por ahora, se queda donde está.
//! - **`absolute`**: sale del flujo. Se anota donde habría ido (su «posición estática») y se
//!   coloca cuando su antepasado posicionado más cercano ya está maquetado, dentro de su caja de
//!   relleno; si no hay ninguno, dentro de la ventana. Con `left` y `right` (o `top` y `bottom`)
//!   el tamaño sale de ahí; si no, se encoge a su contenido. `margin: auto` con los dos lados
//!   lo centra. **`fixed`** se coloca como `absolute` respecto a la ventana, y su capa (con todo
//!   lo que lleva dentro) es **fija**: no se desplaza con la página (`Maquetacion::fijas`).
//! - **Capas**: cada elemento posicionado se dibuja después del contenido normal, en su orden
//!   del documento; `z-index` negativo va debajo y positivo encima. Un `z-index` que no es `auto`
//!   crea un contexto de apilamiento: lo de dentro se ordena dentro de él. Cada capa lleva su
//!   ruta de (z, orden) desde la raíz y se ordenan por ruta.

use crate::bloque::{Hecho, Maquetador};
use crate::intrinseco::extras_horizontales;
use crate::pintura::Pieza;
use alloc::vec::Vec;
use quimera_estilo::{Display, Estilo, Medida, Posicion};
use quimera_html::NodoId;

/// La ruta de una capa: (z-index, orden en el documento) de cada contexto de apilamiento.
pub(crate) type Ruta = Vec<(i32, usize)>;

/// Un elemento `absolute` o `fixed` que espera a su antepasado posicionado.
pub(crate) struct Pendiente {
    pub id: NodoId,
    /// Donde habría ido en el flujo (`NaN` si no se sabe: dentro de texto o de un flex).
    pub estatico: (f32, f32),
    /// El contexto de apilamiento en el que estaba.
    pub apilado: Ruta,
}

/// Las piezas de un elemento posicionado, con su sitio en el orden de dibujo.
#[derive(Clone)]
pub(crate) struct Capa {
    pub ruta: Ruta,
    pub piezas: Vec<Pieza>,
    /// De un elemento `fixed` (o de algo dentro de él): no se desplaza con la página.
    pub fija: bool,
}

/// En el número de nodo de un enlace o control: está en una capa fija.
pub(crate) const FIJO: u32 = 1 << 30;

/// Cuánto se mueve un `relative`: `left` (o `-right`) y `top` (o `-bottom`).
fn desplazamiento(e: &Estilo, ancho: f32) -> (f32, f32) {
    let [t, r, b, l] = e.colocacion.desplazar;
    let dx = match (l, r) {
        (Medida::Auto, Medida::Auto) => 0.0,
        (Medida::Auto, r) => -r.px(ancho, 0.0),
        (l, _) => l.px(ancho, 0.0),
    };
    // Un `top` en % depende del alto del contenedor, que aún no se sabe: cuenta 0.
    let px = |m: Medida| if let Medida::Px(v) = m { v } else { 0.0 };
    let dy = if t == Medida::Auto { -px(b) } else { px(t) };
    (dx, dy)
}

impl Maquetador<'_> {
    /// Si `e` es un elemento `absolute` o `fixed` (sale del flujo).
    pub(crate) fn fuera_del_flujo(e: &Estilo) -> bool {
        e.colocacion.posicion.fuera_del_flujo() && e.display != Display::Ninguno
    }

    /// Un elemento posicionado: se maqueta, coloca a sus `absolute`, se mueve si es `relative` y
    /// sus piezas pasan a su capa.
    pub(crate) fn posicionado(&mut self, id: NodoId, e: &Estilo, (x, y): (f32, f32), ancho: f32) -> Hecho {
        self.orden += 1;
        let mut ruta = self.apilado.clone();
        ruta.push((e.colocacion.z.unwrap_or(0), self.orden));
        let contexto = e.colocacion.z.is_some();
        let apilado_previo = contexto.then(|| core::mem::replace(&mut self.apilado, ruta.clone()));
        let (p0, e0, c0, a0) = (self.piezas.len(), self.enlaces.len(), self.capas.len(), self.absolutos.len());
        let k0 = self.cajas.len();
        let f0 = self.capa_flotantes.len();
        let guardia = self.guardia.replace(id);
        // Un `fixed`, y todo lo que hay dentro, va en la capa fija.
        let fija = self.en_fijo || e.colocacion.posicion == Posicion::Fija;
        let fuera_fijo = core::mem::replace(&mut self.en_fijo, fija);
        let hecho = self.bloque(id, x, y, ancho);
        self.guardia = guardia;
        // Sus `absolute` van dentro de su caja de relleno.
        let (bx, bw) = hecho.caja;
        let b = |i: usize| e.borde[i].ocupa();
        let caja = (bx + b(3), y + b(0), (bw - b(1) - b(3)).max(0.0), (hecho.alto - b(0) - b(2)).max(0.0));
        self.poner_absolutos(a0, caja, false);
        if e.colocacion.posicion == Posicion::Relativa {
            let (dx, dy) = desplazamiento(e, ancho);
            if dx != 0.0 || dy != 0.0 {
                self.piezas[p0..].iter_mut().for_each(|p| p.mover(dx, dy));
                self.capas[c0..].iter_mut().flat_map(|c| c.piezas.iter_mut()).for_each(|p| p.mover(dx, dy));
                for en in &mut self.enlaces[e0..] {
                    (en.0, en.1) = (en.0 + dx, en.1 + dy);
                }
                for (_, c) in &mut self.cajas[k0..] {
                    (c[0], c[1]) = (c[0] + dx, c[1] + dy);
                }
            }
        }
        // Sus flotantes van en su capa, encima de su fondo (no en la del contexto de fuera, que
        // se dibuja antes y quedaría tapada por él).
        let flotantes: Vec<Pieza> = self.capa_flotantes.drain(f0..).collect();
        self.piezas.extend(flotantes);
        let piezas = self.piezas.split_off(p0);
        if contexto {
            ruta.push((0, 0)); // su contenido normal, dentro de su contexto
        }
        if fija {
            self.enlaces[e0..].iter_mut().for_each(|en| en.4 |= FIJO);
        }
        self.capas.push(Capa { ruta, piezas, fija });
        self.en_fijo = fuera_fijo;
        if let Some(previo) = apilado_previo {
            self.apilado = previo;
        }
        hecho
    }

    /// Coloca los `absolute` anotados desde `desde` dentro de `caja` (x, y, ancho, alto). Los
    /// `fixed` siguen esperando a la ventana, salvo que esta sea la ventana.
    /// Con la ventana, los `fixed` que salen al colocar otros (uno dentro de un `absolute`) se
    /// colocan también, hasta que no queda ninguno.
    pub(crate) fn poner_absolutos(&mut self, desde: usize, caja: (f32, f32, f32, f32), ventana: bool) {
        loop {
            let todos: Vec<Pendiente> = self.absolutos.drain(desde..).collect();
            let es_fijo =
                |p: &Pendiente| self.estilos.de(p.id).is_some_and(|e| e.colocacion.posicion == Posicion::Fija);
            let (fijos, propios): (Vec<Pendiente>, Vec<Pendiente>) =
                todos.into_iter().partition(|p| !ventana && es_fijo(p));
            let esperan = self.absolutos.len();
            self.absolutos.extend(fijos);
            for p in propios {
                self.absoluto(p, caja);
            }
            // Sin ventana, los `fixed` esperan arriba; con ella, se repite si salieron más.
            if !ventana || self.absolutos.len() <= esperan {
                break;
            }
        }
    }

    /// Un elemento `absolute` dentro de la caja de relleno de su antepasado posicionado.
    fn absoluto(&mut self, p: Pendiente, (cx, cy, cw, ch): (f32, f32, f32, f32)) {
        let Some(e) = self.estilos.de(p.id) else { return };
        let [t, r, b, l] = e.colocacion.desplazar;
        let lado = |m: Medida, base: f32| (m != Medida::Auto).then(|| m.px(base, 0.0));
        let (arriba, der, abajo, izq) = (lado(t, ch), lado(r, cw), lado(b, ch), lado(l, cw));
        let px = |m: Medida| m.px(cw, 0.0);
        let (mut ml, mut mr, mt, mb) = (px(e.margen[3]), px(e.margen[1]), px(e.margen[0]), px(e.margen[2]));
        let ancho_borde = match e.ancho {
            Medida::Auto if izq.is_some() && der.is_some() => {
                (cw - izq.unwrap_or(0.0) - der.unwrap_or(0.0) - ml - mr).max(0.0)
            }
            // Encogido a su contenido, sin pasar del sitio que dejan sus lados.
            Medida::Auto => {
                let (min, max) = self.intrinseco(p.id);
                max.min((cw - izq.unwrap_or(0.0) - der.unwrap_or(0.0) - ml - mr).max(min))
            }
            m if e.display == Display::Tabla => m.px(cw, 0.0).max(self.intrinseco(p.id).0),
            m => m.px(cw, 0.0) - e.extras_si_caja_borde(cw) + extras_horizontales(e),
        };
        // Con los dos lados y el ancho fijados, `margin: auto` lo centra.
        if let (Some(i), Some(d)) = (izq, der)
            && e.ancho != Medida::Auto
            && e.margen[1] == Medida::Auto
            && e.margen[3] == Medida::Auto
        {
            let sobra = ((cw - i - d - ancho_borde) / 2.0).max(0.0);
            (ml, mr) = (sobra, sobra);
        }
        let (sx, sy) = if p.estatico.0.is_nan() { (cx, cy) } else { p.estatico };
        let x = match (izq, der) {
            (Some(i), _) => cx + i + ml,
            (None, Some(d)) => cx + cw - d - mr - ancho_borde,
            _ => sx + ml,
        };
        // `bloque_libre` pone los márgenes del estilo (`auto` cuenta 0): se le da su caja con ellos.
        let (ml_estilo, mr_estilo) = (px(e.margen[3]), px(e.margen[1]));
        let exterior = ancho_borde + ml_estilo + mr_estilo;
        let estirado = match (e.alto, arriba, abajo) {
            (Medida::Auto, Some(a), Some(b)) => Some((ch - a - b - mt - mb).max(0.0)),
            _ => None,
        };
        // Su `height` en porcentaje va contra el alto de la caja de su antepasado posicionado.
        let base = self.alto_base.replace(ch);
        let alto = estirado.unwrap_or_else(|| self.alto_libre(p.id, exterior));
        let y = match (arriba, abajo) {
            (Some(a), _) => cy + a + mt,
            (None, Some(b)) => cy + ch - b - mb - alto,
            _ => sy + mt,
        };
        let apilado = core::mem::replace(&mut self.apilado, p.apilado);
        let forzado = estirado.and_then(|h| self.alto_forzado.replace((p.id, h)));
        self.bloque_libre(p.id, x - ml_estilo, y, exterior);
        if estirado.is_some() {
            self.alto_forzado = forzado;
        }
        (self.apilado, self.alto_base) = (apilado, base);
    }

    /// Todas las piezas en orden de dibujo: el contenido normal y las capas por su ruta.
    /// Devuelve también dónde acaba la más baja de las capas que se desplazan (cuenta para el alto
    /// del documento; las fijas, no) y los rangos de piezas de las capas fijas.
    pub(crate) fn apilar(&mut self) -> (Vec<Pieza>, f32, Vec<core::ops::Range<usize>>) {
        let mut capas = core::mem::take(&mut self.capas);
        let abajo = capas.iter().filter(|c| !c.fija).flat_map(|c| &c.piezas).map(Pieza::abajo).fold(0.0, f32::max);
        capas.push(Capa { ruta: alloc::vec![(0, 0)], piezas: core::mem::take(&mut self.piezas), fija: false });
        capas.sort_by(|a, b| a.ruta.cmp(&b.ruta));
        let mut piezas = Vec::new();
        let mut fijas: Vec<core::ops::Range<usize>> = Vec::new();
        for c in capas {
            let inicio = piezas.len();
            piezas.extend(c.piezas);
            if c.fija && piezas.len() > inicio {
                match fijas.last_mut() {
                    Some(r) if r.end == inicio => r.end = piezas.len(),
                    _ => fijas.push(inicio..piezas.len()),
                }
            }
        }
        (piezas, abajo, fijas)
    }
}
