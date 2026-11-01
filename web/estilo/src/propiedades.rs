//! Propiedades: los atajos (`margin`, `border`, `font`...) se expanden a propiedades largas, y
//! cada propiedad larga se calcula sobre el estilo del elemento.
//!
//! Un valor que no se entiende se ignora y la propiedad conserva lo que tenía (como en CSS,
//! donde una declaración no válida no cuenta).

use crate::valores::{Alinear, AltoLinea, Borde, Display, Espacios, Estilo, EstiloBorde, Generica, Medida, TipoLista};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_css::{Color, Longitud, Token, color, longitud, piezas};

/// Lo que hace falta para pasar unidades a píxeles.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Contexto {
    /// `font-size` del elemento (para `em`).
    pub letra: f32,
    /// `font-size` del padre (para `font-size: 1.2em`).
    pub letra_padre: f32,
    /// `font-size` de la raíz (para `rem`).
    pub letra_raiz: f32,
    /// Ancho y alto de la ventana (para `vw`, `vh`).
    pub ventana: (f32, f32),
}

const LADOS: [&str; 4] = ["top", "right", "bottom", "left"];

fn ident(t: &[Token]) -> Option<String> {
    match t {
        [Token::Ident(x)] => Some(x.to_ascii_lowercase()),
        _ => None,
    }
}

/// 1 a 4 valores → arriba, derecha, abajo, izquierda (como `margin: 1px 2px`).
fn cuatro<'a>(p: &[&'a [Token]]) -> Option<[&'a [Token]; 4]> {
    Some(match p {
        [a] => [a, a, a, a],
        [a, b] => [a, b, a, b],
        [a, b, c] => [a, b, c, b],
        [a, b, c, d] => [a, b, c, d],
        _ => return None,
    })
}

/// Un atajo → sus propiedades largas, en orden. Una propiedad larga se devuelve tal cual.
pub(crate) fn expandir(propiedad: &str, valor: &[Token]) -> Vec<(String, Vec<Token>)> {
    let p = piezas(valor);
    let largo = |n: String, v: &[Token]| (n, v.to_vec());
    let mut r = Vec::new();
    match propiedad {
        "margin" | "padding" => {
            if let Some(c) = cuatro(&p) {
                for (lado, v) in LADOS.iter().zip(c) {
                    r.push(largo(alloc::format!("{propiedad}-{lado}"), v));
                }
            }
        }
        "border-width" | "border-style" | "border-color" => {
            let tipo = &propiedad[7..];
            if let Some(c) = cuatro(&p) {
                for (lado, v) in LADOS.iter().zip(c) {
                    r.push(largo(alloc::format!("border-{lado}-{tipo}"), v));
                }
            }
        }
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
            let lados: Vec<&str> = if propiedad == "border" { LADOS.to_vec() } else { alloc::vec![&propiedad[7..]] };
            r.extend(expandir_borde(&p, &lados));
        }
        "background" => {
            // Solo el color; el resto (imágenes, posición) todavía no.
            let c = p
                .iter()
                .find(|x| color(x).is_some())
                .map_or_else(|| alloc::vec![Token::Ident("transparent".into())], |x| x.to_vec());
            r.push((String::from("background-color"), c));
        }
        "font" => r.extend(expandir_letra(&p)),
        "list-style" => {
            if let Some(t) = p.iter().find(|x| ident(x).is_some_and(|n| tipo_lista(&n).is_some())) {
                r.push(largo(String::from("list-style-type"), t));
            }
        }
        "text-decoration-line" => r.push(largo(String::from("text-decoration"), valor)),
        _ => r.push(largo(String::from(propiedad), valor)),
    }
    r
}

fn expandir_borde(p: &[&[Token]], lados: &[&str]) -> Vec<(String, Vec<Token>)> {
    // Lo que no se dice vuelve a su valor inicial: medium, none, el color del texto.
    let (mut ancho, mut estilo, mut col) = (
        alloc::vec![Token::Ident("medium".into())],
        alloc::vec![Token::Ident("none".into())],
        alloc::vec![Token::Ident("currentcolor".into())],
    );
    for pieza in p {
        if ident(pieza).is_some_and(|n| estilo_borde(&n).is_some()) {
            estilo = pieza.to_vec();
        } else if color(pieza).is_some() || ident(pieza).as_deref() == Some("currentcolor") {
            col = pieza.to_vec();
        } else {
            ancho = pieza.to_vec();
        }
    }
    let mut r = Vec::new();
    for lado in lados {
        r.push((alloc::format!("border-{lado}-width"), ancho.clone()));
        r.push((alloc::format!("border-{lado}-style"), estilo.clone()));
        r.push((alloc::format!("border-{lado}-color"), col.clone()));
    }
    r
}

/// `font: italic bold 12px/1.5 "Fira Sans", sans-serif`.
fn expandir_letra(p: &[&[Token]]) -> Vec<(String, Vec<Token>)> {
    let mut r = Vec::new();
    let (mut estilo, mut peso) =
        (alloc::vec![Token::Ident("normal".into())], alloc::vec![Token::Ident("normal".into())]);
    for (i, pieza) in p.iter().enumerate() {
        let n = ident(pieza);
        match n.as_deref() {
            Some("italic" | "oblique") => estilo = pieza.to_vec(),
            Some("bold" | "bolder" | "lighter") => peso = pieza.to_vec(),
            Some("normal" | "small-caps") => {}
            _ if matches!(pieza, [Token::Numero(_)]) => peso = pieza.to_vec(),
            _ => {
                // El tamaño (quizá con `/alto-de-línea`) y después la familia.
                let barra = pieza.iter().position(|t| *t == Token::Delim('/'));
                let (tam, alto) = match barra {
                    Some(b) => (&pieza[..b], Some(&pieza[b + 1..])),
                    None => (*pieza, None),
                };
                r.push((String::from("font-size"), tam.to_vec()));
                let alto = alto
                    .filter(|a| !a.is_empty())
                    .map_or_else(|| alloc::vec![Token::Ident("normal".into())], <[Token]>::to_vec);
                r.push((String::from("line-height"), alto));
                let familia: Vec<Token> = p[i + 1..].join(&Token::Blanco);
                if !familia.is_empty() {
                    r.push((String::from("font-family"), familia));
                }
                break;
            }
        }
    }
    r.push((String::from("font-style"), estilo));
    r.push((String::from("font-weight"), peso));
    r
}

fn estilo_borde(n: &str) -> Option<EstiloBorde> {
    Some(match n {
        "none" | "hidden" => EstiloBorde::Ninguno,
        "solid" | "inset" | "outset" | "groove" | "ridge" | "double" => EstiloBorde::Solido,
        "dashed" => EstiloBorde::Discontinuo,
        "dotted" => EstiloBorde::Punteado,
        _ => return None,
    })
}

fn tipo_lista(n: &str) -> Option<TipoLista> {
    Some(match n {
        "disc" => TipoLista::Disco,
        "circle" => TipoLista::Circulo,
        "square" => TipoLista::Cuadrado,
        "decimal" | "decimal-leading-zero" => TipoLista::Decimal,
        "lower-alpha" | "lower-latin" => TipoLista::Letra,
        "upper-alpha" | "upper-latin" => TipoLista::LetraMayuscula,
        "lower-roman" | "upper-roman" => TipoLista::Romano,
        "none" => TipoLista::Ninguno,
        _ => return None,
    })
}

/// Una longitud en píxeles (o porcentaje / auto).
pub(crate) fn medida(v: &[Token], c: &Contexto) -> Option<Medida> {
    let [t] = v else { return None };
    Some(match longitud(t)? {
        Longitud::Px(x) => Medida::Px(x),
        Longitud::Em(x) => Medida::Px(x * c.letra),
        Longitud::Rem(x) => Medida::Px(x * c.letra_raiz),
        Longitud::Vw(x) => Medida::Px(x * c.ventana.0 / 100.0),
        Longitud::Vh(x) => Medida::Px(x * c.ventana.1 / 100.0),
        Longitud::Porcentaje(p) => Medida::Porcentaje(p),
        Longitud::Auto => Medida::Auto,
    })
}

/// `font-size` en píxeles (relativo a la letra del padre).
pub(crate) fn tam_letra(v: &[Token], c: &Contexto) -> Option<f32> {
    if let Some(n) = ident(v) {
        return Some(match n.as_str() {
            "xx-small" => 9.0,
            "x-small" => 10.0,
            "small" => 13.0,
            "medium" => 16.0,
            "large" => 18.0,
            "x-large" => 24.0,
            "xx-large" => 32.0,
            "xxx-large" => 48.0,
            "smaller" => c.letra_padre / 1.2,
            "larger" => c.letra_padre * 1.2,
            _ => return None,
        });
    }
    let padre = Contexto { letra: c.letra_padre, ..*c };
    match medida(v, &padre)? {
        Medida::Px(x) if x >= 0.0 => Some(x),
        Medida::Porcentaje(p) => Some(c.letra_padre * p / 100.0),
        _ => None,
    }
}

/// Copia una propiedad del padre (`inherit`).
fn heredar(e: &mut Estilo, padre: &Estilo, prop: &str) {
    aplicar_campo(e, padre, prop);
}

/// Pone en `e` el campo de `prop` que tiene `de`.
fn aplicar_campo(e: &mut Estilo, de: &Estilo, prop: &str) {
    match prop {
        "display" => e.display = de.display,
        "color" => e.color = de.color,
        "background-color" => e.fondo = de.fondo,
        "font-weight" => e.peso = de.peso,
        "font-style" => e.cursiva = de.cursiva,
        "font-family" => {
            e.familias = de.familias.clone();
            e.generica = de.generica;
        }
        "text-align" => e.alinear = de.alinear,
        "line-height" => e.alto_linea = de.alto_linea,
        "white-space" => e.espacios = de.espacios,
        "text-decoration" => (e.subrayado, e.tachado) = (de.subrayado, de.tachado),
        "width" => e.ancho = de.ancho,
        "height" => e.alto = de.alto,
        "max-width" => e.ancho_max = de.ancho_max,
        "min-height" => e.alto_min = de.alto_min,
        "list-style-type" => e.lista = de.lista,
        "visibility" => e.visible = de.visible,
        "text-transform" => e.transformar = de.transformar,
        p => {
            if let Some((i, campo)) = lado_de(p) {
                match campo {
                    "margin" => e.margen[i] = de.margen[i],
                    "padding" => e.relleno[i] = de.relleno[i],
                    "width" => e.borde[i].ancho = de.borde[i].ancho,
                    "style" => e.borde[i].estilo = de.borde[i].estilo,
                    _ => e.borde[i].color = de.borde[i].color,
                }
            }
        }
    }
}

/// `margin-top` → (0, "margin"); `border-left-color` → (3, "color").
fn lado_de(p: &str) -> Option<(usize, &str)> {
    for (i, lado) in LADOS.iter().enumerate() {
        if let Some(antes) = p.strip_suffix(lado).and_then(|x| x.strip_suffix('-')) {
            return matches!(antes, "margin" | "padding").then_some((i, antes));
        }
        if let Some(campo) =
            p.strip_prefix("border-").and_then(|x| x.strip_prefix(lado)).and_then(|x| x.strip_prefix('-'))
        {
            return Some((i, campo));
        }
    }
    None
}

/// Calcula una propiedad larga sobre `e` (con su `font-size` ya calculado).
pub(crate) fn aplicar(e: &mut Estilo, padre: &Estilo, prop: &str, v: &[Token], c: &Contexto) {
    match ident(v).as_deref() {
        Some("inherit") => return heredar(e, padre, prop),
        Some("initial") => return aplicar_campo(e, &Estilo::default(), prop),
        Some("unset") => return heredar(e, padre, prop), // las heredables; aproximación
        _ => {}
    }
    let n = ident(v);
    match prop {
        "display" => {
            if let Some(d) = n.as_deref().and_then(display) {
                e.display = d;
            }
        }
        "color" => e.color = valor_color(v, padre.color).unwrap_or(e.color),
        "background-color" => e.fondo = valor_color(v, e.color).unwrap_or(e.fondo),
        "font-weight" => e.peso = peso(v, padre.peso).unwrap_or(e.peso),
        "font-style" => e.cursiva = matches!(n.as_deref(), Some("italic" | "oblique")),
        "font-family" => familias(e, v),
        "text-align" => e.alinear = n.as_deref().and_then(alinear).unwrap_or(e.alinear),
        "line-height" => e.alto_linea = alto_linea(v, c).unwrap_or(e.alto_linea),
        "white-space" => e.espacios = n.as_deref().and_then(espacios).unwrap_or(e.espacios),
        "text-decoration" => {
            let p = piezas(v);
            let tiene = |k: &str| p.iter().any(|x| ident(x).as_deref() == Some(k));
            (e.subrayado, e.tachado) = (tiene("underline"), tiene("line-through"));
        }
        "width" => e.ancho = medida(v, c).unwrap_or(e.ancho),
        "height" => e.alto = medida(v, c).unwrap_or(e.alto),
        "max-width" => {
            e.ancho_max = if n.as_deref() == Some("none") { Medida::Auto } else { medida(v, c).unwrap_or(e.ancho_max) }
        }
        "min-height" => e.alto_min = medida(v, c).unwrap_or(e.alto_min),
        "list-style-type" => e.lista = n.as_deref().and_then(tipo_lista).unwrap_or(e.lista),
        "visibility" => e.visible = n.as_deref() != Some("hidden") && n.as_deref() != Some("collapse"),
        "text-transform" => {
            e.transformar = match n.as_deref() {
                Some("uppercase") => Some(true),
                Some("lowercase") => Some(false),
                _ => None,
            }
        }
        p => aplicar_lado(e, p, v, c),
    }
}

fn aplicar_lado(e: &mut Estilo, p: &str, v: &[Token], c: &Contexto) {
    let Some((i, campo)) = lado_de(p) else { return };
    match campo {
        "margin" => e.margen[i] = medida(v, c).unwrap_or(e.margen[i]),
        "padding" => e.relleno[i] = medida(v, c).filter(|m| *m != Medida::Auto).unwrap_or(e.relleno[i]),
        "width" => {
            let ancho = match ident(v).as_deref() {
                Some("thin") => Some(1.0),
                Some("medium") => Some(3.0),
                Some("thick") => Some(5.0),
                _ => match medida(v, c) {
                    Some(Medida::Px(x)) if x >= 0.0 => Some(x),
                    _ => None,
                },
            };
            e.borde[i].ancho = ancho.unwrap_or(e.borde[i].ancho);
        }
        "style" => e.borde[i].estilo = ident(v).as_deref().and_then(estilo_borde).unwrap_or(e.borde[i].estilo),
        "color" => {
            if ident(v).as_deref() == Some("currentcolor") {
                e.borde[i].color = None;
            } else if let Some(col) = color(v) {
                e.borde[i].color = Some(col);
            }
        }
        _ => {}
    }
}

/// Si el valor vale para la propiedad. Lo que no vale no entra en la cascada (como en CSS: una
/// declaración no válida se descarta y gana la anterior).
pub(crate) fn valida(prop: &str, v: &[Token]) -> bool {
    let c = Contexto { letra: 16.0, letra_padre: 16.0, letra_raiz: 16.0, ventana: (800.0, 600.0) };
    let n = ident(v);
    let n = n.as_deref();
    if matches!(n, Some("inherit" | "initial" | "unset")) {
        return true;
    }
    match prop {
        "display" => n.and_then(display).is_some(),
        "color" | "background-color" => valor_color(v, Color::NEGRO).is_some(),
        "font-size" => tam_letra(v, &c).is_some(),
        "font-weight" => peso(v, 400).is_some(),
        "font-style" => matches!(n, Some("normal" | "italic" | "oblique")),
        "font-family" => !v.is_empty(),
        "text-align" => n.and_then(alinear).is_some(),
        "line-height" => alto_linea(v, &c).is_some(),
        "white-space" => n.and_then(espacios).is_some(),
        "text-decoration" => !v.is_empty(),
        "width" | "height" | "min-height" => medida(v, &c).is_some(),
        "max-width" => n == Some("none") || medida(v, &c).is_some(),
        "list-style-type" => n.and_then(tipo_lista).is_some(),
        "visibility" => matches!(n, Some("visible" | "hidden" | "collapse")),
        "text-transform" => matches!(n, Some("none" | "uppercase" | "lowercase" | "capitalize")),
        p => match lado_de(p) {
            Some((_, "margin")) => medida(v, &c).is_some(),
            Some((_, "padding")) => medida(v, &c).is_some_and(|m| m != Medida::Auto),
            Some((_, "width")) => {
                matches!(n, Some("thin" | "medium" | "thick"))
                    || matches!(medida(v, &c), Some(Medida::Px(x)) if x >= 0.0)
            }
            Some((_, "style")) => n.and_then(estilo_borde).is_some(),
            Some((_, "color")) => n == Some("currentcolor") || color(v).is_some(),
            _ => false, // propiedad que todavía no se calcula
        },
    }
}

fn valor_color(v: &[Token], actual: Color) -> Option<Color> {
    if ident(v).as_deref() == Some("currentcolor") {
        return Some(actual);
    }
    color(v)
}

fn display(n: &str) -> Option<Display> {
    Some(match n {
        "block" | "table" | "flex" | "grid" | "flow-root" | "table-row" | "table-cell" | "table-row-group"
        | "table-header-group" | "table-footer-group" | "table-caption" => Display::Bloque,
        "inline" | "contents" => Display::EnLinea,
        "inline-block" | "inline-flex" | "inline-grid" | "inline-table" => Display::BloqueEnLinea,
        "list-item" => Display::ElementoDeLista,
        "none" => Display::Ninguno,
        _ => return None,
    })
}

fn peso(v: &[Token], padre: u16) -> Option<u16> {
    Some(match v {
        [Token::Numero(n)] if (1.0..=1000.0).contains(n) => *n as u16,
        _ => match ident(v)?.as_str() {
            "normal" => 400,
            "bold" => 700,
            "bolder" => {
                if padre < 600 {
                    700
                } else {
                    900
                }
            }
            "lighter" => {
                if padre > 500 {
                    400
                } else {
                    100
                }
            }
            _ => return None,
        },
    })
}

fn familias(e: &mut Estilo, v: &[Token]) {
    let nombres: Vec<String> = v
        .split(|t| *t == Token::Coma)
        .map(|f| {
            f.iter()
                .filter_map(|t| match t {
                    Token::Ident(x) | Token::Cadena(x) => Some(x.as_str()),
                    _ => None,
                })
                .collect::<Vec<&str>>()
                .join(" ")
        })
        .filter(|n| !n.is_empty())
        .collect();
    if nombres.is_empty() {
        return;
    }
    let generica = nombres.iter().find_map(|n| match n.to_ascii_lowercase().as_str() {
        "serif" | "times" | "times new roman" | "georgia" => Some(Generica::Serif),
        "monospace" | "courier" | "courier new" | "consolas" | "menlo" | "monaco" => Some(Generica::Monoespaciada),
        "sans-serif" | "arial" | "helvetica" | "system-ui" | "verdana" | "segoe ui" => Some(Generica::SinSerif),
        n if n.contains("mono") || n.contains("code") => Some(Generica::Monoespaciada),
        _ => None,
    });
    // Una fuente con nombre que no conocemos suele ser sin serifa (la mayoría en la web).
    e.generica = generica.unwrap_or(Generica::SinSerif);
    e.familias = nombres;
}

fn alinear(n: &str) -> Option<Alinear> {
    Some(match n {
        "left" | "start" => Alinear::Izquierda,
        "right" | "end" => Alinear::Derecha,
        "center" | "-webkit-center" => Alinear::Centro,
        "justify" => Alinear::Justificar,
        _ => return None,
    })
}

fn espacios(n: &str) -> Option<Espacios> {
    Some(match n {
        "normal" => Espacios::Normal,
        "pre" => Espacios::Pre,
        "nowrap" => Espacios::SinSalto,
        "pre-wrap" | "break-spaces" => Espacios::PreAjustado,
        "pre-line" => Espacios::PreLinea,
        _ => return None,
    })
}

fn alto_linea(v: &[Token], c: &Contexto) -> Option<AltoLinea> {
    Some(match v {
        [Token::Numero(n)] if *n >= 0.0 => AltoLinea::Factor(*n as f32),
        _ if ident(v).as_deref() == Some("normal") => AltoLinea::Normal,
        _ => match medida(v, c)? {
            Medida::Px(x) => AltoLinea::Px(x),
            Medida::Porcentaje(p) => AltoLinea::Px(c.letra * p / 100.0),
            Medida::Auto => return None,
        },
    })
}

/// Un borde por defecto (para pruebas y para quien construya estilos a mano).
pub fn borde_solido(ancho: f32, color: Color) -> Borde {
    Borde { ancho, estilo: EstiloBorde::Solido, color: Some(color) }
}
