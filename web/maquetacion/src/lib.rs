//! Maquetación de QUIMERA: el documento con sus estilos → una lista de dibujo.
//!
//! Por ahora, el flujo normal de CSS: bloques que se apilan con márgenes que colapsan,
//! `width`/`max-width`/`margin: auto`, bordes y rellenos, texto partido en líneas con
//! `white-space`, `text-align` y `line-height`, listas con marcador, `<br>` e `<img>` con
//! tamaño. El texto se mide a través de [`MedirTexto`], así las fuentes de verdad entran
//! después sin tocar esto.
//!
//! ```
//! use quimera_maquetacion::{Aproximado, maquetar};
//! let doc = quimera_html::analizar("<h1>Hola</h1><p>mundo");
//! let estilos = quimera_estilo::calcular(&doc, &quimera_estilo::Opciones { ancho: 800.0, alto: 600.0 });
//! let m = maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0);
//! assert_eq!(m.textos(), ["Hola", "mundo"]);
//! ```
//!
//! Inspiración: Servo `layout` y la especificación CSS 2.1 (modelo de cajas, flujo normal).
//! Lo que falta está en `docs/ESTILO-MAQUETACION.md`.
#![no_std]

extern crate alloc;

mod bloque;
mod linea;
mod pintura;
mod texto;

pub use pintura::{Enlace, Maquetacion, Pieza};
pub use texto::{Aproximado, Fuente, MedirTexto};

use quimera_estilo::{Color, Estilos};
use quimera_html::Documento;

/// Maqueta el documento en una ventana de `ancho` × `alto` píxeles CSS.
pub fn maquetar(doc: &Documento, estilos: &Estilos, medir: &dyn MedirTexto, ancho: f32, alto: f32) -> Maquetacion {
    let mut m = bloque::Maquetador::nuevo(doc, estilos, medir);
    let Some(html) = doc.buscar("html") else {
        return Maquetacion { ancho, alto, fondo: Color::BLANCO, ..Maquetacion::default() };
    };
    m.raiz = Some(html);
    let hecho = m.bloque(html, 0.0, 0.0, ancho);
    // El fondo de <html> (o, si no tiene, el de <body>) pinta todo el lienzo.
    let fondo_de = |n: &str| doc.buscar(n).and_then(|id| estilos.de(id)).map(|e| e.fondo).filter(|c| c.a > 0);
    let fondo = fondo_de("html").or_else(|| fondo_de("body")).unwrap_or(Color::BLANCO);
    let enlaces = m
        .enlaces
        .iter()
        .map(|&(x, y, ancho, alto, i)| Enlace { x, y, ancho, alto, href: m.hrefs[i as usize].clone() })
        .collect();
    Maquetacion { piezas: m.piezas, ancho, alto: hecho.alto.max(alto), fondo, enlaces, anclas: m.anclas }
}
