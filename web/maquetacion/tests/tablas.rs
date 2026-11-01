//! Tablas: las mismas posiciones que da Chromium con `docs/ejemplos/tablas.html`.
//!
//! La página solo tiene cajas de tamaño fijo (sin texto), así que las fuentes no influyen. Los
//! números esperados salen de Chromium (`getBoundingClientRect` en una ventana de 800 × 600) y
//! se comparan con el fondo de cada caja en la lista de dibujo de QUIMERA.

use quimera_estilo::{Color, Opciones, calcular};
use quimera_maquetacion::{Aproximado, Maquetacion, Pieza, maquetar};

const PAGINA: &str = include_str!("../../../docs/ejemplos/tablas.html");

/// Un color de fondo (r, g, b).
type Rgb = (u8, u8, u8);

/// (color del fondo, x, y, ancho, alto) según Chromium.
const CHROMIUM: &[(Rgb, f32, f32, f32, f32)] = &[
    // 1: dos por dos, espaciado de 2px, relleno de 1px, contenido en medio
    ((1, 200, 100), 8.0, 8.0, 190.0, 60.0),
    ((2, 200, 100), 10.0, 10.0, 82.0, 42.0),
    ((3, 200, 100), 11.0, 21.0, 50.0, 20.0),
    ((4, 200, 100), 94.0, 10.0, 102.0, 42.0),
    ((5, 200, 100), 95.0, 11.0, 100.0, 40.0),
    ((6, 200, 100), 10.0, 54.0, 82.0, 12.0),
    ((7, 200, 100), 11.0, 55.0, 80.0, 10.0),
    ((8, 200, 100), 94.0, 54.0, 102.0, 12.0),
    // 2: bordes que colapsan (2px y 4px)
    ((9, 200, 100), 8.0, 78.0, 82.0, 42.0),
    ((10, 200, 100), 9.0, 80.0, 39.0, 38.0),
    ((11, 200, 100), 13.0, 84.0, 30.0, 30.0),
    ((12, 200, 100), 48.0, 80.0, 40.0, 38.0),
    ((13, 200, 100), 53.0, 94.0, 30.0, 10.0),
    // 3: tabla de 300px: lo que sobra, en proporción
    ((14, 200, 100), 8.0, 130.0, 300.0, 16.0),
    ((15, 200, 100), 10.0, 132.0, 74.9375, 12.0),
    ((16, 200, 100), 86.9375, 132.0, 219.0625, 12.0),
    // 4: colspan más ancho que sus columnas
    ((17, 200, 100), 8.0, 156.0, 206.0, 30.0),
    ((18, 200, 100), 10.0, 158.0, 68.75, 12.0),
    ((19, 200, 100), 80.75, 158.0, 131.25, 12.0),
    ((20, 200, 100), 10.0, 172.0, 202.0, 12.0),
    // 5: rowspan, contenido en medio
    ((21, 200, 100), 8.0, 196.0, 70.0, 70.0),
    ((22, 200, 100), 10.0, 198.0, 32.0, 66.0),
    ((23, 200, 100), 11.0, 226.0, 30.0, 10.0),
    ((24, 200, 100), 44.0, 198.0, 32.0, 32.0),
    ((25, 200, 100), 44.0, 232.0, 32.0, 32.0),
    // 6: vertical-align top y bottom
    ((26, 200, 100), 10.0, 278.0, 22.0, 52.0),
    ((27, 200, 100), 11.0, 279.0, 20.0, 10.0),
    ((28, 200, 100), 34.0, 278.0, 22.0, 52.0),
    ((29, 200, 100), 35.0, 319.0, 20.0, 10.0),
    // 7: una columna al 25 %
    ((30, 200, 100), 8.0, 342.0, 100.0, 12.0),
    ((31, 200, 100), 108.0, 342.0, 300.0, 12.0),
    // 8: border=1 cellspacing=5 cellpadding=4 bgcolor
    ((1, 2, 3), 8.0, 364.0, 117.0, 32.0),
    ((32, 200, 100), 14.0, 370.0, 50.0, 20.0),
    ((33, 200, 100), 69.0, 370.0, 50.0, 20.0),
    // 9: tabla al 100 % con una columna de 100px
    ((34, 200, 100), 8.0, 406.0, 102.0, 12.0),
    ((35, 200, 100), 110.0, 406.0, 682.0, 12.0),
    // 10: centrada; thead, tbody y tfoot en su orden aunque el HTML los traiga al revés
    ((36, 200, 100), 367.0, 428.0, 66.0, 74.0),
    ((37, 200, 100), 369.0, 488.0, 62.0, 12.0),
    ((38, 200, 100), 369.0, 464.0, 62.0, 22.0),
    ((39, 200, 100), 369.0, 430.0, 62.0, 32.0),
];

fn maquetar_pagina() -> Maquetacion {
    let doc = quimera_html::analizar(PAGINA);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0)
}

fn caja(m: &Maquetacion, (r, g, b): Rgb) -> Option<(f32, f32, f32, f32)> {
    m.piezas.iter().find_map(|p| match p {
        Pieza::Rect { x, y, ancho, alto, color } if *color == Color::rgb(r, g, b) => Some((*x, *y, *ancho, *alto)),
        _ => None,
    })
}

#[test]
fn mismas_posiciones_que_chromium() {
    let m = maquetar_pagina();
    let mut fallos = Vec::new();
    for &(color, x, y, w, h) in CHROMIUM {
        match caja(&m, color) {
            // Chromium redondea a 1/64 de píxel.
            Some(q) if [q.0 - x, q.1 - y, q.2 - w, q.3 - h].iter().all(|d| d.abs() < 0.05) => {}
            otra => fallos.push(format!("{color:?}: Chromium {:?} · QUIMERA {otra:?}", (x, y, w, h))),
        }
    }
    assert!(fallos.is_empty(), "{} de {} distintas:\n{}", fallos.len(), CHROMIUM.len(), fallos.join("\n"));
}

#[test]
fn center_centra_la_tabla_pero_no_su_texto() {
    // Sin doctype (como Hacker News): la tabla de 200px va en medio de los 784px del body.
    let doc = quimera_html::analizar(
        "<center><table width=200 style='background:rgb(1,2,3)'><tr><td>hola</td></tr></table></center>",
    );
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0);
    let (x, _, w, _) = caja(&m, (1, 2, 3)).expect("la tabla");
    assert_eq!((x, w), (300.0, 200.0));
    let texto = m.piezas.iter().find_map(|p| match p {
        Pieza::Texto { x, .. } => Some(*x),
        _ => None,
    });
    assert_eq!(texto, Some(303.0), "el texto, a la izquierda de su celda (2px de espacio + 1px de relleno)");
}

#[test]
fn bordes_que_colapsan_se_dibujan_en_las_lineas() {
    let m = maquetar_pagina();
    // La línea entre las dos celdas del caso 2: 4px de ancho, centrada en x = 48.
    let linea = m.piezas.iter().any(|p| {
        matches!(p, Pieza::Rect { x, ancho, alto, color, .. }
            if *color == Color::NEGRO && (*x - 46.0).abs() < 0.01 && *ancho == 4.0 && *alto > 30.0)
    });
    assert!(linea, "falta la línea de 4px entre las celdas");
}
