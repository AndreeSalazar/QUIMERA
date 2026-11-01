//! El pintado sin GPU: cuadros y atlas a partir de una página.

use quimera_estilo::{Opciones, calcular};
use quimera_maquetacion::maquetar;
use quimera_pintar::Pintor;
use quimera_texto::Fuentes;

fn pagina() -> &'static str {
    "<style>body{background:#eee} p{color:#c00}</style><h1>Hola</h1><p>mundo mundo</p><img src=x width=20 height=10>"
}

#[test]
fn la_pagina_se_vuelve_cuadros_y_letras() {
    let fuentes = Fuentes::del_sistema();
    if fuentes.total() == 0 {
        return; // sin fuentes en esta máquina no hay letras que probar
    }
    let mut pintor = Pintor::nuevo(fuentes, 1024);
    let doc = quimera_html::analizar(pagina());
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &pintor.medidor(), 800.0, 600.0);
    let cuadros = pintor.escena(&m, 2.0);
    let letras = cuadros.iter().filter(|c| c.letra.is_some()).count();
    assert_eq!(letras, "Holamundomundo".len(), "una letra por carácter visible (los espacios no tienen píxeles)");
    assert!(cuadros.iter().any(|c| c.letra.is_some() && c.color == [0.8, 0.0, 0.0, 1.0]), "el texto rojo");
    assert!(cuadros.iter().any(|c| c.letra.is_none() && c.ancho == 40.0 && c.alto == 20.0), "la imagen escalada ×2");
    let (_, desde, hasta) = pintor.atlas_cambiado().expect("el atlas tiene letras nuevas");
    assert!(hasta > desde);
    assert!(pintor.atlas_cambiado().is_none(), "ya se entregó");
    // La misma página otra vez: las letras ya están en el atlas.
    let otra = pintor.escena(&m, 2.0);
    assert_eq!(otra.len(), cuadros.len());
    assert!(pintor.atlas_cambiado().is_none(), "nada nuevo que subir");
}

#[test]
fn un_atlas_diminuto_se_vacia_y_vuelve_a_llenar() {
    let fuentes = Fuentes::del_sistema();
    if fuentes.total() == 0 {
        return;
    }
    let mut pintor = Pintor::nuevo(fuentes, 64); // cabe poco
    let doc = quimera_html::analizar("<p style='font-size:30px'>abcdefghijklmnopqrstuvwxyz</p>");
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    let m = maquetar(&doc, &estilos, &pintor.medidor(), 800.0, 600.0);
    let cuadros = pintor.escena(&m, 1.0);
    assert!(!cuadros.is_empty(), "no se rompe aunque no quepa todo");
}
