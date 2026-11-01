//! Cajas en línea (`inline-block`) y controles de formulario: las mismas posiciones que da
//! Chromium con `docs/ejemplos/controles.html` (sin texto, así las fuentes no influyen).

use quimera_estilo::{Color, Opciones, calcular};
use quimera_maquetacion::{Aproximado, Formulario, Maquetacion, Pieza, maquetar, maquetar_pagina};

const PAGINA: &str = include_str!("../../../docs/ejemplos/controles.html");

/// (k del fondo `rgb(k, 60, 120)`, x, y, ancho, alto) según Chromium.
const CAJAS: &[(u8, f32, f32, f32, f32)] = &[
    // 1: apoyadas en la línea base (su borde de abajo)
    (1, 8.0, 28.0, 50.0, 20.0),
    (2, 63.0, 8.0, 30.0, 40.0),
    (3, 98.0, 38.0, 40.0, 10.0),
    // 2: relleno, borde y un bloque dentro; encogida a su contenido
    (4, 8.0, 68.0, 74.0, 29.0),
    (5, 15.0, 75.0, 60.0, 15.0),
    (6, 82.0, 92.0, 20.0, 5.0),
    // 3: la tercera no cabe y baja
    (7, 8.0, 117.0, 90.0, 10.0),
    (8, 98.0, 117.0, 90.0, 10.0),
    (9, 8.0, 127.0, 90.0, 10.0),
    // 4: controles con medidas fijas (el área de texto, con su borde de 1px)
    (10, 8.0, 157.0, 110.0, 30.0),
    (11, 8.0, 191.0, 60.0, 24.0),
    (12, 8.0, 215.0, 82.0, 32.0),
    // 5: la caja que da alto a la línea de las casillas
    (13, 8.0, 267.0, 10.0, 30.0),
];

fn maquetar_casos() -> Maquetacion {
    let doc = quimera_html::analizar(PAGINA);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0)
}

#[test]
fn mismas_posiciones_que_chromium() {
    let m = maquetar_casos();
    let mut fallos = Vec::new();
    for &(k, x, y, w, h) in CAJAS {
        let q = m.piezas.iter().find_map(|p| match p {
            Pieza::Rect { x, y, ancho, alto, color } if *color == Color::rgb(k, 60, 120) => {
                Some((*x, *y, *ancho, *alto))
            }
            _ => None,
        });
        match q {
            Some(q) if [q.0 - x, q.1 - y, q.2 - w, q.3 - h].iter().all(|d| d.abs() < 0.05) => {}
            otra => fallos.push(format!("caja {k}: Chromium {:?} · QUIMERA {otra:?}", (x, y, w, h))),
        }
    }
    assert!(fallos.is_empty(), "{} de {} distintas:\n{}", fallos.len(), CAJAS.len(), fallos.join("\n"));
}

#[test]
fn casillas_con_sus_margenes() {
    // Chromium: la casilla en (22, 284) y el botón de opción en (43, 284), los dos de 13 × 13.
    let m = maquetar_casos();
    let casillas: Vec<(f32, f32, f32, f32)> =
        m.controles.iter().filter(|c| c.ancho == 13.0).map(|c| (c.x, c.y, c.ancho, c.alto)).collect();
    assert_eq!(casillas, [(22.0, 284.0, 13.0, 13.0), (43.0, 284.0, 13.0, 13.0)]);
}

/// Lo escrito y el foco los da la ventana.
struct Escrito;

impl Formulario for Escrito {
    fn valor(&self, _: quimera_html::NodoId) -> Option<String> {
        Some(String::from("hola"))
    }
    fn foco(&self) -> Option<quimera_html::NodoId> {
        Some(quimera_html::NodoId(4))
    }
}

#[test]
fn lo_escrito_ayuda_y_cursor() {
    let doc = quimera_html::analizar("<input placeholder='Buscar...'><input value='x'>");
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &Aproximado, 800.0, 600.0);
    assert_eq!(m.textos(), ["Buscar...", "x"], "el texto de ayuda y el value");
    assert_eq!(m.controles.len(), 2);
    let foco = m.controles[0].nodo;
    struct Foco(quimera_html::NodoId);
    impl Formulario for Foco {
        fn valor(&self, n: quimera_html::NodoId) -> Option<String> {
            (n == self.0).then(|| String::from("hola"))
        }
        fn foco(&self) -> Option<quimera_html::NodoId> {
            Some(self.0)
        }
    }
    let m = maquetar_pagina(&doc, &estilos, &Aproximado, (800.0, 600.0), &|_| None, &Foco(foco));
    assert_eq!(m.textos(), ["hola", "x"], "lo escrito");
    let cursor = m.piezas.iter().any(|p| matches!(p, Pieza::Rect { ancho, .. } if *ancho == 1.0));
    assert!(cursor, "el cursor, en el campo con el foco");
    let _ = Escrito;
}
