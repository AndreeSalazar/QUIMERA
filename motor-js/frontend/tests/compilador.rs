//! Compilador: registros sin copias, entornos para lo capturado, saltos y globales.

use quimera_frontend::bytecode::{Instr, OpBin};
use quimera_frontend::compilar;

/// El código de la función `n` del Stencil.
fn codigo(fuente: &str, n: usize) -> Vec<Instr> {
    compilar(fuente).unwrap().funciones[n].codigo.clone()
}

#[test]
fn una_variable_local_se_actualiza_en_su_registro() {
    // function f(a) { let x = a; x += 1; return x }  →  x vive en r1, sin copias.
    let c = codigo("function f(a) { let x = a; x += 1; return x }", 1);
    assert_eq!(
        c,
        [
            Instr::Mover { dst: 1, src: 0 },
            Instr::BinariaInm { op: OpBin::Suma, dst: 1, a: 1, valor: 1 },
            Instr::Retornar { src: 1 },
            Instr::Indefinido { dst: 2 },
            Instr::Retornar { src: 2 },
        ]
    );
}

#[test]
fn lo_capturado_va_al_entorno_y_lo_demas_a_registros() {
    let s = compilar("function f() { let x = 1; let y = 2; return () => x + 1 }").unwrap();
    let f = &s.funciones[1];
    assert_eq!(f.entorno, 1, "solo x está capturada");
    assert_eq!(f.codigo[0], Instr::CrearEntorno { huecos: 1 });
    assert!(f.codigo.contains(&Instr::EscribirEntorno { saltos: 0, hueco: 0, src: 1 }), "{s}");
    assert!(f.codigo.contains(&Instr::Entero { dst: 0, valor: 2 }), "y vive en r0: {s}");
    let flecha = &s.funciones[2];
    assert!(flecha.flecha);
    assert!(flecha.codigo.iter().any(|i| matches!(i, Instr::LeerEntorno { saltos: 0, hueco: 0, .. })), "{s}");
}

#[test]
fn los_saltos_cuentan_solo_las_funciones_con_entorno() {
    // outer tiene entorno (x), middle también (y), inner no: x está 1 salto fuera de inner.
    let s = compilar(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/datos/basic-closure.js")).unwrap(),
    )
    .unwrap();
    let texto = s.to_string();
    assert!(texto.contains("entorno[1][0]") && texto.contains("entorno[0][0]"), "{texto}");
}

#[test]
fn las_globales_y_los_parametros_por_defecto() {
    let s = compilar("var total = 0; function suma(a, b = 10) { total = total + a + b }").unwrap();
    let texto = s.to_string();
    assert!(texto.contains("global \"total\""), "{texto}");
    let suma = &s.funciones[1];
    assert_eq!(suma.params, 2);
    assert!(suma.codigo.iter().any(|i| matches!(i, Instr::SaltarSiNo { .. })), "comprueba si b es undefined");
}

#[test]
fn bucles_saltan_a_su_sitio() {
    let c =
        codigo("function f(n) { let s = 0; for (let i = 0; i < n; i++) { if (i == 3) continue; s += i } return s }", 1);
    // Todos los saltos apuntan dentro del código.
    for i in &c {
        if let Instr::Saltar { destino } | Instr::SaltarSi { destino, .. } | Instr::SaltarSiNo { destino, .. } = i {
            assert!((*destino as usize) < c.len(), "salto fuera: {i:?}");
            assert_ne!(*destino, 0, "salto sin parchear: {i:?}");
        }
    }
}

#[test]
fn las_funciones_declaradas_existen_desde_el_principio() {
    let c = codigo("f(); function f() {}", 0);
    assert!(matches!(c[0], Instr::Cierre { funcion: 1, .. }), "el cierre se crea antes de llamar: {c:?}");
}

#[test]
fn typeof_de_una_global_inexistente_no_falla() {
    let s = compilar("var t = typeof noExiste").unwrap();
    assert!(s.to_string().contains("typeof global \"noExiste\""));
}

#[test]
fn break_fuera_de_un_bucle_es_un_error() {
    assert!(compilar("break").unwrap_err().mensaje.contains("fuera de un bucle"));
    assert!(compilar("switch (1) { default: continue }").unwrap_err().mensaje.contains("fuera de un bucle"));
}

#[test]
fn salir_de_un_try_cierra_su_manejador_y_ejecuta_su_finally() {
    // `break` dentro del try: FinIntentar, luego el finally (f()), luego el salto.
    let s = compilar("while (1) { try { break } finally { f() } }").unwrap().to_string();
    let fin = s.find("fin try").expect("FinIntentar");
    assert!(
        s[fin..].contains("global \"f\""),
        "finally en línea tras FinIntentar:
{s}"
    );
    assert!(compilar("for (const k in o) {} for (const v of a) {}").is_ok());
}

#[test]
fn los_argumentos_quedan_en_registros_seguidos_aunque_sean_complejos() {
    // Un arreglo y una llamada como argumentos usan temporales propios: no deben separar los argumentos.
    let s = compilar("f(1, [2, g(3)], 4)").unwrap();
    let llamada_f = s.funciones[0]
        .codigo
        .iter()
        .rev()
        .find_map(|i| match i {
            Instr::Llamar { args, n: 3, .. } => Some(*args),
            _ => None,
        })
        .unwrap();
    let c = &s.funciones[0].codigo;
    assert!(c.contains(&Instr::Entero { dst: llamada_f, valor: 1 }), "{s}");
    assert!(c.iter().any(|i| matches!(i, Instr::CrearArreglo { dst, .. } if *dst == llamada_f + 1)), "{s}");
    assert!(c.contains(&Instr::Entero { dst: llamada_f + 2, valor: 4 }), "{s}");
}
