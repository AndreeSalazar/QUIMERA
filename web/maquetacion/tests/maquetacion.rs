//! Posiciones calculadas a mano con las reglas de CSS 2.1 (las mismas que dan los navegadores).

use quimera_estilo::{Color, Opciones, calcular};
use quimera_maquetacion::{Fuente, Maquetacion, MedirTexto, Pieza, maquetar};

/// Cada carácter mide medio `em`: así las cuentas se pueden hacer a mano.
struct MedioEm;

impl MedirTexto for MedioEm {
    fn ancho(&self, texto: &str, f: &Fuente<'_>) -> f32 {
        texto.chars().count() as f32 * f.tam * 0.5
    }
}

fn maquetar_html(html: &str) -> Maquetacion {
    let doc = quimera_html::analizar(html);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar(&doc, &estilos, &MedioEm, 800.0, 600.0)
}

/// (texto, x, línea base) de cada trozo de texto.
fn textos(m: &Maquetacion) -> Vec<(String, f32, f32)> {
    m.piezas
        .iter()
        .filter_map(|p| match p {
            Pieza::Texto { texto, x, base, .. } => Some((texto.clone(), *x, *base)),
            _ => None,
        })
        .collect()
}

fn cerca(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn texto_en(m: &Maquetacion, t: &str) -> (f32, f32) {
    let (_, x, b) =
        textos(m).into_iter().find(|(x, _, _)| x == t).unwrap_or_else(|| panic!("no está {t:?}: {:?}", textos(m)));
    (x, b)
}

#[test]
fn margenes_que_colapsan() {
    // <p> tiene 16px de margen y <body> 8px sin borde: el de <p> sale por arriba (16, no 24).
    // Línea normal de 16px: 19,2px; base = arriba + 1,6 (medio interlineado) + 12,8 (ascenso).
    let m = maquetar_html("<p>a</p><p>b</p>");
    let (x, base) = texto_en(&m, "a");
    assert!(cerca(x, 8.0) && cerca(base, 16.0 + 14.4), "{x} {base}");
    // Entre dos <p>: 16px (el mayor), no 32.
    let (_, base_b) = texto_en(&m, "b");
    assert!(cerca(base_b, 16.0 + 19.2 + 16.0 + 14.4), "{base_b}");
    // <h1>: 2em y margen de .67em → 21,44px por arriba; línea de 38,4px.
    let m = maquetar_html("<h1>T</h1>");
    let (_, base) = texto_en(&m, "T");
    assert!(cerca(base, 21.44 + 3.2 + 25.6), "{base}");
}

#[test]
fn anchos_centrado_bordes_y_relleno() {
    let m = maquetar_html(
        "<style>body{margin:0} div{width:200px;margin:0 auto;border:2px solid red;padding:10px}</style><div>x</div>",
    );
    let (x, _) = texto_en(&m, "x");
    assert!(cerca(x, 288.0 + 2.0 + 10.0), "centrado en 800: (800-224)/2 = 288, más borde y relleno; {x}");
    let rojos: Vec<&Pieza> =
        m.piezas.iter().filter(|p| matches!(p, Pieza::Rect { color, .. } if *color == Color::rgb(255, 0, 0))).collect();
    assert_eq!(rojos.len(), 4, "cuatro lados de borde");
    let m = maquetar_html("<style>div{max-width:100px;margin:auto}</style><div>y</div>");
    let (x, _) = texto_en(&m, "y");
    assert!(cerca(x, 8.0 + (784.0 - 100.0) / 2.0), "max-width + margin auto centra: {x}");
}

#[test]
fn texto_partido_en_lineas_y_alineado() {
    // 16px: cada letra 8px, el espacio 8px. "aaaa aaaa aaaa" = 112px no cabe en 100.
    let m = maquetar_html("<div style='width:100px'>aaaa aaaa aaaa</div>");
    let t = textos(&m);
    assert_eq!(t.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["aaaa aaaa", "aaaa"]);
    assert!(cerca(t[1].2 - t[0].2, 19.2), "una línea más abajo");
    let m =
        maquetar_html("<p style='width:100px;text-align:center'>ab</p><p style='width:100px;text-align:right'>cd</p>");
    assert!(cerca(texto_en(&m, "ab").0, 8.0 + 42.0));
    assert!(cerca(texto_en(&m, "cd").0, 8.0 + 84.0));
}

#[test]
fn blancos_estilos_en_linea_y_br() {
    let m = maquetar_html("<p>  uno   <b>dos</b>\n tres<br>cuatro</p>");
    let t = textos(&m);
    let solo: Vec<&str> = t.iter().map(|x| x.0.as_str()).collect();
    assert_eq!(solo, ["uno ", "dos", " tres", "cuatro"], "blancos juntados, un trozo por estilo, <br> parte");
    assert!(cerca(t[1].1, 8.0 + 32.0), "dos empieza tras 'uno ' (4 letras de 8px)");
    assert!(t[3].2 > t[2].2, "cuatro va en la línea siguiente");
    let m = maquetar_html("<pre>a  b\n  c</pre>");
    assert_eq!(m.textos(), ["a  b", "  c"], "pre respeta blancos y saltos");
}

#[test]
fn listas_ocultos_fondos_e_imagenes() {
    let m = maquetar_html("<ul><li>uno<li>dos</ul><ol start=3><li>tres</ol><div style='display:none'>no</div>");
    let t = m.textos();
    assert!(t.contains(&"• ") && t.contains(&"3. ") && !t.contains(&"no"), "{t:?}");
    let (x_marca, _) = texto_en(&m, "• ");
    assert!(cerca(x_marca, 8.0 + 40.0 - 16.0), "el marcador queda a la izquierda del contenido");
    let m = maquetar_html("<div style='background:yellow'>x</div><img src=a.png width=50 height=20>");
    let i_fondo = m
        .piezas
        .iter()
        .position(|p| matches!(p, Pieza::Rect { color, .. } if *color == Color::rgb(255, 255, 0)))
        .unwrap();
    let i_texto = m.piezas.iter().position(|p| matches!(p, Pieza::Texto { .. })).unwrap();
    assert!(i_fondo < i_texto, "el fondo se pinta debajo del texto");
    assert!(m.piezas.iter().any(|p| matches!(p, Pieza::Imagen { ancho, alto, .. } if *ancho == 50.0 && *alto == 20.0)));
    let m = maquetar_html("<style>body { background: #102030 }</style>");
    assert_eq!(m.fondo, Color::rgb(0x10, 0x20, 0x30), "el fondo de body pinta el lienzo");
}
