//! Flotantes: las mismas posiciones que da Chromium con `docs/ejemplos/flotantes.html`.
//!
//! La página no tiene texto: las líneas son imágenes de tamaño fijo con `font-size: 0`, así que
//! las fuentes no influyen. Los números esperados salen de Chromium (`getBoundingClientRect` en
//! una ventana de 800 × 600).
//!
//! Ojo al caso 6: su flotante sobresale por debajo de su bloque (un bloque normal no abarca sus
//! flotantes) y empuja a los casos 7 y 8, como en Chromium.

use quimera_estilo::{Color, Opciones, calcular};
use quimera_maquetacion::{Aproximado, Maquetacion, Pieza, maquetar};

const PAGINA: &str = include_str!("../../../docs/ejemplos/flotantes.html");

/// Una caja (por el rojo de su fondo `rgb(k, 150, 50)`) o una imagen (por su `src`).
#[derive(Debug, Clone, Copy)]
enum Que {
    Caja(u8),
    Imagen(&'static str),
}

use Que::{Caja, Imagen};

/// (qué, x, y, ancho, alto) según Chromium.
const CHROMIUM: &[(Que, f32, f32, f32, f32)] = &[
    // 1: las líneas rodean al flotante y vuelven al ancho completo
    (Caja(1), 8.0, 8.0, 100.0, 50.0),
    (Imagen("i1"), 108.0, 8.0, 60.0, 20.0),
    (Imagen("i3"), 228.0, 8.0, 60.0, 20.0),
    (Imagen("i4"), 108.0, 28.0, 60.0, 20.0),
    (Imagen("i7"), 108.0, 48.0, 60.0, 20.0),
    (Imagen("i8"), 168.0, 48.0, 60.0, 20.0),
    // 2: uno a cada lado
    (Caja(2), 8.0, 88.0, 50.0, 30.0),
    (Caja(3), 238.0, 88.0, 70.0, 30.0),
    (Imagen("j1"), 58.0, 88.0, 100.0, 10.0),
    (Imagen("j2"), 58.0, 98.0, 100.0, 10.0),
    (Imagen("j3"), 58.0, 108.0, 100.0, 10.0),
    // 3: el segundo no cabe al lado del primero y baja; el derecho no sube por encima de él
    (Caja(4), 8.0, 138.0, 200.0, 40.0),
    (Caja(5), 8.0, 178.0, 150.0, 30.0),
    (Caja(6), 218.0, 178.0, 90.0, 20.0),
    // 4: clear
    (Caja(7), 8.0, 278.0, 80.0, 40.0),
    (Caja(8), 228.0, 278.0, 80.0, 60.0),
    (Caja(9), 8.0, 318.0, 300.0, 10.0),
    (Caja(10), 8.0, 338.0, 300.0, 10.0),
    // 5: ancho encogido a su contenido, con relleno y margen
    (Caja(11), 215.0, 371.0, 90.0, 30.0),
    (Imagen("k1"), 220.0, 376.0, 80.0, 20.0),
    (Imagen("k2"), 8.0, 368.0, 150.0, 10.0),
    // 6: un bloque normal no abarca su flotante; el siguiente lo rodea
    (Caja(12), 8.0, 398.0, 300.0, 10.0),
    (Caja(13), 8.0, 398.0, 60.0, 50.0),
    (Imagen("l1"), 68.0, 398.0, 30.0, 10.0),
    (Caja(14), 8.0, 408.0, 300.0, 15.0),
    (Imagen("l2"), 68.0, 408.0, 30.0, 10.0),
    // 7: el flotante del caso 6 todavía sobresale; la tabla no se mete debajo de los flotantes
    (Caja(15), 68.0, 443.0, 100.0, 50.0),
    (Caja(16), 168.0, 443.0, 120.0, 20.0),
    (Imagen("m1"), 168.0, 443.0, 120.0, 20.0),
    // 8: la celda abarca su flotante (y la tabla va al lado del flotante del caso 7)
    (Caja(17), 168.0, 483.0, 60.0, 60.0),
    (Caja(18), 168.0, 483.0, 40.0, 60.0),
    (Imagen("n1"), 208.0, 483.0, 20.0, 10.0),
    // 9: junto al flotante no cabe la primera imagen: la línea baja
    (Caja(19), 8.0, 563.0, 250.0, 30.0),
    (Imagen("o1"), 8.0, 593.0, 80.0, 10.0),
    (Imagen("o2"), 88.0, 593.0, 40.0, 10.0),
    // 10: <img align=right hspace=4> dentro del texto
    (Imagen("p1"), 8.0, 623.0, 20.0, 10.0),
    (Imagen("p2"), 254.0, 623.0, 50.0, 40.0),
    (Imagen("p3"), 28.0, 623.0, 200.0, 10.0),
    (Imagen("p4"), 8.0, 633.0, 100.0, 10.0),
];

fn maquetar_pagina() -> Maquetacion {
    let doc = quimera_html::analizar(PAGINA);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0)
}

fn buscar(m: &Maquetacion, que: Que) -> Option<(f32, f32, f32, f32)> {
    m.piezas.iter().find_map(|p| match (p, que) {
        (Pieza::Rect { x, y, ancho, alto, color }, Caja(k)) if *color == Color::rgb(k, 150, 50) => {
            Some((*x, *y, *ancho, *alto))
        }
        (Pieza::Imagen { x, y, ancho, alto, src }, Imagen(s)) if src == s => Some((*x, *y, *ancho, *alto)),
        _ => None,
    })
}

#[test]
fn mismas_posiciones_que_chromium() {
    let m = maquetar_pagina();
    let mut fallos = Vec::new();
    for &(que, x, y, w, h) in CHROMIUM {
        match buscar(&m, que) {
            Some(q) if [q.0 - x, q.1 - y, q.2 - w, q.3 - h].iter().all(|d| d.abs() < 0.05) => {}
            otra => fallos.push(format!("{que:?}: Chromium {:?} · QUIMERA {otra:?}", (x, y, w, h))),
        }
    }
    assert!(fallos.is_empty(), "{} de {} distintas:\n{}", fallos.len(), CHROMIUM.len(), fallos.join("\n"));
}

#[test]
fn flow_root_y_overflow_abarcan_sus_flotantes_y_no_se_meten_debajo() {
    let doc = quimera_html::analizar(
        "<div style='float:left;width:100px;height:40px'></div>\
         <div style='display:flow-root;background:rgb(1,2,3)'><div style='float:left;width:20px;height:60px'></div></div>\
         <div style='overflow:hidden;clear:both;background:rgb(4,5,6)'><div style='float:right;width:20px;height:30px'></div></div>",
    );
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0);
    let caja = |r, g, b| {
        m.piezas.iter().find_map(|p| match p {
            Pieza::Rect { x, y, ancho, alto, color } if *color == Color::rgb(r, g, b) => Some((*x, *y, *ancho, *alto)),
            _ => None,
        })
    };
    // Al lado del flotante de 100px (8 + 100) y con el alto de su flotante de 60px.
    assert_eq!(caja(1, 2, 3), Some((108.0, 8.0, 684.0, 60.0)));
    // clear: debajo de todo (8 + 60), y abarca su flotante de 30px.
    assert_eq!(caja(4, 5, 6), Some((8.0, 68.0, 784.0, 30.0)));
}

#[test]
fn los_flotantes_se_dibujan_encima_de_los_bloques() {
    // Caso 6: el fondo del bloque 14 va debajo del flotante 13, aunque venga después en el HTML.
    let m = maquetar_pagina();
    let pos = |k: u8| {
        m.piezas.iter().position(|p| matches!(p, Pieza::Rect { color, .. } if *color == Color::rgb(k, 150, 50)))
    };
    assert!(pos(14) < pos(13), "el flotante, encima");
}
