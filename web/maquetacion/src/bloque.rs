//! Bloques: cada uno ocupa el ancho de su contenedor (o el suyo), apila a sus hijos de arriba
//! abajo y sus márgenes verticales colapsan como en CSS:
//! - entre hermanos, cuenta el mayor de los dos márgenes (no la suma);
//! - el margen superior del primer hijo sale por arriba de su padre si este no tiene borde ni
//!   relleno arriba (y el inferior del último, por abajo, si además su alto es automático).
//!
//! Los hijos en línea seguidos (texto, `<b>`, `<a>`, `<img>`) forman un bloque anónimo que se
//! parte en líneas (`linea.rs`).

use crate::linea::{EnLinea, Item, texto_a_items};
use crate::pintura::Pieza;
use crate::texto::MedirTexto;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_estilo::{Display, Estilo, Estilos, Medida, TipoLista};
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
}

/// Lo que un bloque maquetado devuelve a su padre.
pub(crate) struct Hecho {
    /// Alto de la caja con su borde.
    pub alto: f32,
    /// Margen inferior que sigue «abierto» para colapsar con lo que venga después.
    pub margen_inferior: f32,
    /// Línea base de su primera línea de texto (para el marcador de una lista).
    pub primera_base: Option<f32>,
}

/// Qué es un hijo para su bloque.
enum Clase {
    Bloque,
    EnLinea,
    Nada,
}

impl<'a> Maquetador<'a> {
    pub(crate) fn nuevo(doc: &'a Documento, estilos: &'a Estilos, medir: &'a dyn MedirTexto) -> Maquetador<'a> {
        Maquetador {
            doc,
            estilos,
            medir,
            piezas: Vec::new(),
            contiene_bloque: alloc::vec![None; doc.total()],
            raiz: None,
        }
    }

    fn estilo(&self, id: NodoId) -> Option<&'a Estilo> {
        self.estilos.de(id)
    }

    fn clase(&mut self, id: NodoId) -> Clase {
        match &self.doc.nodo(id).dato {
            Dato::Texto(_) => return Clase::EnLinea,
            Dato::Elemento(_) => {}
            _ => return Clase::Nada,
        }
        match self.estilo(id).map(|e| e.display) {
            None | Some(Display::Ninguno) => Clase::Nada,
            Some(Display::Bloque | Display::ElementoDeLista) => Clase::Bloque,
            // Un elemento en línea con bloques dentro se maqueta como un bloque (los
            // navegadores lo parten en trozos; el resultado visible es casi el mismo).
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
        if e.borde[0].ocupa() > 0.0 || e.relleno[0].px(ancho_contenedor, 0.0) > 0.0 {
            return propio;
        }
        let primero = self
            .doc
            .nodo(id)
            .hijos
            .clone()
            .into_iter()
            .find(|&h| !self.es_blanco(h) && !matches!(self.clase(h), Clase::Nada));
        match primero {
            Some(h) if matches!(self.clase(h), Clase::Bloque) => {
                mayor_margen(propio, self.margen_superior(h, ancho_contenedor))
            }
            _ => propio,
        }
    }

    /// Un texto de solo blancos en modo normal (no crea líneas entre bloques).
    fn es_blanco(&self, id: NodoId) -> bool {
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

    /// Maquetar el bloque `id` con su borde empezando en (x, y) dentro de un contenedor de
    /// `ancho_contenedor`. Su margen superior ya lo ha puesto quien llama.
    pub(crate) fn bloque(&mut self, id: NodoId, x: f32, y: f32, ancho_contenedor: f32) -> Hecho {
        let Some(e) = self.estilo(id) else { return Hecho { alto: 0.0, margen_inferior: 0.0, primera_base: None } };
        let px = |m: Medida| m.px(ancho_contenedor, 0.0);
        let (bt, br, bb, bl) = (e.borde[0].ocupa(), e.borde[1].ocupa(), e.borde[2].ocupa(), e.borde[3].ocupa());
        let (pt, pr, pb, pl) = (px(e.relleno[0]), px(e.relleno[1]), px(e.relleno[2]), px(e.relleno[3]));
        let (ml, mr) = self.margenes_horizontales(e, ancho_contenedor, bl + br + pl + pr);
        let ancho_borde = (ancho_contenedor - ml - mr).max(0.0);
        let ancho_contenido = (ancho_borde - bl - br - pl - pr).max(0.0);
        let (cx, cy) = (x + ml + bl + pl, y + bt + pt);
        let inicio_piezas = self.piezas.len();
        let es_raiz = self.raiz == Some(id);
        let colapsa_arriba = bt + pt == 0.0 && !es_raiz;
        let (alto_contenido, abierto, primera_base) = self.contenido(id, e, cx, cy, ancho_contenido, colapsa_arriba);
        let colapsa_abajo = bb + pb == 0.0 && e.alto == Medida::Auto && !es_raiz;
        let mut alto_contenido = match e.alto {
            Medida::Px(h) => h,
            _ => alto_contenido + if colapsa_abajo { 0.0 } else { abierto },
        };
        alto_contenido = alto_contenido.max(e.alto_min.px(0.0, 0.0));
        let alto = bt + pt + alto_contenido + pb + bb;
        self.fondo_y_bordes(inicio_piezas, e, x + ml, y, ancho_borde, alto);
        if e.display == Display::ElementoDeLista {
            self.marcador(id, e, cx, primera_base.unwrap_or(cy + e.tam_letra * 0.8));
        }
        let propio = px(e.margen[2]);
        let margen_inferior = if colapsa_abajo { mayor_margen(propio, abierto) } else { propio };
        Hecho { alto, margen_inferior, primera_base }
    }

    /// Márgenes izquierdo y derecho: `auto` centra si el ancho está fijado.
    fn margenes_horizontales(&self, e: &Estilo, contenedor: f32, extras: f32) -> (f32, f32) {
        let ml = e.margen[3].px(contenedor, 0.0);
        let mr = e.margen[1].px(contenedor, 0.0);
        let mut ancho = match e.ancho {
            Medida::Auto => return self.limitar(e, contenedor, extras, ml, mr),
            m => m.px(contenedor, 0.0) + extras,
        };
        if let Medida::Px(_) | Medida::Porcentaje(_) = e.ancho_max {
            ancho = ancho.min(e.ancho_max.px(contenedor, f32::MAX) + extras);
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
                Clase::Bloque => {
                    if items_con_contenido(&items) {
                        cursor += abierto.take().unwrap_or(0.0);
                        let (alto, base) = self.lineas(&items, e, cx, cursor, ancho);
                        primera_base = primera_base.or(base);
                        cursor += alto;
                        primero = false;
                    }
                    items.clear();
                    ultimo_espacio = true;
                    let mt = self.margen_superior(h, ancho);
                    let mt = if primero && colapsa_arriba { 0.0 } else { mt };
                    let arriba = cursor + abierto.map_or(mt, |a| mayor_margen(a, mt));
                    let hecho = self.bloque(h, cx, arriba, ancho);
                    primera_base = primera_base.or(hecho.primera_base);
                    cursor = arriba + hecho.alto;
                    abierto = Some(hecho.margen_inferior);
                    primero = false;
                }
            }
        }
        if items_con_contenido(&items) {
            cursor += abierto.take().unwrap_or(0.0);
            let (alto, base) = self.lineas(&items, e, cx, cursor, ancho);
            primera_base = primera_base.or(base);
            cursor += alto;
        }
        (cursor - cy, abierto.unwrap_or(0.0), primera_base)
    }

    fn lineas(&mut self, items: &[Item<'a>], e: &'a Estilo, x: f32, y: f32, ancho: f32) -> (f32, Option<f32>) {
        let l = EnLinea { medir: self.medir, bloque: e, x, y, ancho };
        l.maquetar(items, &mut self.piezas)
    }

    /// El contenido en línea de `id` (un texto o un elemento en línea) como elementos.
    fn recoger(&mut self, id: NodoId, padre: &'a Estilo, items: &mut Vec<Item<'a>>, ultimo_espacio: &mut bool) {
        match &self.doc.nodo(id).dato {
            Dato::Texto(t) => texto_a_items(t, padre, ultimo_espacio, items),
            Dato::Elemento(el) => {
                let Some(e) = self.estilo(id) else { return };
                if e.display == Display::Ninguno {
                    return;
                }
                match el.nombre.as_str() {
                    "br" => {
                        items.push(Item::Salto { estilo: e });
                        *ultimo_espacio = true;
                    }
                    "img" => {
                        let tam = |atributo: &str, m: Medida| match m {
                            Medida::Px(v) => Some(v),
                            _ => el.atributo(atributo).and_then(|v| v.trim_end_matches("px").parse().ok()),
                        };
                        let (ancho, alto) =
                            (tam("width", e.ancho).unwrap_or(0.0), tam("height", e.alto).unwrap_or(0.0));
                        let src = String::from(el.atributo("src").unwrap_or(""));
                        if ancho > 0.0 && alto > 0.0 {
                            items.push(Item::Imagen { ancho, alto, src, estilo: e });
                            *ultimo_espacio = false;
                        }
                    }
                    _ => {
                        for h in self.doc.nodo(id).hijos.clone() {
                            self.recoger(h, e, items, ultimo_espacio);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// El fondo y los bordes van debajo del contenido: se insertan antes de sus piezas.
    fn fondo_y_bordes(&mut self, en: usize, e: &Estilo, x: f32, y: f32, ancho: f32, alto: f32) {
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
            familia: e.familias.first().cloned(),
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
