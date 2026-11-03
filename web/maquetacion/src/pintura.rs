//! La lista de dibujo: lo que sale de la maquetación y entra al pintado, en orden de pintado
//! (lo primero queda debajo).

use alloc::string::String;
use alloc::vec::Vec;
use quimera_estilo::{Color, Generica};

/// Una cosa que dibujar. Las coordenadas son píxeles CSS desde la esquina del documento.
#[derive(Debug, Clone, PartialEq)]
pub enum Pieza {
    /// Un rectángulo relleno (fondos, bordes, subrayados).
    Rect {
        /// Izquierda.
        x: f32,
        /// Arriba.
        y: f32,
        /// Ancho.
        ancho: f32,
        /// Alto.
        alto: f32,
        /// Color.
        color: Color,
    },
    /// Un trozo de texto en una línea.
    Texto {
        /// Izquierda.
        x: f32,
        /// La línea base (donde se apoyan las letras).
        base: f32,
        /// El ancho que ocupa (medido al maquetar).
        ancho: f32,
        /// El texto.
        texto: String,
        /// Tamaño de la letra.
        tam: f32,
        /// Color.
        color: Color,
        /// Negrita.
        negrita: bool,
        /// Cursiva.
        cursiva: bool,
        /// Familia genérica.
        generica: Generica,
        /// La primera familia pedida, si hay.
        familia: Option<String>,
    },
    /// Una imagen (`<img>`), todavía sin cargar.
    Imagen {
        /// Izquierda.
        x: f32,
        /// Arriba.
        y: f32,
        /// Ancho.
        ancho: f32,
        /// Alto.
        alto: f32,
        /// Su `src`.
        src: String,
    },
}

impl Pieza {
    /// La mueve `dy` píxeles hacia abajo.
    pub(crate) fn bajar(&mut self, dy: f32) {
        self.mover(0.0, dy);
    }

    /// La mueve `dx` a la derecha y `dy` hacia abajo.
    pub(crate) fn mover(&mut self, dx: f32, dy: f32) {
        match self {
            Pieza::Rect { x, y, .. } | Pieza::Imagen { x, y, .. } => (*x, *y) = (*x + dx, *y + dy),
            Pieza::Texto { x, base, .. } => (*x, *base) = (*x + dx, *base + dy),
        }
    }

    /// Dónde acaba por abajo (más o menos, para el texto: su tamaño bajo la línea base).
    pub(crate) fn abajo(&self) -> f32 {
        match self {
            Pieza::Rect { y, alto, .. } | Pieza::Imagen { y, alto, .. } => y + alto,
            Pieza::Texto { base, tam, .. } => base + tam * 0.25,
        }
    }
}

/// Un trozo de enlace: un rectángulo donde pulsar y a dónde lleva.
#[derive(Debug, Clone, PartialEq)]
pub struct Enlace {
    /// Izquierda.
    pub x: f32,
    /// Arriba.
    pub y: f32,
    /// Ancho.
    pub ancho: f32,
    /// Alto.
    pub alto: f32,
    /// El `href` tal como está en la página (sin resolver).
    pub href: String,
    /// Su `<a>` (para lanzarle el evento `click`).
    pub nodo: quimera_html::NodoId,
}

/// El resultado de maquetar una página.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Maquetacion {
    /// Lo que hay que dibujar, en orden.
    pub piezas: Vec<Pieza>,
    /// Ancho del documento.
    pub ancho: f32,
    /// Alto del documento (al menos el de la ventana).
    pub alto: f32,
    /// Color del lienzo (el fondo de `<html>`, o el de `<body>` si aquel no tiene).
    pub fondo: Color,
    /// Dónde están los enlaces (un rectángulo por trozo de línea).
    pub enlaces: Vec<Enlace>,
    /// (id, y) de cada bloque con `id`, para ir a `#ancla`.
    pub anclas: Vec<(String, f32)>,
    /// Dónde están los controles de formulario (para saber qué se pulsa).
    pub controles: Vec<Control>,
    /// La caja de borde (x, y, ancho, alto) de cada bloque, en orden de maquetación (si uno sale
    /// dos veces, vale la última): lo que leen `clientWidth` o `getBoundingClientRect`.
    pub cajas: Vec<(quimera_html::NodoId, [f32; 4])>,
}

/// El rectángulo de un control de formulario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Control {
    /// Izquierda.
    pub x: f32,
    /// Arriba.
    pub y: f32,
    /// Ancho.
    pub ancho: f32,
    /// Alto.
    pub alto: f32,
    /// El elemento (`<input>`, `<select>`, `<textarea>`).
    pub nodo: quimera_html::NodoId,
}

impl Maquetacion {
    /// El control que hay en el punto (x, y) del documento (el último dibujado: el de encima).
    pub fn control_en(&self, x: f32, y: f32) -> Option<quimera_html::NodoId> {
        self.controles
            .iter()
            .rev()
            .find(|c| x >= c.x && x < c.x + c.ancho && y >= c.y && y < c.y + c.alto)
            .map(|c| c.nodo)
    }

    /// El enlace que hay en el punto (x, y) del documento.
    pub fn enlace_en(&self, x: f32, y: f32) -> Option<&str> {
        self.trozo_de_enlace_en(x, y).map(|e| e.href.as_str())
    }

    /// El trozo de enlace (con su `href` y su `<a>`) que hay en el punto (x, y) del documento.
    pub fn trozo_de_enlace_en(&self, x: f32, y: f32) -> Option<&Enlace> {
        self.enlaces.iter().find(|e| x >= e.x && x < e.x + e.ancho && y >= e.y && y < e.y + e.alto)
    }

    /// La altura de un ancla (`#id`).
    pub fn ancla(&self, id: &str) -> Option<f32> {
        self.anclas.iter().find(|(i, _)| i == id).map(|(_, y)| *y)
    }
}

impl Maquetacion {
    /// Todo el texto en orden de dibujo, una pieza por línea (para pruebas).
    pub fn textos(&self) -> Vec<&str> {
        self.piezas
            .iter()
            .filter_map(|p| match p {
                Pieza::Texto { texto, .. } => Some(texto.as_str()),
                _ => None,
            })
            .collect()
    }
}
