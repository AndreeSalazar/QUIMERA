//! Controles de formulario (`<input>`, `<select>`, `<textarea>`) y cajas en línea (`inline-block`).
//!
//! - Una **caja en línea** se maqueta aparte, como un bloque con su esquina en (0, 0) y el ancho
//!   de su contenido (o el suyo), y entra en la línea como una palabra grande: con su ancho, su
//!   alto y su línea base (la de su primera línea; sin texto, su borde de abajo). Cuando la línea
//!   sabe dónde va, se mueven sus piezas allí.
//! - Un **control** no tiene hijos que maquetar: su contenido lo pone la maquetación. El texto
//!   escrito (o el de ayuda, en gris), la opción elegida, la marca de una casilla. Lo que ha
//!   escrito la persona lo sabe quien maqueta ([`Formulario`]); si no, se usa el atributo `value`.
//! - Cada control deja su rectángulo para que la ventana sepa dónde se pulsa (`controles`).

use crate::bloque::{Hecho, Maquetador};
use crate::intrinseco::extras_horizontales;
use crate::linea::{EnlaceBruto, Item, fuente};
use crate::pintura::Pieza;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_estilo::{Color, Estilo, Medida};
use quimera_html::NodoId;

/// Lo que sabe la ventana de los controles: lo escrito, lo marcado y cuál tiene el foco.
pub trait Formulario {
    /// Lo escrito en un campo de texto (o la opción elegida en una lista), si cambió.
    fn valor(&self, _control: NodoId) -> Option<String> {
        None
    }
    /// Si una casilla está marcada, si se cambió.
    fn marcado(&self, _control: NodoId) -> Option<bool> {
        None
    }
    /// El control con el foco (en él se dibuja el cursor).
    fn foco(&self) -> Option<NodoId> {
        None
    }
}

/// Sin nada escrito: cada control muestra lo que dice su HTML.
pub struct SinFormulario;

impl Formulario for SinFormulario {}

/// Marca que distingue el rectángulo de un control de los de los enlaces (en la misma lista).
pub(crate) const CONTROL: u32 = 1 << 31;

/// Qué control es un elemento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TipoControl {
    /// `<input>` de texto (y `search`, `email`, `password`, `number`...).
    Texto,
    /// `<input type=submit | button | reset | image>`.
    Boton,
    /// Casilla (`checkbox`) o botón de opción (`radio`, `true`).
    Marca(bool),
    /// `<select>`.
    Lista,
    /// `<textarea>`.
    Area,
}

/// El control que es `id`, si lo es (`<input type=hidden>` no se dibuja).
pub(crate) fn tipo_control(m: &Maquetador<'_>, id: NodoId) -> Option<TipoControl> {
    let el = m.doc.elemento(id)?;
    match el.nombre.as_str() {
        "input" => Some(match el.atributo("type").map(str::to_ascii_lowercase).as_deref() {
            Some("submit" | "button" | "reset" | "image") => TipoControl::Boton,
            Some("checkbox") => TipoControl::Marca(false),
            Some("radio") => TipoControl::Marca(true),
            Some("hidden") => return None,
            _ => TipoControl::Texto,
        }),
        "select" => Some(TipoControl::Lista),
        "textarea" => Some(TipoControl::Area),
        _ => None,
    }
}

/// Lado de una casilla, en píxeles CSS (como Chromium).
const LADO_MARCA: f32 = 13.0;
/// Gris del texto de ayuda (`placeholder`).
const GRIS_AYUDA: Color = Color { r: 117, g: 117, b: 117, a: 255 };

impl<'a> Maquetador<'a> {
    /// Lo que muestra un control y si es el texto de ayuda (en gris).
    fn texto_control(&self, id: NodoId, tipo: TipoControl) -> (String, bool) {
        let Some(el) = self.doc.elemento(id) else { return (String::new(), false) };
        let atributo = |n: &str| el.atributo(n).map(String::from);
        match tipo {
            TipoControl::Texto | TipoControl::Area => {
                let escrito = self.formulario.valor(id).or_else(|| match tipo {
                    TipoControl::Area => Some(self.doc.texto(id)),
                    _ => atributo("value"),
                });
                let escrito = escrito.unwrap_or_default();
                let clave = el.atributo("type").is_some_and(|t| t.eq_ignore_ascii_case("password"));
                match (escrito.is_empty(), clave) {
                    (true, _) => (atributo("placeholder").unwrap_or_default(), true),
                    (false, true) => (escrito.chars().map(|_| '•').collect(), false),
                    (false, false) => (escrito, false),
                }
            }
            TipoControl::Boton => {
                let por_defecto = match el.atributo("type").map(str::to_ascii_lowercase).as_deref() {
                    Some("reset") => "Restablecer",
                    Some("submit" | "image") => "Enviar",
                    _ => "",
                };
                (atributo("value").unwrap_or_else(|| String::from(por_defecto)), false)
            }
            TipoControl::Lista => (self.opcion_elegida(id), false),
            TipoControl::Marca(_) => (String::new(), false),
        }
    }

    /// El texto de la opción elegida de un `<select>` (la marcada con `selected`, o la primera).
    fn opcion_elegida(&self, id: NodoId) -> String {
        if let Some(v) = self.formulario.valor(id) {
            return v;
        }
        let opciones: Vec<NodoId> = self
            .doc
            .descendientes(id)
            .filter(|&o| self.doc.elemento(o).is_some_and(|e| e.nombre == "option"))
            .collect();
        let elegida =
            opciones.iter().find(|&&o| self.doc.elemento(o).is_some_and(|e| e.atributo("selected").is_some()));
        elegida.or(opciones.first()).map(|&o| String::from(self.doc.texto(o).trim())).unwrap_or_default()
    }

    /// El ancho del contenido de un control sin `width`: como lo hace Chromium, `size` caracteres
    /// para un campo, su texto para un botón, la opción más larga para una lista.
    pub(crate) fn ancho_natural_control(&self, id: NodoId, tipo: TipoControl, e: &Estilo) -> f32 {
        let f = fuente(e);
        let cero = self.medir.ancho("0", &f);
        let numero = |n: &str, d: f32| {
            self.doc.elemento(id).and_then(|el| el.atributo(n)).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d)
        };
        match tipo {
            TipoControl::Texto => numero("size", 20.0) * cero,
            TipoControl::Area => numero("cols", 20.0) * cero,
            TipoControl::Boton => self.medir.ancho(&self.texto_control(id, tipo).0, &f),
            TipoControl::Marca(_) => LADO_MARCA,
            TipoControl::Lista => {
                let mas_larga = self
                    .doc
                    .descendientes(id)
                    .filter(|&o| self.doc.elemento(o).is_some_and(|e| e.nombre == "option"))
                    .map(|o| self.medir.ancho(self.doc.texto(o).trim(), &f))
                    .fold(0.0, f32::max);
                mas_larga + e.tam_letra * 1.2 // la flecha
            }
        }
    }

    /// Un control con su caja con borde empezando en (x, y).
    pub(crate) fn control(
        &mut self,
        id: NodoId,
        tipo: TipoControl,
        e: &Estilo,
        (x, y): (f32, f32),
        contenedor: f32,
    ) -> Hecho {
        let px = |m: Medida| m.px(contenedor, 0.0);
        let (bt, br, bb, bl) = (e.borde[0].ocupa(), e.borde[1].ocupa(), e.borde[2].ocupa(), e.borde[3].ocupa());
        let (pt, pr, pb, pl) = (px(e.relleno[0]), px(e.relleno[1]), px(e.relleno[2]), px(e.relleno[3]));
        let (extras_h, extras_v) = (bl + br + pl + pr, bt + bb + pt + pb);
        let (ml, mr) = (px(e.margen[3]), px(e.margen[1]));
        let f = fuente(e);
        let (asc, desc) = (self.medir.ascenso(&f), self.medir.descenso(&f));
        let lh = e.alto_linea_px();
        let filas = self.doc.elemento(id).and_then(|el| el.atributo("rows")).and_then(|v| v.parse::<f32>().ok());
        let w = match e.ancho {
            _ if self.ancho_libre == Some(id) => (contenedor - ml - mr - extras_h).max(0.0),
            Medida::Px(v) => v,
            Medida::Porcentaje(p) => (contenedor * p / 100.0 - e.extras_si_caja_borde(contenedor)).max(0.0),
            Medida::Auto => self.ancho_natural_control(id, tipo, e),
        };
        let h = match (self.alto_forzado.filter(|(n, _)| *n == id), e.alto, tipo) {
            (Some((_, forzado)), _, _) => (forzado - extras_v).max(0.0),
            (None, Medida::Px(v), _) => v,
            (None, _, TipoControl::Area) => filas.unwrap_or(2.0) * lh,
            (None, _, TipoControl::Marca(_)) => LADO_MARCA,
            _ => lh,
        };
        let (bx, by) = (x + ml, y);
        let (cx, cy) = (bx + bl + pl, by + bt + pt);
        let inicio = self.piezas.len();
        // Una sola línea va centrada en vertical; un área de texto, arriba.
        let base = match tipo {
            TipoControl::Area => cy + (lh - asc - desc) / 2.0 + asc,
            _ => cy + (h - asc - desc) / 2.0 + asc,
        };
        match tipo {
            TipoControl::Marca(redondo) => self.marca(id, redondo, (cx, cy), e),
            _ => self.texto_en_control(id, tipo, e, (cx, base, w)),
        }
        let (ancho_borde, alto_borde) = (w + extras_h, h + extras_v);
        self.fondo_y_bordes(inicio, e, bx, by, ancho_borde, alto_borde);
        self.enlaces.push((bx, by, ancho_borde, alto_borde, CONTROL | id.0));
        let primera_base = Some(if matches!(tipo, TipoControl::Marca(_)) { by + alto_borde } else { base });
        Hecho { alto: alto_borde, margen_inferior: px(e.margen[2]), primera_base, caja: (bx, ancho_borde) }
    }

    /// El texto de un control (cortado si no cabe), la flecha de una lista y el cursor con el foco.
    fn texto_en_control(&mut self, id: NodoId, tipo: TipoControl, e: &Estilo, (cx, base, w): (f32, f32, f32)) {
        let f = fuente(e);
        let (texto, ayuda) = self.texto_control(id, tipo);
        let flecha = if tipo == TipoControl::Lista { e.tam_letra * 1.2 } else { 0.0 };
        // Lo que cabe: se quitan letras del final (un campo con mucho texto muestra el principio).
        let mut visible = texto;
        while !visible.is_empty() && self.medir.ancho(&visible, &f) > w - flecha + 0.5 {
            visible.pop();
        }
        let ancho = self.medir.ancho(&visible, &f);
        let x = if tipo == TipoControl::Boton { cx + (w - ancho).max(0.0) / 2.0 } else { cx };
        let color = if ayuda { GRIS_AYUDA } else { e.color };
        let negrita = e.negrita();
        let pieza = |texto: String, x: f32, ancho: f32| Pieza::Texto {
            x,
            base,
            ancho,
            texto,
            tam: e.tam_letra,
            color,
            negrita,
            cursiva: e.cursiva,
            generica: e.generica,
            familia: self.medir.familia(&f).cloned(),
        };
        if !visible.is_empty() {
            let p = pieza(visible, x, ancho);
            self.piezas.push(p);
        }
        if tipo == TipoControl::Lista {
            let p = pieza(String::from("▾"), cx + w - flecha * 0.8, flecha * 0.6);
            self.piezas.push(p);
        }
        let escribible = matches!(tipo, TipoControl::Texto | TipoControl::Area);
        if escribible && e.visible && self.formulario.foco() == Some(id) {
            let (asc, desc) = (self.medir.ascenso(&f), self.medir.descenso(&f));
            let x_cursor = if ayuda { cx } else { x + ancho };
            self.piezas.push(Pieza::Rect { x: x_cursor, y: base - asc, ancho: 1.0, alto: asc + desc, color: e.color });
        }
    }

    /// Una casilla o un botón de opción, con su marca si está marcado.
    fn marca(&mut self, id: NodoId, redondo: bool, (cx, cy): (f32, f32), e: &Estilo) {
        let marcado = self
            .formulario
            .marcado(id)
            .unwrap_or_else(|| self.doc.elemento(id).is_some_and(|el| el.atributo("checked").is_some()));
        let gris = Color { r: 118, g: 118, b: 118, a: 255 };
        let lado = LADO_MARCA;
        let caja =
            [(0.0, 0.0, lado, 1.0), (0.0, lado - 1.0, lado, 1.0), (0.0, 0.0, 1.0, lado), (lado - 1.0, 0.0, 1.0, lado)];
        if !e.visible {
            return;
        }
        self.piezas.push(Pieza::Rect { x: cx, y: cy, ancho: lado, alto: lado, color: Color::BLANCO });
        for (dx, dy, w, h) in caja {
            self.piezas.push(Pieza::Rect { x: cx + dx, y: cy + dy, ancho: w, alto: h, color: gris });
        }
        if marcado {
            let hueco = if redondo { 4.0 } else { 3.0 };
            let azul = Color { r: 0, g: 117, b: 255, a: 255 };
            let (x, y, w) = (cx + hueco, cy + hueco, lado - 2.0 * hueco);
            self.piezas.push(Pieza::Rect { x, y, ancho: w, alto: w, color: azul });
        }
    }

    /// Una caja de bloque dentro de una línea (`inline-block`, botones, campos): se maqueta con su
    /// esquina en (0, 0) y entra en la línea como un elemento más (se mueve al ponerla).
    pub(crate) fn caja_en_linea(&mut self, id: NodoId, e: &'a Estilo) -> Item<'a> {
        let contenedor = self.ancho_actual;
        let px = |m: Medida| m.px(contenedor, 0.0);
        let (ml, mr, mt, mb) = (px(e.margen[3]), px(e.margen[1]), px(e.margen[0]), px(e.margen[2]));
        let exterior = match e.ancho {
            // Encogida a su contenido, sin pasar del sitio que hay.
            Medida::Auto => {
                let (min, max) = self.intrinseco(id);
                max.min((contenedor - ml - mr).max(min)) + ml + mr
            }
            m => m.px(contenedor, 0.0) - e.extras_si_caja_borde(contenedor) + extras_horizontales(e) + ml + mr,
        };
        let (p0, e0) = (self.piezas.len(), self.enlaces.len());
        let hecho = self.bloque_libre(id, 0.0, mt, exterior);
        let piezas = self.piezas.split_off(p0);
        let mut enlaces: Vec<EnlaceBruto> = self.enlaces.split_off(e0);
        let alto = mt + hecho.alto + mb;
        // Un `<button>` se pulsa como un control (envía su formulario, o un script lo escucha).
        if self.doc.elemento(id).is_some_and(|el| el.nombre == "button") {
            enlaces.push((ml, mt, exterior - ml - mr, hecho.alto, CONTROL | id.0));
        }
        let base = hecho.primera_base.unwrap_or(alto);
        Item::Caja { ancho: exterior, alto, base, piezas, enlaces, estilo: e }
    }
}
