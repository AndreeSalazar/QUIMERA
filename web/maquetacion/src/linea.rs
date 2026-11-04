//! Contenido en línea: texto e imágenes partidos en líneas.
//!
//! 1. El contenido en línea de un bloque se convierte en una lista de elementos (palabras,
//!    espacios, saltos, imágenes), aplicando las reglas de `white-space`: en modo normal los
//!    blancos seguidos se juntan en uno y no hay blancos al principio de línea.
//! 2. Se parte en líneas de forma voraz: cabe todo lo que cabe. Cada línea tiene el ancho que
//!    le dejan los flotantes a su altura; si no cabe ni su primera palabra, baja.
//! 3. Cada línea mide lo que su elemento más alto (como el *strut* de CSS, el propio bloque
//!    cuenta siempre), se apoya en una línea base común y se alinea con `text-align`.

use crate::flotante::{Flotante, bajar, espacio};
use crate::pintura::Pieza;
use crate::posicion::Capa;
use crate::texto::{Fuente, MedirTexto};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_estilo::{Alinear, Espacios, Estilo};
use quimera_html::NodoId;

/// Un elemento del contenido en línea.
pub(crate) enum Item<'e> {
    Palabra {
        texto: String,
        estilo: &'e Estilo,
        enlace: Option<u32>,
    },
    /// Un espacio que se puede juntar o quitar (modo normal) o uno que se respeta (`pre`).
    Espacio {
        texto: String,
        estilo: &'e Estilo,
        fijo: bool,
        enlace: Option<u32>,
    },
    Salto {
        estilo: &'e Estilo,
    },
    Imagen {
        ancho: f32,
        alto: f32,
        src: String,
        estilo: &'e Estilo,
        enlace: Option<u32>,
        /// El `<img>` o `<svg>` (para su caja en las medidas del DOM).
        nodo: NodoId,
    },
    /// Una caja de bloque en la línea (`inline-block`, un control): sus piezas ya maquetadas con
    /// su esquina en (0, 0) y su línea base a `base` de arriba.
    Caja {
        ancho: f32,
        alto: f32,
        base: f32,
        piezas: Vec<Pieza>,
        enlaces: Vec<EnlaceBruto>,
        /// Las capas de lo posicionado de dentro y las cajas para las medidas: se mueven con ella.
        capas: Vec<Capa>,
        cajas: Vec<(NodoId, [f32; 4])>,
        /// Las anclas (`id`) de dentro: también se mueven con ella.
        anclas: Vec<(String, f32)>,
        estilo: &'e Estilo,
    },
}

/// Las capas y las cajas de las medidas, adonde van las de los `inline-block` al colocarlos.
pub(crate) type Extra<'a> = (&'a mut Vec<Capa>, &'a mut Vec<(NodoId, [f32; 4])>, &'a mut Vec<(String, f32)>);

/// Una línea con dónde empieza la próxima, su espacio (izquierda, derecha) y su alto (sobre, bajo).
type LineaPuesta = (Linea, usize, (f32, f32), (f32, f32));

/// Un enlace dibujado: su rectángulo y el número de su `href` en la lista del maquetador.
pub(crate) type EnlaceBruto = (f32, f32, f32, f32, u32);

impl<'e> Item<'e> {
    fn estilo(&self) -> &'e Estilo {
        match self {
            Item::Palabra { estilo, .. }
            | Item::Espacio { estilo, .. }
            | Item::Salto { estilo }
            | Item::Imagen { estilo, .. }
            | Item::Caja { estilo, .. } => estilo,
        }
    }

    /// El enlace (`<a href>`) dentro del que está, si lo hay.
    fn enlace(&self) -> Option<u32> {
        match self {
            Item::Palabra { enlace, .. } | Item::Espacio { enlace, .. } | Item::Imagen { enlace, .. } => *enlace,
            Item::Salto { .. } | Item::Caja { .. } => None,
        }
    }
}

pub(crate) fn fuente(e: &Estilo) -> Fuente<'_> {
    Fuente { tam: e.tam_letra, negrita: e.negrita(), cursiva: e.cursiva, generica: e.generica, familias: &e.familias }
}

fn parte_lineas(e: &Estilo) -> bool {
    matches!(e.espacios, Espacios::Normal | Espacios::PreAjustado | Espacios::PreLinea)
}

/// Junta los elementos de un texto. `ultimo_espacio`: si lo anterior acabó en blanco (para no
/// repetirlo en modo normal); empieza en `true` al principio del bloque.
pub(crate) fn texto_a_items<'e>(
    texto: &str,
    estilo: &'e Estilo,
    enlace: Option<u32>,
    ultimo_espacio: &mut bool,
    v: &mut Vec<Item<'e>>,
) {
    let texto: String = match estilo.transformar {
        Some(true) => texto.to_uppercase(),
        Some(false) => texto.to_lowercase(),
        None => String::from(texto),
    };
    let respeta = matches!(estilo.espacios, Espacios::Pre | Espacios::PreAjustado);
    let respeta_saltos = respeta || estilo.espacios == Espacios::PreLinea;
    let mut palabra = String::new();
    let soltar = |p: &mut String, v: &mut Vec<Item<'e>>| {
        if !p.is_empty() {
            v.push(Item::Palabra { texto: core::mem::take(p), estilo, enlace });
        }
    };
    for c in texto.chars() {
        match c {
            '\n' if respeta_saltos => {
                soltar(&mut palabra, v);
                v.push(Item::Salto { estilo });
                *ultimo_espacio = true;
            }
            ' ' | '\t' | '\n' | '\r' | '\u{c}' if respeta => {
                soltar(&mut palabra, v);
                let texto = if c == '\t' { String::from("        ") } else { String::from(" ") };
                v.push(Item::Espacio { texto, estilo, fijo: true, enlace });
            }
            ' ' | '\t' | '\n' | '\r' | '\u{c}' => {
                soltar(&mut palabra, v);
                if !*ultimo_espacio {
                    v.push(Item::Espacio { texto: String::from(" "), estilo, fijo: false, enlace });
                    *ultimo_espacio = true;
                }
            }
            c => {
                palabra.push(c);
                *ultimo_espacio = false;
            }
        }
    }
    soltar(&mut palabra, v);
}

/// Una línea: (índice del elemento, x, ancho) de lo que lleva, y su ancho total.
#[derive(Default)]
struct Linea {
    puestos: Vec<(usize, f32, f32)>,
    ancho: f32,
    /// El salto que la terminó (para el alto de una línea vacía).
    salto: Option<usize>,
}

pub(crate) fn ancho_item(m: &dyn MedirTexto, it: &Item<'_>) -> f32 {
    match it {
        Item::Palabra { texto, estilo, .. } | Item::Espacio { texto, estilo, .. } => m.ancho(texto, &fuente(estilo)),
        Item::Imagen { ancho, .. } | Item::Caja { ancho, .. } => *ancho,
        Item::Salto { .. } => 0.0,
    }
}

/// La siguiente línea desde el elemento `desde`, de `ancho` píxeles como mucho (salvo palabras
/// más largas). Devuelve la línea y dónde empieza la próxima (`None`: no queda nada).
fn siguiente(m: &dyn MedirTexto, items: &[Item<'_>], desde: usize, ancho: f32) -> Option<(Linea, usize)> {
    let mut l = Linea::default();
    let mut espacio: Option<(usize, f32)> = None;
    for (i, it) in items.iter().enumerate().skip(desde) {
        match it {
            Item::Espacio { fijo: false, .. } => {
                if !l.puestos.is_empty() {
                    espacio = Some((i, ancho_item(m, it)));
                }
            }
            Item::Salto { .. } => {
                l.salto = Some(i);
                return Some((l, i + 1));
            }
            _ => {
                let w = ancho_item(m, it);
                let sp = espacio.map_or(0.0, |(_, w)| w);
                if parte_lineas(it.estilo()) && !l.puestos.is_empty() && l.ancho + sp + w > ancho + 0.01 {
                    return Some((l, i));
                }
                if let Some((j, ws)) = espacio.take() {
                    l.puestos.push((j, l.ancho, ws));
                    l.ancho += ws;
                }
                l.puestos.push((i, l.ancho, w));
                l.ancho += w;
            }
        }
    }
    (!l.puestos.is_empty()).then_some((l, items.len()))
}

/// Lo que sube y baja sobre la línea base algo con este estilo (con su medio interlineado).
fn medidas(m: &dyn MedirTexto, e: &Estilo) -> (f32, f32) {
    let f = fuente(e);
    let (asc, desc) = (m.ascenso(&f), m.descenso(&f));
    let lh = e.alto_linea_px();
    let medio = (lh - asc - desc) / 2.0;
    (medio + asc, lh - medio - asc)
}

/// Maquetación de un contenido en línea dentro de un bloque.
pub(crate) struct EnLinea<'a> {
    pub medir: &'a dyn MedirTexto,
    /// El estilo del bloque (alineación y alto mínimo de cada línea).
    pub bloque: &'a Estilo,
    /// Esquina superior izquierda y ancho disponible.
    pub x: f32,
    pub y: f32,
    pub ancho: f32,
    /// Los flotantes del contexto: acortan las líneas que pasan a su lado.
    pub flotantes: &'a [Flotante],
}

impl EnLinea<'_> {
    /// Coloca los elementos y devuelve (alto total, línea base de la primera línea).
    pub(crate) fn maquetar(
        &self,
        items: &[Item<'_>],
        piezas: &mut Vec<Pieza>,
        enlaces: &mut Vec<EnlaceBruto>,
        extra: Extra<'_>,
    ) -> (f32, Option<f32>) {
        let mut y = self.y;
        let mut primera = None;
        let mut desde = 0;
        while let Some((l, proxima, (izq, der), (sobre, bajo))) = self.linea_en(items, desde, y) {
            // Si junto a un flotante no cabe ni la primera palabra, la línea baja.
            let estrecha = izq > self.x || der < self.x + self.ancho;
            if estrecha
                && l.ancho > der - izq + 0.01
                && let Some(abajo) = bajar(self.flotantes, y, (sobre + bajo).max(0.01))
            {
                y = abajo;
                continue;
            }
            desde = proxima;
            let base = y + sobre;
            primera.get_or_insert(base);
            let libre = (der - izq - l.ancho).max(0.0);
            let dx = match self.bloque.alinear {
                Alinear::Centro => libre / 2.0,
                Alinear::Derecha => libre,
                _ => 0.0,
            };
            self.dibujar(
                items,
                &l.puestos,
                (izq + dx, base),
                (piezas, enlaces),
                (&mut *extra.0, &mut *extra.1, &mut *extra.2),
            );
            y += sobre + bajo;
        }
        (y - self.y, primera)
    }

    /// La línea que empieza en el elemento `desde` a la altura `y`, con el espacio que le dejan
    /// los flotantes en toda su altura: (línea, próxima, (izquierda, derecha), (sobre, bajo)).
    /// El alto depende de lo que entra y lo que entra del alto: se ajusta hasta que no cambia.
    fn linea_en(&self, items: &[Item<'_>], desde: usize, y: f32) -> Option<LineaPuesta> {
        let (s0, b0) = medidas(self.medir, self.bloque);
        let mut alto = (s0 + b0).max(0.01);
        let mut resultado = None;
        for _ in 0..4 {
            let lados = espacio(self.flotantes, y, alto, self.x, self.x + self.ancho);
            let (l, proxima) = siguiente(self.medir, items, desde, lados.1 - lados.0)?;
            let (sobre, bajo) = self.alto_de(items, &l);
            let nuevo = (sobre + bajo).max(0.01);
            let otra = espacio(self.flotantes, y, nuevo, self.x, self.x + self.ancho);
            resultado = Some((l, proxima, lados, (sobre, bajo)));
            if otra == lados {
                break;
            }
            alto = nuevo;
        }
        resultado
    }

    /// Lo que sube y baja la línea sobre su base (el bloque cuenta siempre, como el *strut*).
    fn alto_de(&self, items: &[Item<'_>], l: &Linea) -> (f32, f32) {
        let (mut sobre, mut bajo) = medidas(self.medir, self.bloque);
        for &(i, _, _) in &l.puestos {
            let (s, b) = match &items[i] {
                Item::Imagen { alto, .. } => (*alto, 0.0),
                Item::Caja { alto, base, .. } => (*base, (alto - base).max(0.0)),
                it => medidas(self.medir, it.estilo()),
            };
            sobre = sobre.max(s);
            bajo = bajo.max(b);
        }
        if let Some(s) = l.salto.filter(|_| l.puestos.is_empty()) {
            let (s2, b2) = medidas(self.medir, items[s].estilo());
            sobre = sobre.max(s2);
            bajo = bajo.max(b2);
        }
        (sobre, bajo)
    }

    /// Las piezas de una línea: los trozos seguidos con el mismo estilo van en un solo texto.
    fn dibujar(
        &self,
        items: &[Item<'_>],
        puestos: &[(usize, f32, f32)],
        (x0, base): (f32, f32),
        (piezas, enlaces): (&mut Vec<Pieza>, &mut Vec<EnlaceBruto>),
        (capas, cajas, anclas): Extra<'_>,
    ) {
        let mut k = 0;
        while k < puestos.len() {
            let (i, x, _) = puestos[k];
            if let Item::Caja {
                base: b,
                piezas: suyas,
                enlaces: suyos,
                capas: sus_capas,
                cajas: sus_cajas,
                anclas: sus_anclas,
                ..
            } = &items[i]
            {
                // La caja ya está maquetada en (0, 0): se mueve a su sitio en la línea.
                let (dx, dy) = (x0 + x, base - b);
                piezas.extend(suyas.iter().cloned().map(|mut p| {
                    p.mover(dx, dy);
                    p
                }));
                capas.extend(sus_capas.iter().cloned().map(|mut c| {
                    c.piezas.iter_mut().for_each(|p| p.mover(dx, dy));
                    c
                }));
                cajas.extend(sus_cajas.iter().map(|&(id, c)| (id, [c[0] + dx, c[1] + dy, c[2], c[3]])));
                anclas.extend(sus_anclas.iter().map(|(id, y)| (id.clone(), y + dy)));
                enlaces.extend(suyos.iter().map(|&(ex, ey, w, h, n)| (ex + dx, ey + dy, w, h, n)));
                k += 1;
                continue;
            }
            if let Item::Imagen { ancho, alto, src, estilo, enlace, nodo } = &items[i] {
                cajas.push((*nodo, [x0 + x, base - alto, *ancho, *alto]));
                if let Some(e) = enlace {
                    enlaces.push((x0 + x, base - alto, *ancho, *alto, *e));
                }
                if estilo.visible && *ancho > 0.0 && *alto > 0.0 {
                    piezas.push(Pieza::Imagen {
                        x: x0 + x,
                        y: base - alto,
                        ancho: *ancho,
                        alto: *alto,
                        src: src.clone(),
                    });
                }
                k += 1;
                continue;
            }
            let (estilo, enlace) = (items[i].estilo(), items[i].enlace());
            let mut texto = String::new();
            let mut ancho = 0.0;
            while k < puestos.len()
                && !matches!(items[puestos[k].0], Item::Imagen { .. } | Item::Caja { .. })
                && core::ptr::eq(items[puestos[k].0].estilo(), estilo)
                && items[puestos[k].0].enlace() == enlace
            {
                if let Item::Palabra { texto: t, .. } | Item::Espacio { texto: t, .. } = &items[puestos[k].0] {
                    texto.push_str(t);
                }
                ancho += puestos[k].2;
                k += 1;
            }
            if let Some(e) = enlace {
                let f = fuente(estilo);
                let (asc, desc) = (self.medir.ascenso(&f), self.medir.descenso(&f));
                enlaces.push((x0 + x, base - asc, ancho, asc + desc, e));
            }
            if estilo.visible {
                self.trozo(estilo, x0 + x, base, ancho, texto, piezas);
            }
        }
    }

    fn trozo(&self, e: &Estilo, x: f32, base: f32, ancho: f32, texto: String, piezas: &mut Vec<Pieza>) {
        let f = fuente(e);
        let (asc, desc) = (self.medir.ascenso(&f), self.medir.descenso(&f));
        // El texto que es del propio bloque no pinta fondo (ya lo pintó el bloque); solo los
        // elementos en línea (`<span>`, `<mark>`...) pintan el suyo detrás de su texto.
        if e.fondo.a > 0 && !core::ptr::eq(e, self.bloque) {
            piezas.push(Pieza::Rect { x, y: base - asc, ancho, alto: asc + desc, color: e.fondo });
        }
        piezas.push(Pieza::Texto {
            x,
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
        let grosor = (e.tam_letra / 14.0).max(1.0);
        if e.subrayado {
            piezas.push(Pieza::Rect { x, y: base + e.tam_letra * 0.1, ancho, alto: grosor, color: e.color });
        }
        if e.tachado {
            piezas.push(Pieza::Rect { x, y: base - e.tam_letra * 0.3, ancho, alto: grosor, color: e.color });
        }
    }
}
