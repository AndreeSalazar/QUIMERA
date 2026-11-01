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
mod control;
mod flex;
mod flotante;
mod intrinseco;
mod linea;
mod pintura;
mod posicion;
mod reemplazado;
mod tabla;
mod texto;

pub use control::{Formulario, SinFormulario};
pub use pintura::{Control, Enlace, Maquetacion, Pieza};
pub use texto::{Aproximado, Fuente, MedirTexto};

use quimera_estilo::{Color, Estilos};
use quimera_html::Documento;

/// Maqueta el documento en una ventana de `ancho` × `alto` píxeles CSS (sin imágenes cargadas).
pub fn maquetar(doc: &Documento, estilos: &Estilos, medir: &dyn MedirTexto, ancho: f32, alto: f32) -> Maquetacion {
    maquetar_con_imagenes(doc, estilos, medir, (ancho, alto), &|_| None)
}

/// Como [`maquetar`], sabiendo el tamaño propio de las imágenes ya descargadas (por su `src`).
pub fn maquetar_con_imagenes(
    doc: &Documento,
    estilos: &Estilos,
    medir: &dyn MedirTexto,
    (ancho, alto): (f32, f32),
    imagenes: &dyn Fn(&str) -> Option<(u32, u32)>,
) -> Maquetacion {
    maquetar_pagina(doc, estilos, medir, (ancho, alto), imagenes, &SinFormulario)
}

/// Como [`maquetar_con_imagenes`], con lo escrito en los controles y el foco (`formulario`).
pub fn maquetar_pagina(
    doc: &Documento,
    estilos: &Estilos,
    medir: &dyn MedirTexto,
    (ancho, alto): (f32, f32),
    imagenes: &dyn Fn(&str) -> Option<(u32, u32)>,
    formulario: &dyn Formulario,
) -> Maquetacion {
    let mut m = bloque::Maquetador::nuevo(doc, estilos, medir, imagenes);
    m.formulario = formulario;
    let Some(html) = doc.buscar("html") else {
        return Maquetacion { ancho, alto, fondo: Color::BLANCO, ..Maquetacion::default() };
    };
    m.raiz = Some(html);
    let hecho = m.bloque(html, 0.0, 0.0, ancho);
    // Lo `absolute` sin antepasado posicionado y lo `fixed`, dentro de la ventana.
    m.poner_absolutos(0, (0.0, 0.0, ancho, alto), true);
    let (piezas, abajo_capas) = m.apilar();
    // El fondo de <html> (o, si no tiene, el de <body>) pinta todo el lienzo.
    let fondo_de = |n: &str| doc.buscar(n).and_then(|id| estilos.de(id)).map(|e| e.fondo).filter(|c| c.a > 0);
    let fondo = fondo_de("html").or_else(|| fondo_de("body")).unwrap_or(Color::BLANCO);
    // Los rectángulos de los controles van en la misma lista que los de los enlaces, marcados.
    let (controles, enlaces): (alloc::vec::Vec<_>, alloc::vec::Vec<_>) =
        m.enlaces.iter().partition(|e| e.4 & control::CONTROL != 0);
    let controles = controles
        .into_iter()
        .map(|&(x, y, ancho, alto, n)| Control { x, y, ancho, alto, nodo: quimera_html::NodoId(n & !control::CONTROL) })
        .collect();
    let enlaces = enlaces
        .into_iter()
        .map(|&(x, y, ancho, alto, i)| Enlace { x, y, ancho, alto, href: m.hrefs[i as usize].clone() })
        .collect();
    let alto_doc = hecho.alto.max(abajo_capas).max(alto);
    Maquetacion { piezas, ancho, alto: alto_doc, fondo, enlaces, anclas: m.anclas, controles }
}
