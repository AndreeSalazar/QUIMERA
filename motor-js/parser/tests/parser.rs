//! Parser: precedencia, `;` automático, flechas, objetos y errores claros.

use quimera_parser::analizar;
use quimera_parser::ast::{Clave, Expr, Sent};

fn expr(s: &str) -> Expr {
    match analizar(s).unwrap().remove(0) {
        Sent::Expr(e) => e,
        otra => panic!("{otra:?}"),
    }
}

fn b(e: Expr) -> Box<Expr> {
    Box::new(e)
}

fn n(x: f64) -> Expr {
    Expr::Numero(x)
}

fn id(s: &str) -> Expr {
    Expr::Ident(s.into())
}

#[test]
fn precedencia_y_asociatividad() {
    assert_eq!(expr("1 + 2 * 3"), Expr::Binaria("+", b(n(1.0)), b(Expr::Binaria("*", b(n(2.0)), b(n(3.0))))));
    assert_eq!(expr("2 ** 3 ** 2"), Expr::Binaria("**", b(n(2.0)), b(Expr::Binaria("**", b(n(3.0)), b(n(2.0))))));
    assert_eq!(expr("a = b = c"), Expr::Asignar("=", b(id("a")), b(Expr::Asignar("=", b(id("b")), b(id("c"))))));
    assert_eq!(expr("a || b && c"), Expr::Logica("||", b(id("a")), b(Expr::Logica("&&", b(id("b")), b(id("c"))))));
    assert!(matches!(expr("a ? b : c ? d : e"), Expr::Condicional(_, _, c) if matches!(*c, Expr::Condicional(..))));
    assert!(analizar("-a ** 2").is_err(), "una unaria delante de ** necesita paréntesis");
}

#[test]
fn llamadas_miembros_y_new() {
    assert_eq!(
        expr("a.b(c)[d]"),
        Expr::Indice(b(Expr::Llamar(b(Expr::Miembro(b(id("a")), "b".into())), vec![id("c")])), b(id("d")))
    );
    assert_eq!(expr("new A.B(1)"), Expr::Nuevo(b(Expr::Miembro(b(id("A")), "B".into())), vec![n(1.0)]));
    assert_eq!(expr("new A"), Expr::Nuevo(b(id("A")), vec![]));
    assert_eq!(expr("o.default"), Expr::Miembro(b(id("o")), "default".into()), "palabra reservada como propiedad");
}

#[test]
fn punto_y_coma_automatico() {
    let p = analizar("let a = 1\nlet b = 2\nreturn\na").unwrap();
    assert_eq!(p.len(), 4);
    assert_eq!(p[2], Sent::Retornar(None), "return seguido de salto de línea no devuelve nada");
    assert!(analizar("let a = 1 let b = 2").is_err(), "sin salto de línea hace falta ;");
    let p = analizar("a\n++b").unwrap();
    assert!(
        matches!(&p[1], Sent::Expr(Expr::Actualizar { prefijo: true, .. })),
        "++ tras un salto va con lo siguiente"
    );
}

#[test]
fn flechas_y_objetos() {
    let Expr::Funcion(f) = expr("(a, b = 2) => a + b") else { panic!() };
    assert!(f.flecha);
    assert_eq!(f.params.len(), 2);
    assert_eq!(f.cuerpo, vec![Sent::Retornar(Some(Expr::Binaria("+", b(id("a")), b(id("b")))))]);
    let Expr::Asignar(_, _, o) = expr("o = {a, 'b': 1, 2: x, [k]: y, m() { return 1 }}") else { panic!() };
    let Expr::Objeto(props) = *o else { panic!() };
    let claves: Vec<String> = props
        .iter()
        .map(|(k, _)| match k {
            Clave::Nombre(n) => n.clone(),
            Clave::Calculada(_) => "[k]".into(),
        })
        .collect();
    assert_eq!(claves, ["a", "b", "2", "[k]", "m"]);
    assert_eq!(props[0].1, id("a"), "{{a}} es {{a: a}}");
}

#[test]
fn sentencias_de_control() {
    let p = analizar(
        "for (let i = 0; i < 3; i++) { if (i) continue; else break }\n\
         do x--; while (x)\n\
         switch (v) { case 1: f(); default: g() }\n\
         try { h() } catch (e) { throw e }\n\
         for (const k in o) {}",
    )
    .unwrap();
    assert!(matches!(p[0], Sent::Para { .. }));
    assert!(matches!(p[1], Sent::Hacer(..)));
    assert!(matches!(&p[2], Sent::Elegir(_, casos) if casos.len() == 2 && casos[1].0.is_none()));
    assert!(matches!(&p[3], Sent::Intentar { captura: Some((Some(e), _)), .. } if e == "e"));
    assert!(matches!(&p[4], Sent::ParaEn { de: false, .. }));
}

#[test]
fn lo_que_falta_avisa_con_claridad() {
    for (codigo, pista) in [
        ("class A { async *m() {} }", "asíncronos"),
        ("a: for (;;) break a;", "etiquetas"),
        ("async function* g() {}", "asíncronos"),
    ] {
        let e = analizar(codigo).unwrap_err();
        assert!(e.mensaje.contains(pista), "{codigo}: {}", e.mensaje);
    }
}

#[test]
fn barra_es_division_o_expresion_regular_segun_lo_anterior() {
    let re = |p: &str, b: &str| Expr::Regex(p.to_string(), b.to_string());
    assert_eq!(expr("/a[/]b/gi"), re("a[/]b", "gi"));
    assert_eq!(expr(r"x = /\//"), Expr::Asignar("=", b(id("x")), b(re(r"\/", ""))));
    assert!(matches!(expr("a / b / c"), Expr::Binaria("/", ..)));
    assert!(matches!(expr("(a) / 2"), Expr::Binaria("/", ..)));
    assert!(matches!(expr("x[0] /= 2"), Expr::Asignar("/=", ..)));
    assert!(matches!(expr("f(/,/)"), Expr::Llamar(..)));
    let e = analizar("x = /(/").unwrap_err();
    assert!(e.mensaje.contains("expresión regular no válida"), "{}", e.mensaje);
    assert!(analizar("x = /abc").unwrap_err().mensaje.contains("sin cerrar"));
}
