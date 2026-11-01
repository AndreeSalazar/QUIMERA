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
        match self {
            Pieza::Rect { y, .. } | Pieza::Imagen { y, .. } => *y += dy,
            Pieza::Texto { base, .. } => *base += dy,
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
}

impl Maquetacion {
    /// El enlace que hay en el punto (x, y) del documento.
    pub fn enlace_en(&self, x: f32, y: f32) -> Option<&str> {
        self.enlaces
            .iter()
            .find(|e| x >= e.x && x < e.x + e.ancho && y >= e.y && y < e.y + e.alto)
            .map(|e| e.href.as_str())
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
