//! Flexbox (CSS Flexible Box Layout, §9), en versión compacta:
//!
//! 1. Cada hijo es un **elemento flexible** (los textos sueltos, uno anónimo), ordenados por
//!    `order`. Su **tamaño base**: `flex-basis`, o su `width`/`height`, o su contenido
//!    (`max-content` en fila; su alto maquetado en columna).
//! 2. Se reparten en **líneas** (una sola sin `flex-wrap`).
//! 3. En cada línea, el espacio libre se reparte con `flex-grow`, o lo que sobra se quita con
//!    `flex-shrink` (proporcional a tamaño × factor) sin bajar del mínimo del contenido.
//! 4. Los márgenes `auto` se quedan el espacio que sobre; si no hay, `justify-content`.
//! 5. El tamaño cruzado de cada línea es el de su elemento más grande; `align-items` /
//!    `align-self` colocan cada uno (`stretch` lo estira).
//!
//! Los elementos se maquetan con `bloque()`, primero «en seco» para medir (sus piezas se
//! descartan) y después en su sitio.

use crate::bloque::{Clase, Maquetador};
use crate::intrinseco::extras_horizontales;
use crate::linea::Item;
use alloc::vec::Vec;
use core::ops::Range;
use quimera_estilo::{AlinearFlex, Estilo, Justificar, Medida};
use quimera_html::NodoId;

#[derive(Debug, Clone, Copy)]
enum Que {
    Nodo(NodoId),
    /// Un texto suelto: elemento anónimo con el estilo del contenedor.
    Texto(NodoId),
}

/// Un elemento flexible. Los tamaños son de la caja con borde; los márgenes, aparte.
#[derive(Debug, Clone)]
struct Elem {
    que: Que,
    orden: i32,
    crecer: f32,
    encoger: f32,
    /// Márgenes en el eje principal y en el cruzado (inicio, fin); `auto` cuenta 0.
    m_pri: (f32, f32),
    m_cruz: (f32, f32),
    auto_pri: (bool, bool),
    base: f32,
    minimo: f32,
    /// Tamaño final en el eje principal.
    tam: f32,
    /// Tamaño en el eje cruzado.
    cruz: f32,
    alinear: AlinearFlex,
    /// Su tamaño cruzado es `auto` (se puede estirar).
    estirable: bool,
    /// Posición del borde en el eje principal, desde el inicio del contenido.
    pos: f32,
}

impl Elem {
    fn exterior(&self) -> f32 {
        self.tam + self.m_pri.0 + self.m_pri.1
    }
}

/// Lo que comparten las fases: el contenedor y sus ejes.
struct Contenedor<'a> {
    e: &'a Estilo,
    fila: bool,
    /// Tamaño del contenido en el eje principal (si se conoce) y en el cruzado.
    principal: Option<f32>,
    ancho: f32,
    alto: Option<f32>,
    hueco_pri: f32,
    hueco_cruz: f32,
}

impl<'a> Maquetador<'a> {
    /// Maqueta el contenido de un contenedor flex en (cx, cy), de `ancho` y, si se sabe, `alto`.
    /// Devuelve el alto del contenido y la primera línea base.
    pub(crate) fn flex(
        &mut self,
        id: NodoId,
        e: &'a Estilo,
        (cx, cy): (f32, f32),
        ancho: f32,
        alto: Option<f32>,
    ) -> (f32, Option<f32>) {
        let fila = e.flex.direccion.es_fila();
        let huecos = (e.flex.hueco_columnas.px(ancho, 0.0), e.flex.hueco_filas.px(ancho, 0.0));
        let (hueco_pri, hueco_cruz) = if fila { huecos } else { (huecos.1, huecos.0) };
        let c = Contenedor {
            e,
            fila,
            principal: if fila { Some(ancho) } else { alto },
            ancho,
            alto,
            hueco_pri,
            hueco_cruz,
        };
        let mut elems = self.elementos_flex(id, &c);
        if elems.is_empty() {
            return (0.0, None);
        }
        let lineas = partir(&elems, c.principal, c.hueco_pri, e.flex.envolver);
        for l in &lineas {
            resolver(&mut elems[l.clone()], c.principal, c.hueco_pri);
            colocar_principal(&mut elems[l.clone()], &c);
        }
        let cruces = self.tamanos_cruzados(&mut elems, &lineas, &c);
        self.colocar(&mut elems, &lineas, &cruces, &c, (cx, cy))
    }

    /// Los elementos flexibles con su tamaño base, en el orden de `order`.
    fn elementos_flex(&mut self, id: NodoId, c: &Contenedor<'a>) -> Vec<Elem> {
        let mut v = Vec::new();
        for h in self.doc.nodo(id).hijos.clone() {
            match self.clase(h) {
                Clase::Nada => {}
                // Un `absolute` no es un elemento flexible: va dentro de su antepasado posicionado.
                Clase::Absoluto => {
                    let estatico = (f32::NAN, f32::NAN);
                    self.absolutos.push(crate::posicion::Pendiente { id: h, estatico, apilado: self.apilado.clone() });
                }
                _ if self.es_blanco(h) => {}
                _ => v.push(match self.estilos.de(h) {
                    Some(_) => self.elemento_nodo(h, c),
                    None => self.elemento_texto(h, c),
                }),
            }
        }
        v.sort_by_key(|x| x.orden); // estable: a igual `order`, el orden del documento
        v
    }

    fn elemento_nodo(&mut self, h: NodoId, c: &Contenedor<'a>) -> Elem {
        let Some(eh) = self.estilos.de(h) else { return self.elemento_texto(h, c) };
        let px = |m: Medida| m.px(c.ancho, 0.0);
        let es_auto = |m: Medida| m == Medida::Auto;
        let (mi, md, ma, mb) = (eh.margen[3], eh.margen[1], eh.margen[0], eh.margen[2]);
        let ((pri, auto_pri), cruz) = if c.fila {
            (((px(mi), px(md)), (es_auto(mi), es_auto(md))), (px(ma), px(mb)))
        } else {
            (((px(ma), px(mb)), (es_auto(ma), es_auto(mb))), (px(mi), px(md)))
        };
        let alinear = eh.flex.alinear_propio.unwrap_or(c.e.flex.alinear_items);
        let tam_cruz = if c.fila { eh.alto } else { eh.ancho };
        let mut x = Elem {
            que: Que::Nodo(h),
            orden: eh.flex.orden,
            crecer: eh.flex.crecer,
            encoger: eh.flex.encoger,
            m_pri: pri,
            m_cruz: cruz,
            auto_pri,
            base: 0.0,
            minimo: 0.0,
            tam: 0.0,
            cruz: 0.0,
            alinear,
            estirable: tam_cruz == Medida::Auto && !(es_auto(if c.fila { ma } else { mi })),
            pos: 0.0,
        };
        (x.base, x.minimo) = self.base_flex(h, eh, c, &x);
        x.tam = x.base;
        x
    }

    /// (tamaño base, mínimo) de un elemento en el eje principal.
    fn base_flex(&mut self, h: NodoId, eh: &Estilo, c: &Contenedor<'a>, x: &Elem) -> (f32, f32) {
        if c.fila {
            let extras = extras_horizontales(eh);
            let maximo = self.intrinseco(h).1;
            let minimo = self.minimo_contenido(h);
            let fijo = match (eh.flex.base, eh.ancho) {
                (Medida::Auto, Medida::Auto) => None,
                (Medida::Auto, w) => Some(w.px(c.ancho, 0.0) + extras),
                (b, _) => Some(b.px(c.ancho, 0.0) + extras),
            };
            let especificado = match eh.ancho {
                Medida::Auto => f32::MAX,
                w => w.px(c.ancho, 0.0) + extras,
            };
            return (fijo.unwrap_or(maximo), minimo.min(especificado));
        }
        let extras =
            eh.relleno[0].px(c.ancho, 0.0) + eh.relleno[2].px(c.ancho, 0.0) + eh.borde[0].ocupa() + eh.borde[2].ocupa();
        let fijo = match (eh.flex.base, eh.alto) {
            (Medida::Px(b), _) => Some(b + extras),
            (Medida::Porcentaje(p), _) if c.alto.is_some() => c.alto.map(|a| a * p / 100.0 + extras),
            (_, Medida::Px(a)) => Some(a + extras),
            _ => None,
        };
        let natural = match fijo {
            Some(f) => f,
            None => {
                let ancho = self.ancho_en_columna(h, c, x);
                self.en_seco(|m| m.bloque_libre(h, 0.0, 0.0, ancho).alto)
            }
        };
        (natural, if fijo.is_some() { 0.0 } else { natural })
    }

    /// Ancho exterior de un elemento en una columna: todo si se estira; si no, su contenido.
    fn ancho_en_columna(&mut self, h: NodoId, c: &Contenedor<'a>, x: &Elem) -> f32 {
        if x.estirable && x.alinear == AlinearFlex::Estirar {
            return c.ancho;
        }
        let maximo = self.intrinseco(h).1 + x.m_cruz.0 + x.m_cruz.1;
        maximo.min(c.ancho)
    }

    fn elemento_texto(&mut self, h: NodoId, c: &Contenedor<'a>) -> Elem {
        let (minimo, maximo) = self.texto_intrinseco(h, c.e);
        let base = if c.fila { maximo } else { self.en_seco(|m| m.texto_suelto(h, c.e, (0.0, 0.0), c.ancho).0) };
        Elem {
            que: Que::Texto(h),
            orden: 0,
            crecer: 0.0,
            encoger: 1.0,
            m_pri: (0.0, 0.0),
            m_cruz: (0.0, 0.0),
            auto_pri: (false, false),
            base,
            minimo: if c.fila { minimo } else { base },
            tam: base,
            cruz: 0.0,
            alinear: c.e.flex.alinear_items,
            estirable: true,
            pos: 0.0,
        }
    }

    /// El tamaño cruzado de cada elemento (fila: su alto al ancho final) y el de cada línea.
    fn tamanos_cruzados(&mut self, elems: &mut [Elem], lineas: &[Range<usize>], c: &Contenedor<'a>) -> Vec<f32> {
        let mut cruces = Vec::with_capacity(lineas.len());
        for l in lineas {
            let mut mayor = 0.0f32;
            for x in &mut elems[l.clone()] {
                x.cruz = match x.que {
                    Que::Nodo(h) if c.fila => {
                        let ancho = x.exterior();
                        self.en_seco(|m| m.bloque_libre(h, 0.0, 0.0, ancho).alto)
                    }
                    Que::Nodo(h) => {
                        let ancho = self.ancho_en_columna(h, c, x);
                        ancho - x.m_cruz.0 - x.m_cruz.1
                    }
                    Que::Texto(h) if c.fila => self.en_seco(|m| m.texto_suelto(h, c.e, (0.0, 0.0), x.tam).0),
                    Que::Texto(_) => c.ancho,
                };
                mayor = mayor.max(x.cruz + x.m_cruz.0 + x.m_cruz.1);
            }
            // Una sola línea en un contenedor de tamaño cruzado conocido: la línea lo ocupa entero.
            let conocido = if c.fila { c.alto } else { Some(c.ancho) };
            cruces.push(match conocido {
                Some(t) if lineas.len() == 1 => t,
                _ => mayor,
            });
        }
        cruces
    }

    /// Coloca cada elemento en su sitio (ya sin «en seco»). Devuelve (alto, primera base).
    fn colocar(
        &mut self,
        elems: &mut [Elem],
        lineas: &[Range<usize>],
        cruces: &[f32],
        c: &Contenedor<'a>,
        (cx, cy): (f32, f32),
    ) -> (f32, Option<f32>) {
        let mut inicio_linea = 0.0;
        let mut primera_base = None;
        for (l, &cruz_linea) in lineas.iter().zip(cruces) {
            for x in &elems[l.clone()] {
                let libre = cruz_linea - x.cruz - x.m_cruz.0 - x.m_cruz.1;
                let estirar = x.estirable && x.alinear == AlinearFlex::Estirar;
                let desp = match x.alinear {
                    AlinearFlex::Fin => libre,
                    AlinearFlex::Centro => libre / 2.0,
                    _ => 0.0,
                };
                let cruz = if estirar { cruz_linea - x.m_cruz.0 - x.m_cruz.1 } else { x.cruz };
                let base = self.poner(x, c, (cx, cy), inicio_linea + desp, cruz);
                primera_base = primera_base.or(base);
            }
            inicio_linea += cruz_linea + c.hueco_cruz;
        }
        let total_cruz = (inicio_linea - c.hueco_cruz).max(0.0);
        let alto = if c.fila {
            total_cruz
        } else {
            let usado = elems.iter().map(|x| x.pos + x.tam + x.m_pri.1).fold(0.0, f32::max);
            c.alto.unwrap_or(usado)
        };
        (alto, primera_base)
    }

    /// Maqueta un elemento en su posición: `cruz_pos` desde el inicio de su línea (margen
    /// incluido) y `cruz` de tamaño cruzado.
    fn poner(&mut self, x: &Elem, c: &Contenedor<'a>, (cx, cy): (f32, f32), cruz_pos: f32, cruz: f32) -> Option<f32> {
        let (px, py, ancho_ext, alto_borde) = if c.fila {
            (cx + x.pos - x.m_pri.0, cy + cruz_pos + x.m_cruz.0, x.exterior(), cruz)
        } else {
            (cx + cruz_pos, cy + x.pos, cruz + x.m_cruz.0 + x.m_cruz.1, x.tam)
        };
        match x.que {
            Que::Nodo(h) => {
                let previo = self.alto_forzado.replace((h, alto_borde));
                let hecho = self.bloque_libre(h, px, py, ancho_ext);
                self.alto_forzado = previo;
                hecho.primera_base
            }
            Que::Texto(h) => self.texto_suelto(h, c.e, (px, py), ancho_ext).1,
        }
    }

    /// Un texto suelto (elemento anónimo) partido en líneas en (x, y) con este ancho.
    fn texto_suelto(&mut self, h: NodoId, e: &'a Estilo, (x, y): (f32, f32), ancho: f32) -> (f32, Option<f32>) {
        let mut items: Vec<Item<'a>> = Vec::new();
        let mut ultimo_espacio = true;
        self.recoger(h, e, &mut items, &mut ultimo_espacio);
        self.pendientes.clear(); // un texto suelto de un contenedor flex no tiene flotantes
        self.lineas(&items, e, x, y, ancho)
    }

    /// Ejecuta `f` y descarta lo que haya dibujado (para medir sin dejar rastro).
    pub(crate) fn en_seco<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let (p, e, a) = (self.piezas.len(), self.enlaces.len(), self.anclas.len());
        let (fl, capa, pend) = (self.flotantes.len(), self.capa_flotantes.len(), self.pendientes.len());
        let (abs, capas, cajas) = (self.absolutos.len(), self.capas.len(), self.cajas.len());
        let r = f(self);
        self.cajas.truncate(cajas);
        self.piezas.truncate(p);
        self.enlaces.truncate(e);
        self.anclas.truncate(a);
        self.flotantes.truncate(fl);
        self.capa_flotantes.truncate(capa);
        self.pendientes.truncate(pend);
        self.absolutos.truncate(abs);
        self.capas.truncate(capas);
        r
    }

    /// `bloque()` sin el `width` propio del elemento: ocupa el ancho exterior que se le da.
    pub(crate) fn bloque_libre(&mut self, h: NodoId, x: f32, y: f32, ancho_exterior: f32) -> crate::bloque::Hecho {
        let previo = self.ancho_libre.replace(h);
        let hecho = self.bloque(h, x, y, ancho_exterior);
        self.ancho_libre = previo;
        hecho
    }
}

/// Las líneas: rangos de elementos. Sin `wrap`, todo en una; con `wrap`, cabe lo que cabe.
fn partir(elems: &[Elem], principal: Option<f32>, hueco: f32, envolver: bool) -> Vec<Range<usize>> {
    let Some(disponible) = principal.filter(|_| envolver) else { return alloc::vec![0..elems.len()] };
    let mut lineas = Vec::new();
    let (mut inicio, mut usado) = (0, 0.0);
    for (i, x) in elems.iter().enumerate() {
        let extra = if i > inicio { hueco } else { 0.0 };
        if i > inicio && usado + extra + x.exterior() > disponible + 0.01 {
            lineas.push(inicio..i);
            (inicio, usado) = (i, 0.0);
            usado += x.exterior();
        } else {
            usado += extra + x.exterior();
        }
    }
    lineas.push(inicio..elems.len());
    lineas
}

/// Reparte el espacio libre de una línea (`flex-grow`) o quita lo que sobra (`flex-shrink`).
fn resolver(linea: &mut [Elem], principal: Option<f32>, hueco: f32) {
    let Some(disponible) = principal else { return };
    let huecos = hueco * linea.len().saturating_sub(1) as f32;
    let mut congelado = alloc::vec![false; linea.len()];
    for _ in 0..linea.len() + 1 {
        let usado: f32 = linea.iter().map(Elem::exterior).sum::<f32>() + huecos;
        let libre = disponible - usado;
        if libre.abs() < 0.01 {
            return;
        }
        let activos: Vec<usize> = (0..linea.len()).filter(|&i| !congelado[i]).collect();
        if libre > 0.0 {
            let suma: f32 = activos.iter().map(|&i| linea[i].crecer).sum();
            if suma <= 0.0 {
                return;
            }
            // Con factores que suman menos de 1, solo se reparte esa fracción del espacio.
            let repartir = if suma < 1.0 { libre * suma } else { libre };
            for &i in &activos {
                linea[i].tam += repartir * linea[i].crecer / suma;
            }
            return;
        }
        let suma: f32 = activos.iter().map(|&i| linea[i].encoger * linea[i].base).sum();
        if suma <= 0.0 {
            return;
        }
        let mut alguno = false;
        for &i in &activos {
            let x = &mut linea[i];
            let nuevo = x.tam + libre * (x.encoger * x.base) / suma;
            if nuevo < x.minimo {
                x.tam = x.minimo;
                congelado[i] = true;
                alguno = true;
            } else {
                x.tam = nuevo;
            }
        }
        if !alguno {
            return;
        }
    }
}

/// Posición de cada elemento en el eje principal: márgenes `auto` o `justify-content`.
fn colocar_principal(linea: &mut [Elem], c: &Contenedor<'_>) {
    let n = linea.len() as f32;
    let usado: f32 = linea.iter().map(Elem::exterior).sum::<f32>() + c.hueco_pri * (n - 1.0).max(0.0);
    let libre = c.principal.map_or(0.0, |d| d - usado);
    let autos = linea.iter().map(|x| u8::from(x.auto_pri.0) + u8::from(x.auto_pri.1)).sum::<u8>();
    let (mut inicio, mut entre) = (0.0, c.hueco_pri);
    let por_auto = if autos > 0 && libre > 0.0 { libre / f32::from(autos) } else { 0.0 };
    if por_auto == 0.0 {
        match (c.e.flex.justificar, libre > 0.0) {
            (Justificar::Fin, _) => inicio = libre,
            (Justificar::Centro, _) | (Justificar::Alrededor | Justificar::Uniforme, false) => inicio = libre / 2.0,
            (Justificar::Entre, true) if n > 1.0 => entre += libre / (n - 1.0),
            (Justificar::Alrededor, true) => {
                inicio = libre / (2.0 * n);
                entre += libre / n;
            }
            (Justificar::Uniforme, true) => {
                inicio = libre / (n + 1.0);
                entre += libre / (n + 1.0);
            }
            _ => {}
        }
    }
    let mut cursor = inicio;
    for x in linea.iter_mut() {
        if x.auto_pri.0 {
            cursor += por_auto;
        }
        x.pos = cursor + x.m_pri.0;
        cursor += x.exterior() + entre;
        if x.auto_pri.1 {
            cursor += por_auto;
        }
    }
    if c.e.flex.direccion.inversa() {
        let total = c.principal.unwrap_or(cursor - entre);
        for x in linea.iter_mut() {
            x.pos = total - (x.pos + x.tam);
        }
    }
}
