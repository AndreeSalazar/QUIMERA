//! Ver una página con QUIMERA antes de tener ventana: HTML → árbol → estilo → maquetación →
//! una imagen SVG con la lista de dibujo.
//!
//! Uso: `cargo run --release -p quimera-ver -- pagina.html [salida.svg] [ancho]`
//! Las hojas `<link rel="stylesheet">` locales (relativas a la página) se cargan.
//!
//! Cada texto lleva el ancho que midió QUIMERA (`textLength`): el visor de SVG usa sus propias
//! fuentes, pero la imagen respeta la maquetación de QUIMERA.

use quimera_estilo::{Color, Generica, Opciones};
use quimera_maquetacion::{Aproximado, Maquetacion, Pieza, maquetar};
use std::fmt::Write as _;
use std::time::Instant;

fn color(c: Color) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("rgba({},{},{},{:.3})", c.r, c.g, c.b, f32::from(c.a) / 255.0)
    }
}

fn escapar(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn familia(f: Option<&String>, g: Generica) -> String {
    let generica = match g {
        Generica::Serif => "serif",
        Generica::SinSerif => "sans-serif",
        Generica::Monoespaciada => "monospace",
    };
    match f {
        Some(f) if f != generica => format!("'{}', {generica}", escapar(f)),
        _ => String::from(generica),
    }
}

/// La lista de dibujo como SVG.
fn svg(m: &Maquetacion) -> String {
    let mut s = String::new();
    let (w, h) = (m.ancho, m.alto);
    let _ = writeln!(s, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#);
    let _ = writeln!(s, r#"<rect width="100%" height="100%" fill="{}"/>"#, color(m.fondo));
    for p in &m.piezas {
        match p {
            Pieza::Rect { x, y, ancho, alto, color: c } => {
                let _ = writeln!(
                    s,
                    r#"<rect x="{x:.2}" y="{y:.2}" width="{ancho:.2}" height="{alto:.2}" fill="{}"/>"#,
                    color(*c)
                );
            }
            Pieza::Texto { x, base, ancho, texto, tam, color: c, negrita, cursiva, generica, familia: f } => {
                let ajuste = if *ancho > 0.0 && texto.trim() != "" {
                    format!(r#" textLength="{ancho:.2}" lengthAdjust="spacingAndGlyphs""#)
                } else {
                    String::new()
                };
                let _ = writeln!(
                    s,
                    r#"<text x="{x:.2}" y="{base:.2}" font-size="{tam:.2}" font-family="{}" font-weight="{}" font-style="{}" fill="{}" xml:space="preserve"{ajuste}>{}</text>"#,
                    familia(f.as_ref(), *generica),
                    if *negrita { "bold" } else { "normal" },
                    if *cursiva { "italic" } else { "normal" },
                    color(*c),
                    escapar(texto)
                );
            }
            Pieza::Imagen { x, y, ancho, alto, src } => {
                let _ = writeln!(
                    s,
                    r##"<rect x="{x:.2}" y="{y:.2}" width="{ancho:.2}" height="{alto:.2}" fill="#eeeeee" stroke="#999999"><title>{}</title></rect>"##,
                    escapar(src)
                );
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(entrada) = args.next() else {
        eprintln!("uso: quimera-ver pagina.html [salida.svg] [ancho]");
        std::process::exit(2);
    };
    let salida = args.next().unwrap_or_else(|| format!("{}.svg", entrada.trim_end_matches(".html")));
    let ancho: f32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(800.0);
    let html = match std::fs::read_to_string(&entrada) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("no se puede leer {entrada}: {e}");
            std::process::exit(1);
        }
    };
    let t = Instant::now();
    let doc = quimera_html::analizar(&html);
    let t_html = t.elapsed();
    let t = Instant::now();
    // Las hojas `<link rel="stylesheet">` locales, relativas a la página.
    let carpeta = std::path::Path::new(&entrada).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let externa = |href: &str| {
        let ruta = href.split(['?', '#']).next().unwrap_or(href);
        std::fs::read_to_string(carpeta.join(ruta)).ok()
    };
    let estilos = quimera_estilo::calcular_con_hojas(&doc, &Opciones { ancho, alto: 600.0 }, &externa);
    let t_estilo = t.elapsed();
    let t = Instant::now();
    let m = maquetar(&doc, &estilos, &Aproximado, ancho, 600.0);
    let t_maq = t.elapsed();
    if let Err(e) = std::fs::write(&salida, svg(&m)) {
        eprintln!("no se puede escribir {salida}: {e}");
        std::process::exit(1);
    }
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;
    println!(
        "{entrada}: {} nodos, {} piezas, {:.0}×{:.0} px → {salida}\n  HTML {:.2} ms · estilo {:.2} ms · maquetación {:.2} ms",
        doc.total(),
        m.piezas.len(),
        m.ancho,
        m.alto,
        ms(t_html),
        ms(t_estilo),
        ms(t_maq)
    );
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn la_pagina_se_vuelve_svg() {
        let doc = quimera_html::analizar(
            "<body style='background:#123456'><p style='color:red'>a &lt; b & \"c\"</p><img src=x width=10 height=5>",
        );
        let estilos = quimera_estilo::calcular(&doc, &super::Opciones { ancho: 300.0, alto: 200.0 });
        let m = super::maquetar(&doc, &estilos, &super::Aproximado, 300.0, 200.0);
        let svg = super::svg(&m);
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains(r##"fill="#123456""##), "fondo del lienzo");
        assert!(svg.contains("a &lt; b &amp; &quot;c&quot;"), "texto escapado");
        assert!(svg.contains(r##"fill="#ff0000""##) && svg.contains("textLength="), "color y ancho medido");
        assert!(svg.contains("<title>x</title>"), "la imagen como recuadro");
    }
}
