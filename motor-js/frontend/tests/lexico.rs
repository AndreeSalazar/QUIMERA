//! Léxico: números, cadenas, signos, comentarios y saltos de línea.

use quimera_frontend::lexico::{Tipo, tokens};

fn tipos(s: &str) -> Vec<Tipo> {
    tokens(s).unwrap().into_iter().map(|t| t.tipo).collect()
}

fn numero(s: &str) -> f64 {
    match &tipos(s)[0] {
        Tipo::Numero(n) => *n,
        otro => panic!("{s}: {otro:?}"),
    }
}

#[test]
fn numeros_de_todas_las_formas() {
    assert_eq!(numero("42"), 42.0);
    assert_eq!(numero("10_000_000"), 1e7);
    assert_eq!(numero("0xFF"), 255.0);
    assert_eq!(numero("0b1010"), 10.0);
    assert_eq!(numero("0o17"), 15.0);
    assert_eq!(numero(".5"), 0.5);
    assert_eq!(numero("1.5e3"), 1500.0);
    assert_eq!(numero("2E-2"), 0.02);
    assert!(tokens("1__0").is_err(), "separador doble");
    assert!(tokens("10_").is_err(), "separador al final");
    assert!(tokens("3in").is_err(), "número pegado a un nombre");
    assert!(tokens("12n").is_err(), "BigInt todavía no");
}

#[test]
fn cadenas_con_escapes() {
    let c = |s: &str| match &tipos(s)[0] {
        Tipo::Cadena(c) => c.clone(),
        otro => panic!("{otro:?}"),
    };
    assert_eq!(c(r#""a\nb\t\"c\"""#), "a\nb\t\"c\"");
    assert_eq!(c(r"'\x41B\u{43}'"), "ABC");
    assert_eq!(c(r"'😀'"), "😀", "par sustituto");
    assert_eq!(c("'línea\\\nsigue'"), "líneasigue", "continuación de línea");
    assert!(tokens("'sin cerrar").is_err());
    assert!(tokens("'a\nb'").is_err());
}

#[test]
fn signos_mas_largos_primero_y_palabras() {
    assert_eq!(tipos("a >>>= b"), [Tipo::Ident("a".into()), Tipo::Signo(">>>="), Tipo::Ident("b".into()), Tipo::Fin]);
    assert_eq!(tipos("x?.y")[1], Tipo::Signo("?."));
    assert_eq!(tipos("c?.5:1")[1], Tipo::Signo("?"), "?. seguido de un dígito es ? y un número");
    assert_eq!(tipos("let of")[..2], [Tipo::Clave("let"), Tipo::Ident("of".into())]);
    assert_eq!(tipos("año")[0], Tipo::Ident("año".into()), "identificadores Unicode");
}

#[test]
fn comentarios_y_saltos_de_linea() {
    let t = tokens("a // uno\n/* dos\ntres */ b /* c */ d").unwrap();
    let saltos: Vec<(String, bool, u32)> = t
        .iter()
        .filter_map(|t| match &t.tipo {
            Tipo::Ident(n) => Some((n.clone(), t.salto_antes, t.pos.linea)),
            _ => None,
        })
        .collect();
    assert_eq!(saltos, [("a".into(), false, 1), ("b".into(), true, 3), ("d".into(), false, 3)]);
    assert!(tokens("/* sin cerrar").is_err());
}

#[test]
fn errores_con_posicion() {
    let e = tokens("let x = 1;\nlet y = `plantilla`;").unwrap_err();
    assert_eq!((e.pos.linea, e.pos.columna), (2, 9));
    assert!(e.mensaje.contains("plantillas"));
}
