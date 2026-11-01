//! Flotantes (CSS 2.1 §9.5): `float: left | right` y `clear`.
//!
//! - Cada **contexto de formato de bloque** (la raíz, cada flotante, cada celda, cada elemento
//!   flexible) lleva su lista de flotantes, con su caja de margen en coordenadas del documento.
//! - Un flotante se mide (ancho pedido, o «encogido para caber»: entre su mínimo y su máximo de
//!   contenido), y se pone lo más arriba posible (nunca por encima de un flotante anterior) y lo
//!   más a su lado posible donde quepa; si no cabe al lado de los que hay, baja.
//! - Las **líneas de texto** se acortan donde hay flotantes; si en una no cabe ni la primera
//!   palabra, baja hasta debajo del flotante.
//! - Lo que es su propio contexto (tablas, contenedores flex, `display: flow-root`, `overflow:
//!   hidden`) no se mete debajo de un flotante: se pone a su lado, o debajo si no cabe.
//! - `clear` baja un bloque (o un flotante) hasta debajo de los flotantes de ese lado.
//! - El contexto crece hasta abarcar sus flotantes; un bloque normal, no (sus flotantes pueden
//!   salir por abajo, como en los navegadores).
//! - Los flotantes se dibujan encima de los fondos de los bloques normales de su contexto.

use crate::bloque::{Hecho, Maquetador};
use crate::intrinseco::extras_horizontales;
use quimera_estilo::{Display, Flotar, Limpiar, Medida};
use quimera_html::NodoId;

/// Un flotante puesto: su caja de margen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Flotante {
    pub x: f32,
    pub y: f32,
    pub ancho: f32,
    pub alto: f32,
    pub derecha: bool,
}

fn toca(f: &Flotante, y: f32, alto: f32) -> bool {
    f.y < y + alto && f.y + f.alto > y && f.alto > 0.0
}

/// El espacio libre entre `x0` y `x1` en la franja `y .. y + alto`.
pub(crate) fn espacio(fl: &[Flotante], y: f32, alto: f32, x0: f32, x1: f32) -> (f32, f32) {
    let (mut izq, mut der) = (x0, x1);
    for f in fl.iter().filter(|f| toca(f, y, alto)) {
        if f.derecha {
            der = der.min(f.x);
        } else {
            izq = izq.max(f.x + f.ancho);
        }
    }
    (izq, der)
}

/// Dónde acaba el primero de los flotantes que tocan la franja (para bajar hasta ahí).
pub(crate) fn bajar(fl: &[Flotante], y: f32, alto: f32) -> Option<f32> {
    fl.iter().filter(|f| toca(f, y, alto)).map(|f| f.y + f.alto).filter(|&b| b > y).reduce(f32::min)
}

/// Debajo de los flotantes del lado que pide `clear`.
fn debajo(fl: &[Flotante], limpiar: Limpiar, y: f32) -> f32 {
    let cuenta = |f: &&Flotante| match limpiar {
        Limpiar::No => false,
        Limpiar::Izquierda => !f.derecha,
        Limpiar::Derecha => f.derecha,
        Limpiar::Ambos => true,
    };
    fl.iter().filter(cuenta).map(|f| f.y + f.alto).fold(y, f32::max)
}

/// Dónde va un flotante de `ancho × alto` (caja de margen) entre `x0` y `x1`, desde `y` hacia abajo.
fn colocar(fl: &[Flotante], (ancho, alto): (f32, f32), (x0, x1): (f32, f32), mut y: f32, derecha: bool) -> (f32, f32) {
    loop {
        let (izq, der) = espacio(fl, y, alto.max(0.01), x0, x1);
        let libre = izq <= x0 && der >= x1;
        if der - izq >= ancho - 0.01 || libre {
            return (if derecha { (der - ancho).max(izq.min(x0)) } else { izq }, y);
        }
        match bajar(fl, y, alto.max(0.01)) {
            Some(b) => y = b,
            None => return (if derecha { der - ancho } else { izq }, y),
        }
    }
}

impl Maquetador<'_> {
    /// Pone el flotante `id` en un bloque cuyo contenido empieza en `cx` y mide `ancho`, no más
    /// arriba de `y`.
    pub(crate) fn flotante(&mut self, id: NodoId, cx: f32, y: f32, ancho: f32) {
        let Some(e) = self.estilos.de(id) else { return };
        let px = |m: Medida| m.px(ancho, 0.0);
        let (mt, mr, mb, ml) = (px(e.margen[0]), px(e.margen[1]), px(e.margen[2]), px(e.margen[3]));
        let ancho_borde = match e.ancho {
            // Encogido para caber: su máximo, sin pasar del sitio que hay ni bajar de su mínimo.
            Medida::Auto => {
                let (min, max) = self.intrinseco(id);
                max.min((ancho - ml - mr).max(min))
            }
            // Una tabla nunca es más estrecha que su contenido (y su `width` ya incluye el borde).
            m if e.display == Display::Tabla => m.px(ancho, 0.0).max(self.intrinseco(id).0),
            m => m.px(ancho, 0.0) + extras_horizontales(e),
        };
        let exterior = ancho_borde + ml + mr;
        let alto = self.en_seco(|m| m.bloque_libre(id, 0.0, 0.0, exterior).alto) + mt + mb;
        // Nunca por encima de un flotante anterior; con `clear`, debajo de los de ese lado.
        let y = self.flotantes.iter().map(|f| f.y).fold(y, f32::max);
        let y = debajo(&self.flotantes, e.limpiar, y);
        let derecha = e.flotar == Flotar::Derecha;
        let (fx, fy) = colocar(&self.flotantes, (exterior, alto), (cx, cx + ancho), y, derecha);
        let p0 = self.piezas.len();
        let hecho = self.bloque_libre(id, fx, fy + mt, exterior);
        let piezas = self.piezas.split_off(p0);
        self.capa_flotantes.extend(piezas);
        self.flotantes.push(Flotante { x: fx, y: fy, ancho: exterior, alto: hecho.alto + mt + mb, derecha });
    }

    /// Los flotantes que aparecieron dentro del contenido en línea, arriba de sus líneas.
    pub(crate) fn poner_pendientes(&mut self, cx: f32, y: f32, ancho: f32) {
        for id in core::mem::take(&mut self.pendientes) {
            self.flotante(id, cx, y, ancho);
        }
    }

    /// `clear` de un bloque: debajo de los flotantes de ese lado.
    pub(crate) fn despejar(&self, id: NodoId, y: f32) -> f32 {
        match self.estilos.de(id) {
            Some(e) if e.limpiar != Limpiar::No => debajo(&self.flotantes, e.limpiar, y),
            _ => y,
        }
    }

    /// Dónde acaba el más bajo de los flotantes del contexto (si hay alguno).
    pub(crate) fn fondo_flotantes(&self) -> Option<f32> {
        self.flotantes.iter().map(|f| f.y + f.alto).reduce(f32::max)
    }

    /// Un bloque hijo en (cx, y) con `ancho`. Si es su propio contexto (una tabla o un contenedor
    /// flex), no se mete debajo de los flotantes: va a su lado, o debajo si no cabe.
    pub(crate) fn bloque_junto(&mut self, h: NodoId, cx: f32, y: f32, ancho: f32) -> Hecho {
        let propio = self
            .estilos
            .de(h)
            .is_some_and(|e| matches!(e.display, Display::Tabla | Display::Flex | Display::RaizFlujo) || e.recorta);
        if !propio || self.flotantes.is_empty() {
            return self.bloque(h, cx, y, ancho);
        }
        let es_tabla = self.estilos.de(h).is_some_and(|e| e.display == Display::Tabla);
        let minimo = if es_tabla { self.intrinseco(h).0 } else { 0.0 };
        let mut arriba = y;
        loop {
            let (izq, der) = espacio(&self.flotantes, arriba, 1.0, cx, cx + ancho);
            let siguiente = bajar(&self.flotantes, arriba, 1.0);
            if der - izq >= minimo || siguiente.is_none() {
                let hecho = self.bloque(h, izq, arriba, der - izq);
                return Hecho { alto: hecho.alto + arriba - y, ..hecho };
            }
            arriba = siguiente.unwrap_or(arriba);
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const IZQ: Flotante = Flotante { x: 0.0, y: 0.0, ancho: 100.0, alto: 50.0, derecha: false };
    const DER: Flotante = Flotante { x: 700.0, y: 0.0, ancho: 100.0, alto: 80.0, derecha: true };

    #[test]
    fn el_espacio_entre_flotantes() {
        assert_eq!(espacio(&[IZQ, DER], 10.0, 20.0, 0.0, 800.0), (100.0, 700.0));
        assert_eq!(espacio(&[IZQ, DER], 60.0, 20.0, 0.0, 800.0), (0.0, 700.0), "el izquierdo ya acabó");
        assert_eq!(bajar(&[IZQ, DER], 10.0, 20.0), Some(50.0));
    }

    #[test]
    fn si_no_cabe_baja() {
        // 650 no caben entre 100 y 700: baja hasta y = 50, donde el izquierdo ya acabó.
        assert_eq!(colocar(&[IZQ, DER], (650.0, 10.0), (0.0, 800.0), 0.0, false), (0.0, 50.0));
        assert_eq!(colocar(&[IZQ, DER], (100.0, 10.0), (0.0, 800.0), 0.0, true), (600.0, 0.0));
        assert_eq!(debajo(&[IZQ, DER], Limpiar::Izquierda, 0.0), 50.0);
        assert_eq!(debajo(&[IZQ, DER], Limpiar::Ambos, 0.0), 80.0);
    }
}
