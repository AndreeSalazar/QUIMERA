//! La cascada da los mismos valores calculados que un navegador.

use quimera_estilo::{
    Alinear, AlinearVertical, Color, Display, Espacios, Estilo, Estilos, Flotar, Generica, Limpiar, Medida, Opciones,
    Posicion, TipoLista, calcular,
};
use quimera_html::{Documento, analizar};

fn estilos(html: &str) -> (Documento, Estilos) {
    let doc = analizar(html);
    let e = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    (doc, e)
}

/// El estilo del elemento con este id.
fn de<'a>(doc: &Documento, e: &'a Estilos, id: &str) -> &'a Estilo {
    let nodo = doc.descendientes(Documento::RAIZ).find(|&n| doc.elemento(n).and_then(|x| x.atributo("id")) == Some(id));
    e.de(nodo.unwrap_or_else(|| panic!("no hay #{id}"))).unwrap_or_else(|| panic!("#{id} sin estilo"))
}

const ROJO: Color = Color::rgb(255, 0, 0);
const AZUL: Color = Color::rgb(0, 0, 255);

#[test]
fn hoja_del_navegador() {
    let (doc, e) = estilos(
        "<h1 id=t>T</h1><p id=p>x <b id=b>y</b> <a id=a href=#>z</a></p><ul><li id=li>i</li></ul><pre id=pre>c</pre>",
    );
    let body = e.de(doc.buscar("body").unwrap()).unwrap();
    assert_eq!(body.margen, [Medida::Px(8.0); 4]);
    let h1 = de(&doc, &e, "t");
    assert_eq!((h1.display, h1.tam_letra, h1.peso), (Display::Bloque, 32.0, 700));
    assert!(matches!(h1.margen[0], Medida::Px(m) if (m - 21.44).abs() < 0.01), "0.67em de 32px");
    assert_eq!(de(&doc, &e, "p").margen[0], Medida::Px(16.0));
    assert_eq!((de(&doc, &e, "b").display, de(&doc, &e, "b").peso), (Display::EnLinea, 700));
    let a = de(&doc, &e, "a");
    assert_eq!((a.color, a.subrayado), (Color::rgb(0, 0, 0xee), true));
    let li = de(&doc, &e, "li");
    assert_eq!((li.display, li.lista), (Display::ElementoDeLista, TipoLista::Disco));
    let pre = de(&doc, &e, "pre");
    assert_eq!((pre.espacios, pre.generica), (Espacios::Pre, Generica::Monoespaciada));
    assert!(e.de(doc.buscar("head").unwrap()).is_some_and(|h| h.display == Display::Ninguno));
}

#[test]
fn orden_de_la_cascada() {
    let (doc, e) = estilos(
        "<style>
           #x { color: red }        p { color: blue }
           .a { color: green !important } .b { color: blue }
           p.c { color: blue } p.c { color: red }
           p.d { color: red !important }
         </style>
         <p id=x>id gana a etiqueta</p>
         <p id=y class='a b'>important gana</p>
         <p id=z class=c>a igualdad, la última</p>
         <p id=w class=d style='color: blue'>important gana al style</p>
         <p id=v style='color: red' class=c>style gana a selectores</p>",
    );
    assert_eq!(de(&doc, &e, "x").color, ROJO);
    assert_eq!(de(&doc, &e, "y").color, Color::rgb(0, 128, 0));
    assert_eq!(de(&doc, &e, "z").color, ROJO);
    assert_eq!(de(&doc, &e, "w").color, ROJO);
    assert_eq!(de(&doc, &e, "v").color, ROJO);
}

#[test]
fn herencia_y_palabras_clave() {
    let (doc, e) = estilos(
        "<style>
           div { color: red; margin: 10px; text-align: center; font-family: 'Fira Sans', sans-serif }
           .hereda { margin: inherit } .inicial { color: initial } .actual { border: 2px solid currentcolor }
         </style>
         <div><p id=p>p</p><p id=h class=hereda>h</p><p id=i class=inicial>i</p><span id=s class=actual>s</span></div>",
    );
    let p = de(&doc, &e, "p");
    assert_eq!((p.color, p.alinear, p.generica), (ROJO, Alinear::Centro, Generica::SinSerif));
    assert_eq!(p.familias, ["Fira Sans", "sans-serif"]);
    assert_eq!(p.margen[0], Medida::Px(16.0), "margin no se hereda (el de <p> del navegador)");
    assert_eq!(de(&doc, &e, "h").margen, [Medida::Px(10.0); 4]);
    assert_eq!(de(&doc, &e, "i").color, Color::NEGRO);
    let s = de(&doc, &e, "s");
    assert_eq!((s.borde[0].ocupa(), s.color_borde(0)), (2.0, ROJO));
}

#[test]
fn atajos_y_unidades() {
    let (doc, e) = estilos(
        "<style>
           html { font-size: 20px }
           #m { margin: 1px 2px 3px; padding: 1em 5% ; width: 50vw; max-width: none }
           #f { font: italic bold 12px/1.5 Georgia, serif }
           #b { border: 3px dashed blue; border-left: none; background: #ff0 url(x.png) }
           #r { font-size: 2rem; line-height: 2 }  #c { font-size: 150% } #k { font-size: larger }
         </style>
         <div id=m>m</div><p id=f>f</p><div id=b>b</div><p id=r>r</p><div id=c>c<span id=k>k</span></div>",
    );
    let m = de(&doc, &e, "m");
    assert_eq!(m.margen, [Medida::Px(1.0), Medida::Px(2.0), Medida::Px(3.0), Medida::Px(2.0)]);
    assert_eq!(m.relleno, [Medida::Px(20.0), Medida::Porcentaje(5.0), Medida::Px(20.0), Medida::Porcentaje(5.0)]);
    assert_eq!((m.ancho, m.ancho_max), (Medida::Px(400.0), Medida::Auto));
    let f = de(&doc, &e, "f");
    assert_eq!(
        (f.cursiva, f.peso, f.tam_letra, f.alto_linea_px(), f.generica),
        (true, 700, 12.0, 18.0, Generica::Serif)
    );
    let b = de(&doc, &e, "b");
    assert_eq!(
        (b.borde[0].ocupa(), b.borde[3].ocupa(), b.color_borde(1), b.fondo),
        (3.0, 0.0, AZUL, Color::rgb(255, 255, 0))
    );
    let r = de(&doc, &e, "r");
    assert_eq!((r.tam_letra, r.alto_linea_px()), (40.0, 80.0));
    assert_eq!(de(&doc, &e, "c").tam_letra, 30.0);
    assert_eq!(de(&doc, &e, "k").tam_letra, 36.0);
}

#[test]
fn media_y_reglas_malas() {
    let (doc, e) = estilos(
        "<style>
           p { color: red }
           @media (max-width: 600px) { p { color: blue } }
           @media screen and (min-width: 700px) { p { text-align: right } }
           @media print { p { display: none } }
           p { color: notacolor; margin: 1px 2px 3px 4px 5px; width: calc(10px + 1em) }
         </style><p id=p>p</p>",
    );
    let p = de(&doc, &e, "p");
    assert_eq!((p.color, p.alinear, p.display), (ROJO, Alinear::Derecha, Display::Bloque));
    assert_eq!((p.margen[1], p.ancho), (Medida::Px(0.0), Medida::Auto), "lo que no se entiende no cuenta");
}

#[test]
fn hojas_externas_en_orden_de_documento() {
    let doc = analizar(
        "<link rel=stylesheet href=a.css><style>p { color: blue }</style><link rel='alternate stylesheet' href=b.css><p id=p>x",
    );
    let externa = |href: &str| (href == "a.css").then(|| String::from("p { color: red; font-size: 20px }"));
    let e = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho: 800.0, alto: 600.0 }, &externa);
    let p = de(&doc, &e, "p");
    assert_eq!((p.color, p.tam_letra), (AZUL, 20.0), "el <style> va después del <link>: gana en color");
}

#[test]
fn propiedades_de_flexbox() {
    use quimera_estilo::{AlinearFlex, Direccion, Justificar};
    let (doc, e) = estilos(
        "<style>
           #c { display: flex; flex-flow: column-reverse wrap; justify-content: space-between; align-items: center; gap: 8px 2em }
           #a { flex: 2 3 10%; align-self: flex-end; order: -1 }
           #b { flex: auto } #n { flex: none } #u { flex: 1 } #l { flex: 100px }
         </style>
         <div id=c><p id=a>a</p><p id=b>b</p><p id=n>n</p><p id=u>u</p><p id=l>l</p></div>",
    );
    let c = de(&doc, &e, "c");
    assert_eq!(c.display, Display::Flex);
    assert_eq!(
        (c.flex.direccion, c.flex.envolver, c.flex.justificar),
        (Direccion::ColumnaInversa, true, Justificar::Entre)
    );
    assert_eq!(
        (c.flex.alinear_items, c.flex.hueco_filas, c.flex.hueco_columnas),
        (AlinearFlex::Centro, Medida::Px(8.0), Medida::Px(32.0))
    );
    let a = de(&doc, &e, "a");
    assert_eq!(
        (a.flex.crecer, a.flex.encoger, a.flex.base, a.flex.alinear_propio, a.flex.orden),
        (2.0, 3.0, Medida::Porcentaje(10.0), Some(AlinearFlex::Fin), -1)
    );
    let f = |id: &str| {
        let x = de(&doc, &e, id).flex;
        (x.crecer, x.encoger, x.base)
    };
    assert_eq!(f("b"), (1.0, 1.0, Medida::Auto));
    assert_eq!(f("n"), (0.0, 0.0, Medida::Auto));
    assert_eq!(f("u"), (1.0, 1.0, Medida::Porcentaje(0.0)));
    assert_eq!(f("l"), (1.0, 1.0, Medida::Px(100.0)));
}

#[test]
fn tablas_en_la_hoja_del_navegador() {
    let (doc, e) = estilos(
        "<!doctype html><table id=t><tbody id=b><tr id=r><td id=d valign=top>1<th id=h>2</table>\
         <table id=v border=2 cellspacing=7 cellpadding=5 style='border-collapse:collapse'><tr><td id=c>3</table>",
    );
    assert_eq!(de(&doc, &e, "t").display, Display::Tabla);
    assert_eq!(de(&doc, &e, "t").tabla.espaciado, (2.0, 2.0));
    assert_eq!(de(&doc, &e, "b").display, Display::GrupoFilas);
    assert_eq!(de(&doc, &e, "r").display, Display::Fila);
    assert_eq!(de(&doc, &e, "h").display, Display::Celda);
    // tbody { vertical-align: middle }; tr y td lo heredan; valign gana a la hoja.
    assert_eq!(de(&doc, &e, "h").tabla.vertical, AlinearVertical::Medio);
    assert_eq!(de(&doc, &e, "d").tabla.vertical, AlinearVertical::Arriba);
    assert_eq!((de(&doc, &e, "h").peso, de(&doc, &e, "h").alinear), (700, Alinear::Centro));
    // Atributos antiguos: border, cellspacing, cellpadding (que la celda toma de su tabla).
    let (v, c) = (de(&doc, &e, "v"), de(&doc, &e, "c"));
    assert_eq!((v.borde[0].ancho, v.tabla.espaciado, v.tabla.colapsar), (2.0, (7.0, 7.0), true));
    assert_eq!((c.relleno[0], c.borde[0].ancho), (Medida::Px(5.0), 1.0));
    assert!(c.tabla.colapsar, "border-collapse se hereda");
}

#[test]
fn modo_de_compatibilidad_sin_doctype() {
    // Sin doctype, <center><table> centra la tabla pero no su texto (como en Chromium).
    let pagina = "<center><table><tr><td id=d>1</table></center>";
    let (doc, e) = estilos(pagina);
    assert_eq!(de(&doc, &e, "d").alinear, Alinear::Izquierda);
    let (doc, e) = estilos(&format!("<!DOCTYPE html>{pagina}"));
    assert_eq!(de(&doc, &e, "d").alinear, Alinear::Centro);
    // <center> y align=center centran también los bloques de dentro; text-align: center, no.
    let (doc, e) = estilos("<center id=c>1</center><div id=a align=center>2</div><p id=p style='text-align:center'>3");
    assert!(de(&doc, &e, "c").centrar_bloques && de(&doc, &e, "a").centrar_bloques);
    assert!(!de(&doc, &e, "p").centrar_bloques);
}

#[test]
fn float_y_clear() {
    let (doc, e) = estilos(
        "<span id=s style='float:right'>1</span><p id=p style='clear:both'>2</p>\
         <img id=i src=a align=left hspace=5><td id=t style='float:left'>",
    );
    let s = de(&doc, &e, "s");
    assert_eq!((s.flotar, s.display), (Flotar::Derecha, Display::Bloque), "un flotante es un bloque");
    assert_eq!(de(&doc, &e, "p").limpiar, Limpiar::Ambos);
    let i = de(&doc, &e, "i");
    assert_eq!((i.flotar, i.margen[1], i.margen[3]), (Flotar::Izquierda, Medida::Px(5.0), Medida::Px(5.0)));
}

#[test]
fn position_inset_y_z_index() {
    let (doc, e) = estilos(
        "<span id=a style='position:absolute;float:left;inset:1px 2px 3px;z-index:5'>1</span>\
         <div id=r style='position:relative;left:10%;top:-4px;z-index:auto'>2</div>",
    );
    let a = de(&doc, &e, "a");
    assert_eq!((a.colocacion.posicion, a.display, a.flotar), (Posicion::Absoluta, Display::Bloque, Flotar::No));
    assert_eq!(a.colocacion.desplazar, [Medida::Px(1.0), Medida::Px(2.0), Medida::Px(3.0), Medida::Px(2.0)]);
    assert_eq!(a.colocacion.z, Some(5));
    let r = de(&doc, &e, "r");
    assert_eq!((r.colocacion.posicion, r.colocacion.z), (Posicion::Relativa, None));
    assert_eq!((r.colocacion.desplazar[0], r.colocacion.desplazar[3]), (Medida::Px(-4.0), Medida::Porcentaje(10.0)));
}

#[test]
fn variables_css() {
    let (doc, e) = estilos(
        "<style>:root { --fondo: #00f; --hueco: 6px; --borde: 2px solid var(--fondo) }
         .tarjeta { --hueco: 10px; padding: var(--hueco) 0; border: var(--borde); color: var(--nada, red) }
         .mala { color: blue; color: var(--no-existe) }</style>
         <div id=d class=tarjeta><p id=p style='margin-left: var(--hueco)'>x</p></div><p id=m class=mala>y</p>",
    );
    let (d, p, m) = (de(&doc, &e, "d"), de(&doc, &e, "p"), de(&doc, &e, "m"));
    assert_eq!((d.relleno[0], d.relleno[1]), (Medida::Px(10.0), Medida::Px(0.0)), "atajo con var()");
    assert_eq!((d.borde[3].ancho, d.borde[3].color), (2.0, Some(AZUL)), "var() dentro de una variable");
    assert_eq!(d.color, ROJO, "valor de reserva");
    assert_eq!(p.margen[3], Medida::Px(10.0), "se hereda la del padre más cercano");
    // Una variable que no existe deja la propiedad en `unset` (color se hereda: negro), no en `blue`.
    assert_eq!(m.color, Color::NEGRO);
    // `unset` en una propiedad que no se hereda es su valor inicial, no el del padre.
    let (doc, e) = estilos(
        "<div style='display:flex;padding:9px'><span id=s style='display:var(--no);padding:var(--no)'>z</span></div>",
    );
    let s = de(&doc, &e, "s");
    assert_eq!((s.display, s.relleno[0]), (Display::EnLinea, Medida::Px(0.0)));
}

#[test]
fn box_sizing() {
    let (doc, e) = estilos(
        "<style>* { box-sizing: border-box }</style>
         <div id=a style='width:100px;height:50px;padding:10px;border:5px solid'></div>
         <div id=b style='width:50%;padding:10px;box-sizing:content-box'></div><button id=c></button>",
    );
    let (a, b, c) = (de(&doc, &e, "a"), de(&doc, &e, "b"), de(&doc, &e, "c"));
    assert_eq!((a.ancho, a.alto), (Medida::Px(70.0), Medida::Px(20.0)), "100 - 2×10 - 2×5");
    assert!(a.caja_borde && !b.caja_borde && c.caja_borde, "los botones, border-box por defecto");
    assert_eq!(b.ancho, Medida::Porcentaje(50.0));
}
