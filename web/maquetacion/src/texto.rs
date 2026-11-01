//! Medir texto: lo único que la maquetación necesita de las fuentes.
//!
//! Es un rasgo para que las fuentes de verdad (`quimera-texto`) entren después sin tocar la
//! maquetación. Mientras tanto, [`Aproximado`] da anchos por carácter parecidos a los de
//! Arial/Times/Courier.

use alloc::string::String;
use quimera_estilo::Generica;

/// La letra con que se mide un texto.
#[derive(Debug, Clone, Copy)]
pub struct Fuente<'a> {
    /// Tamaño en píxeles.
    pub tam: f32,
    /// Negrita.
    pub negrita: bool,
    /// Cursiva.
    pub cursiva: bool,
    /// Familia genérica.
    pub generica: Generica,
    /// Las familias pedidas, en orden.
    pub familias: &'a [String],
}

/// Lo que la maquetación pregunta a las fuentes.
pub trait MedirTexto {
    /// El ancho de un texto en píxeles.
    fn ancho(&self, texto: &str, fuente: &Fuente<'_>) -> f32;
    /// Cuánto sube la letra sobre la línea base.
    fn ascenso(&self, fuente: &Fuente<'_>) -> f32 {
        fuente.tam * 0.8
    }
    /// Cuánto baja bajo la línea base.
    fn descenso(&self, fuente: &Fuente<'_>) -> f32 {
        fuente.tam * 0.2
    }
}

/// Medidas aproximadas sin fuentes: un ancho por clase de carácter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Aproximado;

impl MedirTexto for Aproximado {
    fn ancho(&self, texto: &str, f: &Fuente<'_>) -> f32 {
        let em: f32 = match f.generica {
            Generica::Monoespaciada => texto.chars().count() as f32 * 0.6,
            g => texto.chars().map(|c| ancho_proporcional(c, g == Generica::Serif)).sum(),
        };
        em * f.tam * if f.negrita { 1.06 } else { 1.0 }
    }
}

/// Ancho en `em` de un carácter en una fuente proporcional (medias de Arial y Times).
fn ancho_proporcional(c: char, serif: bool) -> f32 {
    let base = match c {
        ' ' | '\u{a0}' => 0.27,
        'i' | 'j' | 'l' | '.' | ',' | '\'' | '!' | '|' | ':' | ';' | 'I' => 0.26,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' | '/' => 0.34,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.84,
        'A'..='Z' => 0.67,
        '0'..='9' | 'a'..='z' | '$' | '#' | '?' | '_' => 0.53,
        c if c.is_ascii() => 0.5,
        c if ('\u{2E80}'..='\u{9FFF}').contains(&c) || ('\u{AC00}'..='\u{D7AF}').contains(&c) => 1.0,
        _ => 0.56,
    };
    if serif { base * 0.95 } else { base }
}
