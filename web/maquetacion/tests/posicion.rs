//! `position` y `z-index`: lo mismo que Chromium con `docs/ejemplos/posicion.html`.
//!
//! La página no tiene texto. Las posiciones esperadas salen de `getBoundingClientRect` y la caja
//! que queda encima en cada punto, de `elementFromPoint`, en una ventana de 800 × 600.

use quimera_estilo::{Color, Opciones, calcular};
use quimera_maquetacion::{Aproximado, Maquetacion, Pieza, maquetar};

const PAGINA: &str = include_str!("../../../docs/ejemplos/posicion.html");

/// (k del fondo `rgb(k, 80, 160)`, x, y, ancho, alto) según Chromium.
const CAJAS: &[(u8, f32, f32, f32, f32)] = &[
    // 1: relative se mueve 20 a la derecha y 10 abajo; su hermano no se entera
    (1, 28.0, 18.0, 50.0, 20.0),
    (2, 8.0, 28.0, 50.0, 20.0),
    // 2: absolute arriba a la derecha
    (3, 253.0, 93.0, 50.0, 20.0),
    // 3: estirado entre sus cuatro lados (dentro de la caja de relleno)
    (4, 38.0, 178.0, 258.0, 43.0),
    // 4: posición estática; no ocupa sitio
    (5, 8.0, 256.0, 300.0, 15.0),
    (6, 8.0, 271.0, 40.0, 10.0),
    (7, 8.0, 271.0, 300.0, 15.0),
    // 5: centrado con margin: auto
    (8, 108.0, 336.0, 100.0, 20.0),
    // 6: encogido a su contenido, desde abajo
    (9, 14.0, 454.0, 76.0, 18.0),
    (10, 17.0, 457.0, 70.0, 12.0),
    // 7: z-index
    (11, 8.0, 496.0, 100.0, 40.0),
    (12, 28.0, 506.0, 60.0, 40.0),
    (13, 48.0, 516.0, 60.0, 40.0),
    (14, 78.0, 496.0, 60.0, 30.0),
    // 8: relative sin z-index
    (15, 8.0, 596.0, 80.0, 30.0),
    (16, 8.0, 606.0, 120.0, 30.0),
    // 9: absolute dentro de absolute
    (17, 18.0, 666.0, 150.0, 40.0),
    (18, 128.0, 691.0, 30.0, 10.0),
    // 10: sin antepasado posicionado: arriba a la derecha de la ventana. Chromium da x = 755
    // porque esta página es más alta que la ventana y su barra de desplazamiento le quita 15 px
    // (785 de ancho); QUIMERA no le quita ancho a la página: 800 − 30 = 770.
    (19, 770.0, 0.0, 30.0, 30.0),
];

/// (x, y, la caja que queda encima) según Chromium.
const ENCIMA: &[(f32, f32, u8)] = &[
    (40.0, 538.0, 12), // z-index 2 encima del contenido normal
    (70.0, 530.0, 12), // z-index 2 encima del 1
    (85.0, 533.0, 12),
    (90.0, 510.0, 11), // z-index -1 debajo del contenido normal
    (20.0, 610.0, 15), // un relative encima del bloque normal que le sigue
    (100.0, 620.0, 16),
    (140.0, 696.0, 18), // el absolute de dentro, encima del de fuera
];

fn maquetar_pagina() -> Maquetacion {
    let doc = quimera_html::analizar(PAGINA);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0)
}

/// (k, x, y, ancho, alto) de las cajas de colores, en orden de dibujo.
fn cajas(m: &Maquetacion) -> Vec<(u8, f32, f32, f32, f32)> {
    m.piezas
        .iter()
        .filter_map(|p| match p {
            Pieza::Rect { x, y, ancho, alto, color } if (color.g, color.b) == (80, 160) && color.r != 99 => {
                Some((color.r, *x, *y, *ancho, *alto))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn mismas_posiciones_que_chromium() {
    let v = cajas(&maquetar_pagina());
    let mut fallos = Vec::new();
    for &(k, x, y, w, h) in CAJAS {
        match v.iter().find(|c| c.0 == k) {
            Some(&(_, qx, qy, qw, qh)) if [qx - x, qy - y, qw - w, qh - h].iter().all(|d| d.abs() < 0.05) => {}
            otra => fallos.push(format!("caja {k}: Chromium {:?} · QUIMERA {otra:?}", (x, y, w, h))),
        }
    }
    assert!(fallos.is_empty(), "{} de {} distintas:\n{}", fallos.len(), CAJAS.len(), fallos.join("\n"));
}

#[test]
fn misma_caja_encima_que_chromium() {
    let v = cajas(&maquetar_pagina());
    for &(x, y, esperada) in ENCIMA {
        // La última que se dibuja en ese punto es la que se ve.
        let encima = v.iter().rev().find(|c| x >= c.1 && x < c.1 + c.3 && y >= c.2 && y < c.2 + c.4).map(|c| c.0);
        assert_eq!(encima, Some(esperada), "en ({x}, {y})");
    }
}

#[test]
fn el_documento_crece_hasta_lo_posicionado() {
    let doc = quimera_html::analizar("<div style='position:absolute;top:1500px;height:100px'></div>");
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0);
    assert_eq!(m.alto, 600.0, "sin fondo ni borde no dibuja nada: no cuenta");
    let doc = quimera_html::analizar("<div style='position:absolute;top:1500px;height:100px;background:red'></div>");
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0);
    assert_eq!(m.alto, 1600.0);
    let _ = Color::NEGRO;
}
