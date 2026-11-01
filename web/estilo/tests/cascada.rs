//! La cascada da los mismos valores calculados que un navegador.

use quimera_estilo::{
    Alinear, Color, Display, Espacios, Estilo, Estilos, Generica, Medida, Opciones, TipoLista, calcular,
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
