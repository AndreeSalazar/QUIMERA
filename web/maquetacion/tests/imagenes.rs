//! El tamaño de `<img>`: los mismos resultados que da Chromium con estas páginas.

use quimera_estilo::{Opciones, calcular};
use quimera_maquetacion::{Fuente, Maquetacion, MedirTexto, Pieza, maquetar_con_imagenes};

struct MedioEm;

impl MedirTexto for MedioEm {
    fn ancho(&self, texto: &str, f: &Fuente<'_>) -> f32 {
        texto.chars().count() as f32 * f.tam * 0.5
    }
}

/// Todas las imágenes "propias" miden 400×200 (proporción 2:1), salvo `falta.png`, que no llegó.
fn maquetar_html(html: &str) -> Maquetacion {
    let doc = quimera_html::analizar(html);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar_con_imagenes(&doc, &estilos, &MedioEm, (800.0, 600.0), &|src| (src != "falta.png").then_some((400, 200)))
}

/// (x, y, ancho, alto) de cada imagen.
fn imagenes(m: &Maquetacion) -> Vec<(f32, f32, f32, f32)> {
    m.piezas
        .iter()
        .filter_map(|p| match p {
            Pieza::Imagen { x, y, ancho, alto, .. } => Some((*x, *y, *ancho, *alto)),
            _ => None,
        })
        .collect()
}

fn tam(html: &str) -> (f32, f32) {
    let v = imagenes(&maquetar_html(html));
    assert_eq!(v.len(), 1, "{v:?}");
    (v[0].2, v[0].3)
}

#[test]
fn tamano_propio() {
    assert_eq!(tam("<img src=a.png>"), (400.0, 200.0));
}

#[test]
fn una_medida_guarda_la_proporcion() {
    assert_eq!(tam("<img src=a.png width=100>"), (100.0, 50.0));
    assert_eq!(tam("<img src=a.png style='height:50px'>"), (100.0, 50.0));
    assert_eq!(tam("<img src=a.png style='width:50%'>"), (392.0, 196.0));
}

#[test]
fn las_dos_medidas_deforman() {
    assert_eq!(tam("<img src=a.png width=100 height=100>"), (100.0, 100.0));
}

#[test]
fn css_gana_al_atributo() {
    assert_eq!(tam("<style>img{width:40px}</style><img src=a.png width=100 height=100>"), (40.0, 100.0));
}

#[test]
fn max_width_reduce_sin_deformar() {
    // El patrón de casi todas las páginas: atributos y luego `max-width:100%; height:auto`.
    let html = "<style>img{max-width:100%;height:auto}</style><div style='width:200px'><img src=a.png width=400 height=200></div>";
    assert_eq!(tam(html), (200.0, 100.0));
}

#[test]
fn sin_cargar_usa_la_proporcion_de_los_atributos() {
    let html = "<style>img{max-width:100%;height:auto}</style><div style='width:200px'><img src=falta.png width=400 height=100></div>";
    assert_eq!(tam(html), (200.0, 50.0));
}

#[test]
fn bloque_centrado() {
    let v = imagenes(&maquetar_html("<img src=a.png style='display:block;margin:0 auto;width:100px'>"));
    // body: 8px de margen, 784px de ancho; (784 − 100) / 2 = 342.
    assert_eq!(v, vec![(350.0, 8.0, 100.0, 50.0)]);
}

#[test]
fn elemento_flexible() {
    let v =
        imagenes(&maquetar_html("<div style='display:flex'><img src=a.png style='width:100px'><img src=a.png></div>"));
    assert_eq!(v.len(), 2, "{v:?}");
    assert_eq!((v[0].0, v[0].2), (8.0, 100.0));
    // La segunda cabe entera (100 + 400 < 784): empieza tras la primera, con su tamaño propio.
    assert_eq!((v[1].0, v[1].2, v[1].3), (108.0, 400.0, 200.0));
}
