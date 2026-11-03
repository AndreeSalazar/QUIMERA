//! Cascada de estilos de QUIMERA: para cada elemento del documento, el valor de cada propiedad.
//!
//! Junta la hoja del navegador (`navegador.css`), los `<style>` de la página y el atributo
//! `style`; aplica las reglas de `@media` que se cumplen en la pantalla; ordena por origen,
//! `!important`, especificidad y orden; expande los atajos (`margin`, `border`, `font`...);
//! hereda lo heredable y pasa las unidades a píxeles.
//!
//! ```
//! let doc = quimera_html::analizar("<style>p { color: red; font-size: 2em }</style><p>Hola");
//! let estilos = quimera_estilo::calcular(&doc, &quimera_estilo::Opciones { ancho: 800.0, alto: 600.0 });
//! let p = estilos.de(doc.buscar("p").unwrap()).unwrap();
//! assert_eq!((p.color, p.tam_letra), (quimera_estilo::Color::rgb(255, 0, 0), 32.0));
//! ```
//!
//! Inspiración: Stylo (Servo). Lo que falta está en `docs/ESTILO-MAQUETACION.md`.
#![no_std]

extern crate alloc;

mod cascada;
mod elemento;
mod flex;
mod fuentes;
mod medios;
mod posicion;
mod propiedades;
mod tabla;
mod valores;
mod variables;

use alloc::vec::Vec;
pub use cascada::CacheHojas;
pub use elemento::Nodo;
pub use flex::{AlinearFlex, Direccion, Flex, Justificar};
pub use fuentes::{FuenteWeb, fuentes_web};
pub use posicion::{Colocacion, Posicion};
pub use propiedades::borde_solido;
pub use quimera_html::NodoId;
pub use tabla::{AlinearVertical, Tabla};
pub use valores::{
    Alinear, AltoLinea, Borde, Color, Display, Espacios, Estilo, EstiloBorde, Flotar, Generica, Limpiar, Medida,
    SIN_BORDE, TipoLista,
};
pub use variables::Variables;

/// La pantalla para la que se calcula (`@media`, `vw`, `vh`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opciones {
    /// Ancho de la ventana en píxeles CSS.
    pub ancho: f32,
    /// Alto de la ventana.
    pub alto: f32,
}

/// El estilo calculado de cada elemento.
#[derive(Debug, Clone, Default)]
pub struct Estilos {
    estilos: Vec<Option<Estilo>>,
}

impl Estilos {
    /// El estilo de un elemento (`None` si no es un elemento o está dentro de un `display: none`).
    pub fn de(&self, id: NodoId) -> Option<&Estilo> {
        self.estilos.get(id.0 as usize)?.as_ref()
    }
}

/// Calcula los estilos de todo el documento (sin hojas externas).
pub fn calcular(doc: &quimera_html::Documento, op: &Opciones) -> Estilos {
    cascada::calcular(doc, op, &|_| None, &CacheHojas::default())
}

/// Como [`calcular`], con las hojas de `<link rel="stylesheet">`: `externa` recibe cada `href`
/// y devuelve el texto CSS si lo tiene (el estilo no lee archivos ni red: lo hace quien llama).
pub fn calcular_con_hojas(
    doc: &quimera_html::Documento,
    op: &Opciones,
    externa: &dyn Fn(&str) -> Option<alloc::string::String>,
) -> Estilos {
    cascada::calcular(doc, op, externa, &CacheHojas::default())
}

/// Como [`calcular_con_hojas`], guardando en `cache` las hojas analizadas para la próxima vez.
pub fn calcular_con_cache(
    doc: &quimera_html::Documento,
    op: &Opciones,
    externa: &dyn Fn(&str) -> Option<alloc::string::String>,
    cache: &CacheHojas,
) -> Estilos {
    cascada::calcular(doc, op, externa, cache)
}
