//! Flexbox: posiciones calculadas a mano con la especificación (como las dan los navegadores).

use quimera_estilo::{Color, Opciones, calcular};
use quimera_maquetacion::{Fuente, Maquetacion, MedirTexto, Pieza, maquetar};

/// Cada carácter mide medio `em` (8 px a 16 px): las cuentas se hacen a mano.
struct MedioEm;

impl MedirTexto for MedioEm {
    fn ancho(&self, texto: &str, f: &Fuente<'_>) -> f32 {
        texto.chars().count() as f32 * f.tam * 0.5
    }
}

fn maquetar_html(html: &str) -> Maquetacion {
    let html = format!("<style>body {{ margin: 0 }} .c > * {{ background: rgb(1, 2, 3) }}</style>{html}");
    let doc = quimera_html::analizar(&html);
    let estilos = calcular(&doc, &Opciones { ancho: 800.0, alto: 600.0 });
    maquetar(&doc, &estilos, &MedioEm, 800.0, 600.0)
}

/// Los rectángulos de fondo de los elementos flexibles: (x, y, ancho, alto).
fn cajas(m: &Maquetacion) -> Vec<(f32, f32, f32, f32)> {
    m.piezas
        .iter()
        .filter_map(|p| match p {
            Pieza::Rect { x, y, ancho, alto, color } if *color == Color::rgb(1, 2, 3) => Some((*x, *y, *ancho, *alto)),
            _ => None,
        })
        .collect()
}

fn cerca(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

fn xs(m: &Maquetacion) -> Vec<f32> {
    cajas(m).iter().map(|c| c.0).collect()
}

fn anchos(m: &Maquetacion) -> Vec<f32> {
    cajas(m).iter().map(|c| c.2).collect()
}

fn todos_cerca(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| cerca(*x, *y))
}

const TRES: &str = "<div style='width:100px'>a</div><div style='width:100px'>b</div><div style='width:100px'>c</div>";

#[test]
fn justify_content() {
    let caso =
        |j: &str| xs(&maquetar_html(&format!("<div class=c style='display:flex;justify-content:{j}'>{TRES}</div>")));
    assert!(todos_cerca(&caso("flex-start"), &[0.0, 100.0, 200.0]));
    assert!(todos_cerca(&caso("center"), &[250.0, 350.0, 450.0]));
    assert!(todos_cerca(&caso("flex-end"), &[500.0, 600.0, 700.0]));
    assert!(todos_cerca(&caso("space-between"), &[0.0, 350.0, 700.0]));
    // space-around: 500 libres, 500/6 a cada lado de cada uno.
    assert!(
        todos_cerca(&caso("space-around"), &[500.0 / 6.0, 350.0, 800.0 - 500.0 / 6.0 - 100.0]),
        "{:?}",
        caso("space-around")
    );
    assert!(todos_cerca(&caso("space-evenly"), &[125.0, 350.0, 575.0]));
    let m = maquetar_html(&format!("<div class=c style='display:flex;flex-direction:row-reverse'>{TRES}</div>"));
    assert!(todos_cerca(&xs(&m), &[700.0, 600.0, 500.0]), "{:?}", xs(&m));
}

#[test]
fn crecer_encoger_y_minimo() {
    // flex: 1 y flex: 2 con base 0: se reparten 800 en 1:2.
    let m =
        maquetar_html("<div class=c style='display:flex'><div style='flex:1'>a</div><div style='flex:2'>b</div></div>");
    assert!(todos_cerca(&anchos(&m), &[800.0 / 3.0, 1600.0 / 3.0]), "{:?}", anchos(&m));
    // Dos de 600 en 800: encogen igual (misma base y factor) → 400 cada uno.
    let m = maquetar_html(
        "<div class=c style='display:flex'><div style='width:600px'>a</div><div style='width:600px'>b</div></div>",
    );
    assert!(todos_cerca(&anchos(&m), &[400.0, 400.0]), "{:?}", anchos(&m));
    // Una palabra de 20 letras (160 px) no deja encoger su caja por debajo de 160.
    let m = maquetar_html(
        "<div class=c style='display:flex'><div>aaaaaaaaaaaaaaaaaaaa</div><div style='width:780px'>b</div></div>",
    );
    let a = anchos(&m);
    assert!(cerca(a[0], 160.0) && cerca(a[1], 640.0), "{a:?}");
    // flex-grow sin base: el espacio libre (800 - 8 - 8) va entero al que crece.
    let m = maquetar_html("<div class=c style='display:flex'><div>a</div><div style='flex-grow:1'>b</div></div>");
    assert!(todos_cerca(&anchos(&m), &[8.0, 792.0]), "{:?}", anchos(&m));
}

#[test]
fn margen_auto_gap_order_y_texto_suelto() {
    let m = maquetar_html(
        "<div class=c style='display:flex'><div style='width:100px'>logo</div><div style='width:100px;margin-left:auto'>menú</div></div>",
    );
    assert!(todos_cerca(&xs(&m), &[0.0, 700.0]), "margin-left:auto empuja a la derecha: {:?}", xs(&m));
    let m = maquetar_html(&format!("<div class=c style='display:flex;gap:10px'>{TRES}</div>"));
    assert!(todos_cerca(&xs(&m), &[0.0, 110.0, 220.0]), "{:?}", xs(&m));
    let m = maquetar_html(
        "<div class=c style='display:flex'><div style='width:100px'>a</div><div style='width:100px;order:-1'>b</div></div>",
    );
    let mut texto_x: Vec<(String, f32)> = m
        .piezas
        .iter()
        .filter_map(|p| match p {
            Pieza::Texto { texto, x, .. } => Some((texto.clone(), *x)),
            _ => None,
        })
        .collect();
    texto_x.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(texto_x, vec![(String::from("a"), 100.0), (String::from("b"), 0.0)], "order:-1 va primero");
    let m = maquetar_html("<div style='display:flex'>hola <b>mundo</b></div>");
    assert_eq!(m.textos(), ["hola", "mundo"], "texto suelto e <b> son elementos flexibles aparte");
}

#[test]
fn alinear_estirar_columnas_y_envolver() {
    // Contenedor de 200 de alto: center deja la línea de 19,2 px en medio; stretch la estira.
    let m = maquetar_html("<div class=c style='display:flex;height:200px;align-items:center'><div>a</div></div>");
    let c = cajas(&m)[0];
    assert!(cerca(c.1, (200.0 - 19.2) / 2.0) && cerca(c.3, 19.2), "{c:?}");
    let m = maquetar_html("<div class=c style='display:flex;height:200px'><div>a</div><div>b</div></div>");
    assert!(cajas(&m).iter().all(|c| cerca(c.3, 200.0)), "stretch: {:?}", cajas(&m));
    // Columna con gap: uno debajo de otro, 19,2 + 10.
    let m = maquetar_html(
        "<div class=c style='display:flex;flex-direction:column;gap:10px'><div>a</div><div>b</div></div>",
    );
    let ys: Vec<f32> = cajas(&m).iter().map(|c| c.1).collect();
    assert!(todos_cerca(&ys, &[0.0, 29.2]) && cajas(&m).iter().all(|c| cerca(c.2, 800.0)), "{:?}", cajas(&m));
    // Columna centrada: cada elemento mide su contenido y va en medio.
    let m = maquetar_html(
        "<div class=c style='display:flex;flex-direction:column;align-items:center'><div>abcd</div></div>",
    );
    let c = cajas(&m)[0];
    assert!(cerca(c.2, 32.0) && cerca(c.0, 384.0), "{c:?}");
    // Envolver: cinco de 300 en 800 → 2, 2, 1.
    let cinco = "<div style='width:300px'>a</div>".repeat(5);
    let m = maquetar_html(&format!("<div class=c style='display:flex;flex-wrap:wrap'>{cinco}</div>"));
    let pos: Vec<(f32, f32)> = cajas(&m).iter().map(|c| (c.0, c.1)).collect();
    assert_eq!(pos.len(), 5);
    assert!(
        cerca(pos[1].0, 300.0) && cerca(pos[2].0, 0.0) && cerca(pos[2].1, 19.2) && cerca(pos[4].1, 38.4),
        "{pos:?}"
    );
}

#[test]
fn min_width_y_max_width_de_los_elementos() {
    // Resultados de Chromium con los mismos contenedores de 800 px.
    let ancho = |html: &str| anchos(&maquetar_html(&format!("<div class=c style='display:flex'>{html}</div>")));
    let ancho_contenido = "<div style='width:1000px;height:10px'></div>";
    // Con `min-width: 0` encoge por debajo de su contenido.
    let a = ancho(&format!(
        "<div style='flex:1;min-width:0'>{ancho_contenido}</div><div style='width:200px;flex:none'></div>"
    ));
    assert!(todos_cerca(&a, &[600.0, 200.0]), "{a:?}");
    // Sin él, el contenido es el mínimo (y desborda).
    let a = ancho(&format!("<div style='flex:1'>{ancho_contenido}</div><div style='width:200px;flex:none'></div>"));
    assert!(todos_cerca(&a, &[1000.0, 200.0]), "{a:?}");
    // El que llega a su `max-width` se para y el otro se queda lo que sobra.
    let a = ancho("<div style='flex:1;max-width:300px;min-width:240px'>x</div><div style='flex:1'>y</div>");
    assert!(todos_cerca(&a, &[300.0, 500.0]), "{a:?}");
}
