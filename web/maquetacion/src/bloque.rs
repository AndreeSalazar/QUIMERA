//! Bloques: cada uno ocupa el ancho de su contenedor (o el suyo), apila a sus hijos de arriba
//! abajo y sus márgenes verticales colapsan como en CSS:
//! - entre hermanos, cuenta el mayor de los dos márgenes (no la suma);
//! - el margen superior del primer hijo sale por arriba de su padre si este no tiene borde ni
//!   relleno arriba (y el inferior del último, por abajo, si además su alto es automático).
//!
//! Los hijos en línea seguidos (texto, `<b>`, `<a>`, `<img>`) forman un bloque anónimo que se
//! parte en líneas (`linea.rs`).

use crate::flotante::Flotante;
use crate::linea::{EnLinea, EnlaceBruto, Item, texto_a_items};
use crate::pintura::Pieza;
use crate::posicion::{Capa, Pendiente, Ruta};
use crate::texto::MedirTexto;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_estilo::{Display, Estilo, Estilos, Flotar, Medida, TipoLista};
use quimera_html::{Dato, Documento, NodoId};

pub(crate) struct Maquetador<'a> {
    pub doc: &'a Documento,
    pub estilos: &'a Estilos,
    pub medir: &'a dyn MedirTexto,
    pub piezas: Vec<Pieza>,
    /// Si un elemento en línea tiene bloques dentro (se calcula una vez por elemento).
    contiene_bloque: Vec<Option<bool>>,
    /// El elemento raíz: sus márgenes no colapsan con los de sus hijos (CSS 2.1, 8.3.1).
    pub raiz: Option<NodoId>,
    /// Los `href` de los enlaces vistos y su `<a>` (los elementos guardan su número).
    pub hrefs: Vec<(String, NodoId)>,
    /// El enlace dentro del que se está maquetando.
    enlace: Option<u32>,
    /// Los rectángulos de los enlaces dibujados.
    pub enlaces: Vec<EnlaceBruto>,
    /// (id, y) de cada bloque con `id`: a dónde llevan los `#ancla`.
    pub anclas: Vec<(String, f32)>,
    /// La caja de borde (x, y, ancho, alto) de cada bloque maquetado, para las medidas del DOM.
    pub cajas: Vec<(NodoId, [f32; 4])>,
    /// Tamaños intrínsecos ya calculados, por nodo.
    pub intrinsecos: Vec<Option<(f32, f32)>>,
    /// Este elemento ignora su `width`: ocupa el ancho que se le da (elementos flexibles).
    pub ancho_libre: Option<NodoId>,
    /// Alto de la caja con borde impuesto a este elemento (`align-items: stretch`, columnas).
    pub alto_forzado: Option<(NodoId, f32)>,
    /// El alto de contenido definido del bloque contenedor: contra él se resuelven los
    /// `height` en porcentaje (sin él, cuentan como `auto`).
    pub alto_base: Option<f32>,
    /// Altos ya medidos en seco: (nodo, ancho, alto base, alto impuesto) → alto (ver
    /// `Maquetador::alto_libre`).
    pub altos_en_seco: alloc::collections::BTreeMap<(u32, u32, u32, u32), f32>,
    /// Se maqueta un elemento `fixed` (o algo dentro de él): sus capas son fijas.
    pub en_fijo: bool,
    /// Bordes (arriba, derecha, abajo, izquierda) que ocupa este elemento en vez de los suyos, sin
    /// dibujarlos: las celdas de una tabla con bordes que colapsan (los dibuja la tabla).
    pub bordes_celda: Option<(NodoId, [f32; 4])>,
    /// Los flotantes del contexto de formato actual.
    pub flotantes: Vec<Flotante>,
    /// Las piezas de los flotantes: se dibujan al final de su contexto, encima de los bloques.
    pub capa_flotantes: Vec<Pieza>,
    /// Flotantes vistos dentro de contenido en línea, que se ponen arriba de sus líneas.
    pub pendientes: Vec<NodoId>,
    /// Los `absolute`/`fixed` que esperan a su antepasado posicionado.
    pub absolutos: Vec<Pendiente>,
    /// Las piezas de los elementos posicionados, cada una con su sitio en el orden de dibujo.
    pub capas: Vec<Capa>,
    /// El contexto de apilamiento actual (ruta de `z-index` desde la raíz).
    pub apilado: Ruta,
    /// Cuántos elementos posicionados se han visto (su orden en el documento).
    pub orden: usize,
    /// El elemento posicionado que se está maquetando ahora (para no volver a posicionarlo).
    pub guardia: Option<NodoId>,
    /// Lo escrito en los controles y cuál tiene el foco (lo sabe la ventana).
    pub formulario: &'a dyn crate::control::Formulario,
    /// El tamaño propio de cada imagen por su `src` (si ya se descargó).
    pub imagenes: &'a dyn Fn(&str) -> Option<(u32, u32)>,
    /// Ancho del contenido del bloque que se está llenando (para los `%` de las imágenes en línea).
    pub ancho_actual: f32,
    /// Se mide el ancho intrínseco: las imágenes en línea no resuelven su `width` en porcentaje.
    pub midiendo: bool,
}

/// Lo que un bloque maquetado devuelve a su padre.
pub(crate) struct Hecho {
    /// Alto de la caja con su borde.
    pub alto: f32,
    /// Margen inferior que sigue «abierto» para colapsar con lo que venga después.
    pub margen_inferior: f32,
    /// Línea base de su primera línea de texto (para el marcador de una lista).
    pub primera_base: Option<f32>,
    /// Izquierda y ancho de su caja con borde.
    pub caja: (f32, f32),
}

/// Qué es un hijo para su bloque.
pub(crate) enum Clase {
    Bloque,
    /// `float: left | right`: fuera del flujo, a un lado.
    Flotante,
    /// `position: absolute | fixed`: fuera del flujo, dentro de su antepasado posicionado.
    Absoluto,
    EnLinea,
    Nada,
}

impl<'a> Maquetador<'a> {
    pub(crate) fn nuevo(
        doc: &'a Documento,
        estilos: &'a Estilos,
        medir: &'a dyn MedirTexto,
        imagenes: &'a dyn Fn(&str) -> Option<(u32, u32)>,
    ) -> Maquetador<'a> {
        Maquetador {
            doc,
            estilos,
            medir,
            piezas: Vec::new(),
            contiene_bloque: alloc::vec![None; doc.total()],
            raiz: None,
            hrefs: Vec::new(),
            enlace: None,
            enlaces: Vec::new(),
            anclas: Vec::new(),
            cajas: Vec::new(),
            intrinsecos: alloc::vec![None; doc.total()],
            ancho_libre: None,
            alto_forzado: None,
            alto_base: None,
            altos_en_seco: alloc::collections::BTreeMap::new(),
            en_fijo: false,
            bordes_celda: None,
            flotantes: Vec::new(),
            capa_flotantes: Vec::new(),
            pendientes: Vec::new(),
            absolutos: Vec::new(),
            capas: Vec::new(),
            apilado: Vec::new(),
            orden: 0,
            guardia: None,
            imagenes,
            formulario: &crate::control::SinFormulario,
            ancho_actual: 0.0,
            midiendo: false,
        }
    }

    fn estilo(&self, id: NodoId) -> Option<&'a Estilo> {
        self.estilos.de(id)
    }

    pub(crate) fn clase(&mut self, id: NodoId) -> Clase {
        match &self.doc.nodo(id).dato {
            Dato::Texto(_) => return Clase::EnLinea,
            Dato::Elemento(_) => {}
            _ => return Clase::Nada,
        }
        if self.estilo(id).is_some_and(Self::fuera_del_flujo) {
            return Clase::Absoluto;
        }
        if self.estilo(id).is_some_and(|e| e.flotar != Flotar::No && e.display != Display::Ninguno) {
            return Clase::Flotante;
        }
        match self.estilo(id).map(|e| e.display) {
            None | Some(Display::Ninguno) => Clase::Nada,
            Some(Display::Columna) => Clase::Nada,
            // Las partes de una tabla fuera de una tabla se maquetan como bloques (los navegadores
            // las meten en una tabla anónima).
            Some(
                Display::Bloque
                | Display::RaizFlujo
                | Display::ElementoDeLista
                | Display::Flex
                | Display::Tabla
                | Display::GrupoFilas
                | Display::Fila
                | Display::Celda
                | Display::Titulo,
            ) => Clase::Bloque,
            // Un elemento en línea con bloques dentro se maqueta como un bloque (los
            // navegadores lo parten en trozos; el resultado visible es casi el mismo).
            Some(Display::BloqueEnLinea) => Clase::EnLinea,
            Some(_) if self.tiene_bloques(id) => Clase::Bloque,
            Some(_) => Clase::EnLinea,
        }
    }

    fn tiene_bloques(&mut self, id: NodoId) -> bool {
        if let Some(r) = self.contiene_bloque[id.0 as usize] {
            return r;
        }
        let hijos = self.doc.nodo(id).hijos.clone();
        let r = hijos.into_iter().any(|h| matches!(self.clase(h), Clase::Bloque));
        self.contiene_bloque[id.0 as usize] = Some(r);
        r
    }

    /// Margen superior que el bloque `id` pone hacia fuera (con el de su primer hijo si colapsan).
    fn margen_superior(&mut self, id: NodoId, ancho_contenedor: f32) -> f32 {
        let Some(e) = self.estilo(id) else { return 0.0 };
        let propio = e.margen[0].px(ancho_contenedor, 0.0);
        // Un contenedor flex o una tabla no dejan salir los márgenes de sus hijos.
        if matches!(e.display, Display::Flex | Display::Tabla | Display::RaizFlujo)
            || e.recorta
            || e.borde[0].ocupa() > 0.0
            || e.relleno[0].px(ancho_contenedor, 0.0) > 0.0
        {
            return propio;
        }
        let primero =
            self.doc.nodo(id).hijos.clone().into_iter().find(|&h| {
                !self.es_blanco(h) && !matches!(self.clase(h), Clase::Nada | Clase::Flotante | Clase::Absoluto)
            });
        match primero {
            Some(h) if matches!(self.clase(h), Clase::Bloque) => {
                mayor_margen(propio, self.margen_superior(h, ancho_contenedor))
            }
            _ => propio,
        }
    }

    /// Un texto de solo blancos en modo normal (no crea líneas entre bloques).
    pub(crate) fn es_blanco(&self, id: NodoId) -> bool {
        match &self.doc.nodo(id).dato {
            Dato::Texto(t) => {
                let normal = self.doc.nodo(id).padre.and_then(|p| self.estilo(p)).is_none_or(|e| {
                    !matches!(e.espacios, quimera_estilo::Espacios::Pre | quimera_estilo::Espacios::PreAjustado)
                });
                normal && t.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}'))
            }
            Dato::Comentario(_) => true,
            _ => false,
        }
    }

    /// Lo que no se maqueta como un bloque normal: un elemento posicionado (que pasa a su capa),
    /// una imagen o una tabla.
    fn especial(&mut self, id: NodoId, e: &'a Estilo, (x, y): (f32, f32), ancho: f32) -> Option<Hecho> {
        if e.colocacion.posicion.posicionado() && self.guardia != Some(id) {
            return Some(self.posicionado(id, e, (x, y), ancho));
        }
        if crate::reemplazado::es_imagen(self, id) {
            return Some(self.bloque_imagen(id, e, (x, y), ancho));
        }
        if let Some(tipo) = crate::control::tipo_control(self, id) {
            return Some(self.control(id, tipo, e, (x, y), ancho));
        }
        (e.display == Display::Tabla).then(|| self.tabla(id, e, (x, y), ancho))
    }

    /// Maquetar el bloque `id` con su borde empezando en (x, y) dentro de un contenedor de
    /// `ancho_contenedor`. Su margen superior ya lo ha puesto quien llama.
    pub(crate) fn bloque(&mut self, id: NodoId, x: f32, y: f32, ancho_contenedor: f32) -> Hecho {
        let Some(e) = self.estilo(id) else {
            return Hecho { alto: 0.0, margen_inferior: 0.0, primera_base: None, caja: (x, 0.0) };
        };
        if let Some(hecho) = self.especial(id, e, (x, y), ancho_contenedor) {
            return hecho;
        }
        let px = |m: Medida| m.px(ancho_contenedor, 0.0);
        let impuestos = self.bordes_celda.filter(|(n, _)| *n == id).map(|(_, b)| b);
        let [bt, br, bb, bl] =
            impuestos.unwrap_or([e.borde[0].ocupa(), e.borde[1].ocupa(), e.borde[2].ocupa(), e.borde[3].ocupa()]);
        let (pt, pr, pb, pl) = (px(e.relleno[0]), px(e.relleno[1]), px(e.relleno[2]), px(e.relleno[3]));
        let (ml, mr) = if self.ancho_libre == Some(id) {
            (px(e.margen[3]), px(e.margen[1]))
        } else {
            let (ml, mr) = self.margenes_horizontales(e, ancho_contenedor, bl + br + pl + pr);
            // Dentro de `<center>` (o `align=center`), lo que sobra se reparte a los dos lados.
            let sobra = mr - px(e.margen[1]);
            if self.padre_centra(id) && sobra > 0.0 && e.margen[1] != Medida::Auto && e.margen[3] != Medida::Auto {
                (ml + sobra / 2.0, mr - sobra / 2.0)
            } else {
                (ml, mr)
            }
        };
        let forzado = self.alto_forzado.filter(|(n, _)| *n == id).map(|(_, a)| a);
        let ancho_borde = (ancho_contenedor - ml - mr).max(0.0);
        let ancho_contenido = (ancho_borde - bl - br - pl - pr).max(0.0);
        let (cx, cy) = (x + ml + bl + pl, y + bt + pt);
        let inicio_piezas = self.piezas.len();
        if let Some(i) = self.doc.elemento(id).and_then(|el| el.atributo("id")) {
            self.anclas.push((String::from(i), y));
        }
        let enlace_previo = self.entrar_en_enlace(id);
        let propio = self.contexto_propio(id, e);
        let contexto = propio.then(|| core::mem::take(&mut self.flotantes));
        let capa_inicio = self.capa_flotantes.len();
        let colapsa_arriba = bt + pt == 0.0 && !propio;
        let definido = self.alto_definido(e, forzado, bt + pt + pb + bb);
        let base_previa = core::mem::replace(&mut self.alto_base, definido);
        let (alto_contenido, abierto, primera_base) = if e.display == Display::Flex {
            let (alto, base) = self.flex(id, e, (cx, cy), ancho_contenido, definido);
            (alto, 0.0, base)
        } else {
            self.contenido(id, e, cx, cy, ancho_contenido, colapsa_arriba)
        };
        (self.enlace, self.alto_base) = (enlace_previo, base_previa);
        let colapsa_abajo = bb + pb == 0.0 && e.alto == Medida::Auto && !propio;
        let mut alto_contenido = match definido.filter(|_| forzado.is_none() && e.alto != Medida::Auto) {
            Some(h) => h,
            None => alto_contenido + if colapsa_abajo { 0.0 } else { abierto },
        };
        alto_contenido = alto_contenido.max(e.alto_min.px(0.0, 0.0));
        if let Some(fuera) = contexto {
            // Un contexto de formato crece hasta abarcar sus flotantes.
            if let (Some(abajo), Medida::Auto) = (self.fondo_flotantes(), e.alto) {
                alto_contenido = alto_contenido.max(abajo - cy);
            }
            self.flotantes = fuera;
        }
        let alto = forzado.unwrap_or(bt + pt + alto_contenido + pb + bb);
        self.cajas.push((id, [x + ml, y, ancho_borde, alto]));
        self.caja(inicio_piezas, e, (x + ml, y, ancho_borde, alto), impuestos.is_none());
        if e.display == Display::ElementoDeLista {
            self.marcador(id, e, cx, primera_base.unwrap_or(cy + e.tam_letra * 0.8));
        }
        if propio {
            let capa: Vec<Pieza> = self.capa_flotantes.drain(capa_inicio..).collect();
            self.piezas.extend(capa);
        }
        let propio = px(e.margen[2]);
        let margen_inferior = if colapsa_abajo { mayor_margen(propio, abierto) } else { propio };
        Hecho { alto, margen_inferior, primera_base, caja: (x + ml, ancho_borde) }
    }

    /// El alto de contenido definido de un bloque: el impuesto (`forzado`, con borde), su `height`
    /// en píxeles o un porcentaje del alto definido de su contenedor.
    pub(crate) fn alto_definido(&self, e: &Estilo, forzado: Option<f32>, verticales: f32) -> Option<f32> {
        match (forzado, e.alto) {
            (Some(f), _) => Some((f - verticales).max(0.0)),
            (None, Medida::Px(h)) => Some(h),
            (None, Medida::Porcentaje(p)) => {
                let resta = if e.caja_borde { verticales } else { 0.0 };
                self.alto_base.map(|b| (b * p / 100.0 - resta).max(0.0))
            }
            _ => None,
        }
    }

    /// El fondo y los bordes de una caja, debajo de su contenido. Sin `bordes` (una celda con
    /// bordes que colapsan, que los dibuja su tabla), solo el fondo.
    fn caja(&mut self, en: usize, e: &Estilo, (x, y, ancho, alto): (f32, f32, f32, f32), bordes: bool) {
        if bordes {
            self.fondo_y_bordes(en, e, x, y, ancho, alto);
        } else if e.visible && e.fondo.a > 0 {
            self.piezas.insert(en, Pieza::Rect { x, y, ancho, alto, color: e.fondo });
        }
    }

    /// Si `id` es su propio contexto de formato: la raíz, lo que se maqueta con su ancho dado
    /// (celdas, elementos flexibles, flotantes), `display: flow-root` y `overflow` que no es
    /// `visible`. Abarca sus flotantes y sus márgenes no se juntan con los de sus hijos.
    pub(crate) fn contexto_propio(&self, id: NodoId, e: &Estilo) -> bool {
        self.raiz == Some(id) || self.ancho_libre == Some(id) || e.display == Display::RaizFlujo || e.recorta
    }

    /// Si el padre de `id` centra sus bloques (`text-align: -webkit-center`).
    pub(crate) fn padre_centra(&self, id: NodoId) -> bool {
        self.doc.nodo(id).padre.and_then(|p| self.estilo(p)).is_some_and(|e| e.centrar_bloques)
    }

    /// Márgenes izquierdo y derecho: `auto` centra si el ancho está fijado.
    fn margenes_horizontales(&self, e: &Estilo, contenedor: f32, extras: f32) -> (f32, f32) {
        let ml = e.margen[3].px(contenedor, 0.0);
        let mr = e.margen[1].px(contenedor, 0.0);
        let mut ancho = match e.ancho {
            Medida::Auto => return self.limitar(e, contenedor, extras, ml, mr),
            Medida::Porcentaje(_) => {
                (e.ancho.px(contenedor, 0.0) - e.extras_si_caja_borde(contenedor)).max(0.0) + extras
            }
            m => m.px(contenedor, 0.0) + extras,
        };
        if let Medida::Px(_) | Medida::Porcentaje(_) = e.ancho_max {
            let resta = if let Medida::Porcentaje(_) = e.ancho_max { e.extras_si_caja_borde(contenedor) } else { 0.0 };
            ancho = ancho.min(e.ancho_max.px(contenedor, f32::MAX) - resta + extras);
        }
        repartir(e, contenedor, ancho, ml, mr)
    }

    /// Con `width: auto`: todo el contenedor, salvo que `max-width` lo limite.
    fn limitar(&self, e: &Estilo, contenedor: f32, extras: f32, ml: f32, mr: f32) -> (f32, f32) {
        let disponible = contenedor - ml - mr;
        match e.ancho_max {
            Medida::Auto => (ml, mr),
            m => {
                let maximo = m.px(contenedor, f32::MAX) + extras;
                if disponible <= maximo { (ml, mr) } else { repartir(e, contenedor, maximo, ml, mr) }
            }
        }
    }

    /// Los hijos uno debajo de otro. Devuelve (alto, margen inferior abierto, primera base).
    fn contenido(
        &mut self,
        id: NodoId,
        e: &'a Estilo,
        cx: f32,
        cy: f32,
        ancho: f32,
        colapsa_arriba: bool,
    ) -> (f32, f32, Option<f32>) {
        let hijos = self.doc.nodo(id).hijos.clone();
        let ancho_previo = core::mem::replace(&mut self.ancho_actual, ancho);
        let mut cursor = cy;
        let mut abierto: Option<f32> = None; // margen inferior del último bloque, aún sin poner
        let mut primero = true;
        let mut primera_base = None;
        let mut items: Vec<Item<'a>> = Vec::new();
        let mut ultimo_espacio = true;
        for h in hijos {
            match self.clase(h) {
                Clase::Nada => {}
                Clase::EnLinea => self.recoger(h, e, &mut items, &mut ultimo_espacio),
                // Un flotante entre texto va arriba de esas líneas; si no, donde va el flujo.
                Clase::Flotante if items_con_contenido(&items) => self.pendientes.push(h),
                Clase::Flotante => self.flotante(h, cx, cursor + abierto.unwrap_or(0.0), ancho),
                Clase::Absoluto => self.absolutos.push(Pendiente {
                    id: h,
                    estatico: (cx, cursor + abierto.unwrap_or(0.0)),
                    apilado: self.apilado.clone(),
                }),
                Clase::Bloque => {
                    if items_con_contenido(&items) {
                        cursor += abierto.take().unwrap_or(0.0);
                        self.poner_pendientes(cx, cursor, ancho);
                        let (alto, base) = self.lineas(&items, e, cx, cursor, ancho);
                        primera_base = primera_base.or(base);
                        cursor += alto;
                        primero = false;
                    }
                    self.poner_pendientes(cx, cursor + abierto.unwrap_or(0.0), ancho);
                    items.clear();
                    ultimo_espacio = true;
                    let mt = self.margen_superior(h, ancho);
                    let mt = if primero && colapsa_arriba { 0.0 } else { mt };
                    let arriba = cursor + abierto.map_or(mt, |a| mayor_margen(a, mt));
                    let arriba = self.despejar(h, arriba);
                    let hecho = self.bloque_junto(h, cx, arriba, ancho);
                    primera_base = primera_base.or(hecho.primera_base);
                    cursor = arriba + hecho.alto;
                    abierto = Some(hecho.margen_inferior);
                    primero = false;
                }
            }
        }
        if items_con_contenido(&items) {
            cursor += abierto.take().unwrap_or(0.0);
            self.poner_pendientes(cx, cursor, ancho);
            let (alto, base) = self.lineas(&items, e, cx, cursor, ancho);
            primera_base = primera_base.or(base);
            cursor += alto;
        }
        self.poner_pendientes(cx, cursor + abierto.unwrap_or(0.0), ancho);
        self.ancho_actual = ancho_previo;
        (cursor - cy, abierto.unwrap_or(0.0), primera_base)
    }

    pub(crate) fn lineas(
        &mut self,
        items: &[Item<'a>],
        e: &'a Estilo,
        x: f32,
        y: f32,
        ancho: f32,
    ) -> (f32, Option<f32>) {
        let l = EnLinea { medir: self.medir, bloque: e, x, y, ancho, flotantes: &self.flotantes };
        l.maquetar(items, &mut self.piezas, &mut self.enlaces, (&mut self.capas, &mut self.cajas))
    }

    /// El contenido en línea de `id` (un texto o un elemento en línea) como elementos.
    pub(crate) fn recoger(
        &mut self,
        id: NodoId,
        padre: &'a Estilo,
        items: &mut Vec<Item<'a>>,
        ultimo_espacio: &mut bool,
    ) {
        match &self.doc.nodo(id).dato {
            Dato::Texto(t) => texto_a_items(t, padre, self.enlace, ultimo_espacio, items),
            Dato::Elemento(el) => {
                let Some(e) = self.estilo(id) else { return };
                if e.display == Display::Ninguno {
                    return;
                }
                if e.flotar != Flotar::No {
                    self.pendientes.push(id);
                    return;
                }
                if Self::fuera_del_flujo(e) {
                    let estatico = (f32::NAN, f32::NAN);
                    self.absolutos.push(Pendiente { id, estatico, apilado: self.apilado.clone() });
                    return;
                }
                // Una caja en línea (y los controles): se maqueta aparte y va como una palabra.
                let control = crate::control::tipo_control(self, id).is_some();
                let reemplazado = matches!(el.nombre.as_str(), "img" | "svg" | "video");
                if (e.display == Display::BloqueEnLinea || control) && !reemplazado {
                    items.push(self.caja_en_linea(id, e));
                    *ultimo_espacio = false;
                    return;
                }
                match el.nombre.as_str() {
                    "br" => {
                        items.push(Item::Salto { estilo: e });
                        *ultimo_espacio = true;
                    }
                    "img" | "svg" | "video" => {
                        let contenedor = if self.midiendo { 0.0 } else { self.ancho_actual };
                        let (ancho, alto) = self.tam_imagen(id, e, contenedor);
                        let src = self.src_imagen(id, e, (ancho, alto));
                        // Sin área (una imagen que aún no cargó) también ocupa su sitio y tiene caja.
                        if ancho > 0.0 || alto > 0.0 {
                            items.push(Item::Imagen { ancho, alto, src, estilo: e, enlace: self.enlace, nodo: id });
                            *ultimo_espacio = false;
                        }
                    }
                    _ => {
                        let previo = self.entrar_en_enlace(id);
                        for h in self.doc.nodo(id).hijos.clone() {
                            self.recoger(h, e, items, ultimo_espacio);
                        }
                        self.enlace = previo;
                    }
                }
            }
            _ => {}
        }
    }

    /// Si `id` es un `<a href>`, lo marca como el enlace actual; devuelve el que había.
    fn entrar_en_enlace(&mut self, id: NodoId) -> Option<u32> {
        let previo = self.enlace;
        if let Some(href) = self.doc.elemento(id).filter(|e| e.nombre == "a").and_then(|e| e.atributo("href")) {
            self.hrefs.push((String::from(href), id));
            self.enlace = Some(self.hrefs.len() as u32 - 1);
        }
        previo
    }

    /// El fondo y los bordes van debajo del contenido: se insertan antes de sus piezas.
    pub(crate) fn fondo_y_bordes(&mut self, en: usize, e: &Estilo, x: f32, y: f32, ancho: f32, alto: f32) {
        if !e.visible {
            return;
        }
        let mut debajo = Vec::new();
        if e.fondo.a > 0 {
            debajo.push(Pieza::Rect { x, y, ancho, alto, color: e.fondo });
        }
        let b = |i: usize| e.borde[i].ocupa();
        let lados = [
            (x, y, ancho, b(0)),
            (x + ancho - b(1), y, b(1), alto),
            (x, y + alto - b(2), ancho, b(2)),
            (x, y, b(3), alto),
        ];
        for (i, (bx, by, bw, bh)) in lados.into_iter().enumerate() {
            if bw > 0.0 && bh > 0.0 {
                debajo.push(Pieza::Rect { x: bx, y: by, ancho: bw, alto: bh, color: e.color_borde(i) });
            }
        }
        self.piezas.splice(en..en, debajo);
    }

    /// El marcador de un `<li>` (`•`, `1.`...), fuera del contenido, a la izquierda.
    fn marcador(&mut self, id: NodoId, e: &Estilo, cx: f32, base: f32) {
        let posicion = self.posicion_en_lista(id);
        let texto = match e.lista {
            TipoLista::Ninguno => return,
            TipoLista::Disco => String::from("• "),
            TipoLista::Circulo => String::from("◦ "),
            TipoLista::Cuadrado => String::from("▪ "),
            TipoLista::Decimal => alloc::format!("{posicion}. "),
            TipoLista::Letra => alloc::format!("{}. ", letra(posicion, b'a')),
            TipoLista::LetraMayuscula => alloc::format!("{}. ", letra(posicion, b'A')),
            TipoLista::Romano => alloc::format!("{}. ", romano(posicion)),
        };
        let f = crate::linea::fuente(e);
        let ancho = self.medir.ancho(&texto, &f);
        self.piezas.push(Pieza::Texto {
            x: cx - ancho,
            base,
            ancho,
            texto,
            tam: e.tam_letra,
            color: e.color,
            negrita: e.negrita(),
            cursiva: e.cursiva,
            generica: e.generica,
            familia: self.medir.familia(&f).cloned(),
        });
    }

    /// El número de un `<li>` en su lista (contando el `start` de un `<ol>`).
    fn posicion_en_lista(&self, id: NodoId) -> i64 {
        let Some(padre) = self.doc.nodo(id).padre else { return 1 };
        let inicio: i64 =
            self.doc.elemento(padre).and_then(|p| p.atributo("start")).and_then(|s| s.trim().parse().ok()).unwrap_or(1);
        let antes = self
            .doc
            .nodo(padre)
            .hijos
            .iter()
            .take_while(|&&h| h != id)
            .filter(|&&h| self.estilos.de(h).is_some_and(|e| e.display == Display::ElementoDeLista));
        inicio + antes.count() as i64
    }
}

/// Dos márgenes que colapsan: el mayor de los positivos más el menor de los negativos.
fn mayor_margen(a: f32, b: f32) -> f32 {
    a.max(0.0).max(b.max(0.0)) + a.min(0.0).min(b.min(0.0))
}

/// Con un ancho fijado, lo que sobra va a los márgenes `auto` (los dos: centrado).
fn repartir(e: &Estilo, contenedor: f32, ancho_borde: f32, ml: f32, mr: f32) -> (f32, f32) {
    let sobra = contenedor - ancho_borde;
    match (e.margen[3] == Medida::Auto, e.margen[1] == Medida::Auto) {
        (true, true) => (sobra.max(0.0) / 2.0, sobra.max(0.0) / 2.0),
        (true, false) => (sobra - mr, mr),
        _ => (ml, sobra - ml),
    }
}

/// Si hay algo que no sean espacios sueltos.
fn items_con_contenido(items: &[Item<'_>]) -> bool {
    items.iter().any(|i| !matches!(i, Item::Espacio { fijo: false, .. }))
}

fn letra(n: i64, base: u8) -> char {
    char::from(base + ((n - 1).rem_euclid(26)) as u8)
}

fn romano(mut n: i64) -> String {
    let mut s = String::new();
    for (v, r) in [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}
