//! Tamaños intrínsecos: cuánto mide un elemento de ancho como **mínimo** (su palabra o imagen
//! más ancha: `min-content`) y como **máximo** (todo en una línea: `max-content`).
//!
//! Los usa flexbox para el tamaño base de sus elementos y para no encogerlos por debajo de su
//! contenido; también servirán para `float`, `inline-block` y las tablas. Se guardan por nodo.

use crate::bloque::{Clase, Maquetador};
use crate::linea::{Item, ancho_item};
use alloc::vec::Vec;
use quimera_estilo::{Display, Espacios, Estilo, Medida};
use quimera_html::NodoId;

/// (mínimo, máximo) en píxeles.
pub(crate) type Intrinseco = (f32, f32);

/// Los anchos de un contenido en línea: el máximo es su línea más larga; el mínimo, su pieza
/// más ancha (o la línea entera si no se puede partir).
pub(crate) fn de_items(m: &dyn crate::texto::MedirTexto, items: &[Item<'_>]) -> Intrinseco {
    let (mut minimo, mut maximo, mut linea) = (0.0f32, 0.0f32, 0.0f32);
    let mut sin_partir = 0.0f32; // lo que va seguido sin poder partirse
    for it in items {
        let w = ancho_item(m, it);
        match it {
            Item::Salto { .. } => {
                maximo = maximo.max(linea);
                minimo = minimo.max(sin_partir);
                (linea, sin_partir) = (0.0, 0.0);
            }
            Item::Espacio { estilo, fijo: false, .. } if parte(estilo) => {
                minimo = minimo.max(sin_partir);
                sin_partir = 0.0;
                linea += w;
            }
            _ => {
                linea += w;
                sin_partir += w;
            }
        }
    }
    (minimo.max(sin_partir), maximo.max(linea))
}

fn parte(e: &Estilo) -> bool {
    matches!(e.espacios, Espacios::Normal | Espacios::PreAjustado | Espacios::PreLinea)
}

/// Lo que suman en horizontal los rellenos y bordes (los porcentajes cuentan 0).
pub(crate) fn extras_horizontales(e: &Estilo) -> f32 {
    e.relleno[1].px(0.0, 0.0) + e.relleno[3].px(0.0, 0.0) + e.borde[1].ocupa() + e.borde[3].ocupa()
}

/// Los márgenes horizontales (`auto` y porcentajes cuentan 0).
pub(crate) fn margenes_horizontales(e: &Estilo) -> f32 {
    e.margen[1].px(0.0, 0.0) + e.margen[3].px(0.0, 0.0)
}

impl<'a> Maquetador<'a> {
    /// (mínimo, máximo) del ancho de la caja con borde del elemento `id` (sin sus márgenes).
    pub(crate) fn intrinseco(&mut self, id: NodoId) -> Intrinseco {
        if let Some(r) = self.intrinsecos.get(id.0 as usize).copied().flatten() {
            return r;
        }
        let r = self.calcular_intrinseco(id);
        if let Some(c) = self.intrinsecos.get_mut(id.0 as usize) {
            *c = Some(r);
        }
        r
    }

    fn calcular_intrinseco(&mut self, id: NodoId) -> Intrinseco {
        let Some(e) = self.estilos.de(id) else { return (0.0, 0.0) };
        if e.display == Display::Ninguno {
            return (0.0, 0.0);
        }
        let extras = extras_horizontales(e);
        if crate::reemplazado::es_imagen(self, id) {
            let w = self.tam_imagen(id, e, 0.0).0 + extras;
            return (w, w);
        }
        if let Medida::Px(w) = e.ancho {
            return (w + extras, w + extras);
        }
        let (mut minimo, mut maximo) = self.contenido_intrinseco(id, e);
        if let Medida::Px(tope) = e.ancho_max {
            (minimo, maximo) = (minimo.min(tope), maximo.min(tope));
        }
        (minimo + extras, maximo + extras)
    }

    /// El mínimo del **contenido** de `id` con sus rellenos y bordes, sin mirar su `width` (el
    /// mínimo automático de un elemento flexible: no se encoge por debajo de su contenido).
    pub(crate) fn minimo_contenido(&mut self, id: NodoId) -> f32 {
        let Some(e) = self.estilos.de(id) else { return 0.0 };
        self.contenido_intrinseco(id, e).0 + extras_horizontales(e)
    }

    /// Los anchos del contenido de `id`: bloques uno debajo de otro (cuenta el más ancho) y
    /// contenido en línea; en un contenedor flex en fila, los elementos uno al lado de otro.
    fn contenido_intrinseco(&mut self, id: NodoId, e: &'a Estilo) -> Intrinseco {
        let en_fila = e.display == Display::Flex && e.flex.direccion.es_fila();
        let (mut minimo, mut maximo) = (0.0f32, 0.0f32);
        let mut items: Vec<Item<'a>> = Vec::new();
        let mut ultimo_espacio = true;
        let hijos = self.doc.nodo(id).hijos.clone();
        let mut cuantos = 0;
        let juntar = |minimo: &mut f32, maximo: &mut f32, (a, b): Intrinseco| {
            if en_fila {
                *minimo = if e.flex.envolver { minimo.max(a) } else { *minimo + a };
                *maximo += b;
            } else {
                *minimo = minimo.max(a);
                *maximo = maximo.max(b);
            }
        };
        for h in hijos {
            match self.clase(h) {
                Clase::Nada => {}
                Clase::EnLinea if !en_fila => self.recoger(h, e, &mut items, &mut ultimo_espacio),
                _ => {
                    if !items.is_empty() {
                        juntar(&mut minimo, &mut maximo, de_items(self.medir, &items));
                        items.clear();
                        ultimo_espacio = true;
                    }
                    let (a, b) = match self.estilos.de(h) {
                        Some(eh) => {
                            let (a, b) = self.intrinseco(h);
                            let m = margenes_horizontales(eh);
                            (a + m, b + m)
                        }
                        None => self.texto_intrinseco(h, e),
                    };
                    juntar(&mut minimo, &mut maximo, (a, b));
                    cuantos += 1;
                }
            }
        }
        if !items.is_empty() {
            juntar(&mut minimo, &mut maximo, de_items(self.medir, &items));
        }
        if en_fila && cuantos > 1 {
            let hueco = e.flex.hueco_columnas.px(0.0, 0.0) * (cuantos - 1) as f32;
            maximo += hueco;
            if !e.flex.envolver {
                minimo += hueco;
            }
        }
        (minimo, maximo)
    }

    /// Los anchos de un texto suelto (por ejemplo, dentro de un contenedor flex).
    pub(crate) fn texto_intrinseco(&mut self, id: NodoId, padre: &'a Estilo) -> Intrinseco {
        let mut items: Vec<Item<'a>> = Vec::new();
        let mut ultimo_espacio = true;
        self.recoger(id, padre, &mut items, &mut ultimo_espacio);
        de_items(self.medir, &items)
    }
}
