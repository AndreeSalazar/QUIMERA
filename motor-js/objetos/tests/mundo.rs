//! Objetos, arreglos, prototipos y entornos, también cuando el GC mueve cosas por medio.

use quimera_objetos::{Clase, Config, Mundo, Valor};

/// Un mundo con la guardería diminuta: casi cada asignación provoca una recolección.
fn apretado() -> Mundo {
    Mundo::nuevo(Config { guarderia: 16, objeto_grande: 64, umbral_mayor_minimo: 64 })
}

#[test]
fn propiedades_que_crecen_y_sobreviven() {
    let mut w = apretado();
    let o = w.crear_objeto(Valor::NULO);
    let r = w.m.raiz(o);
    let claves: Vec<u32> = (0..40).map(|i| w.cadenas.internar(&format!("p{i}"))).collect();
    for (i, &k) in claves.iter().enumerate() {
        let hijo = w.crear_objeto(Valor::NULO); // basura y movimientos por medio
        let _ = hijo;
        let o = w.m.valor(&r);
        assert!(w.escribir(o, k, Valor::entero(i as i32)));
    }
    w.m.mayor();
    let o = w.m.valor(&r);
    for (i, &k) in claves.iter().enumerate() {
        assert_eq!(w.leer(o, k), Some(Valor::entero(i as i32)), "p{i}");
    }
    assert!(w.m.estadisticas().menores > 5, "{:?}", w.m.estadisticas());
}

#[test]
fn la_misma_construccion_comparte_forma() {
    let mut w = Mundo::default();
    let (x, y) = (w.cadenas.internar("x"), w.cadenas.internar("y"));
    let a = w.crear_objeto(Valor::NULO);
    w.escribir(a, x, Valor::entero(1));
    w.escribir(a, y, Valor::entero(2));
    let ra = w.m.raiz(a);
    let b = w.crear_objeto(Valor::NULO);
    w.escribir(b, x, Valor::entero(3));
    w.escribir(b, y, Valor::entero(4));
    let a = w.m.valor(&ra);
    assert_eq!(w.forma(a), w.forma(b));
    assert_eq!(w.claves(b), [x, y]);
}

#[test]
fn la_cadena_de_prototipos() {
    let mut w = Mundo::default();
    let k = w.cadenas.internar("base");
    let raiz = w.crear_objeto(Valor::NULO);
    w.escribir(raiz, k, Valor::entero(42));
    let mut actual = raiz;
    let marca = w.m.marca_raices();
    for _ in 0..10 {
        w.m.raiz(actual);
        actual = w.crear_objeto(actual);
    }
    assert_eq!(w.leer(actual, k), Some(Valor::entero(42)), "diez saltos");
    assert_eq!(w.leer_propia(actual, k), None);
    let nada = w.cadenas.internar("nada");
    assert_eq!(w.leer(actual, nada), None);
    w.m.soltar_raices(marca);
}

#[test]
fn arreglos_densos_push_pop_y_length() {
    let mut w = apretado();
    let a = w.crear_arreglo(&[Valor::entero(1), Valor::entero(2)], Valor::NULO);
    let r = w.m.raiz(a);
    for i in 3..=500 {
        let a = w.m.valor(&r);
        assert_eq!(w.empujar(a, Valor::entero(i)), Some(i as u32));
    }
    let a = w.m.valor(&r);
    assert_eq!(w.clase(a), Some(Clase::Arreglo));
    assert_eq!(w.longitud(a), Some(500));
    assert_eq!(w.leer_elemento(a, 499), Some(Valor::entero(500)));
    assert_eq!(w.leer_elemento(a, 9999), Some(Valor::INDEFINIDO));
    assert_eq!(w.sacar(a), Some(Valor::entero(500)));
    let length = w.clave_length;
    assert_eq!(w.leer(a, length), Some(Valor::numero(499.0)));
    assert!(w.escribir(a, length, Valor::entero(10)));
    assert_eq!(w.longitud(a), Some(10));
    assert!(!w.escribir_elemento(a, 100_000_000, Valor::NULO), "disperso: todavía no");
}

#[test]
fn entornos_encadenados() {
    let mut w = apretado();
    let fuera = w.crear_entorno(Valor::NULO, 2);
    w.escribir_entorno(fuera, 0, 1, Valor::entero(7));
    let r = w.m.raiz(fuera);
    let fuera = w.m.valor(&r);
    let dentro = w.crear_entorno(fuera, 30); // más de 14 huecos: el camino largo
    w.escribir_entorno(dentro, 0, 29, Valor::entero(8));
    assert_eq!(w.leer_entorno(dentro, 1, 1), Some(Valor::entero(7)));
    assert_eq!(w.leer_entorno(dentro, 0, 29), Some(Valor::entero(8)));
    assert_eq!(w.leer_entorno(dentro, 5, 0), None, "no hay tantos niveles");
}

#[test]
fn funciones_y_nativas() {
    let mut w = Mundo::default();
    let env = w.crear_entorno(Valor::NULO, 0);
    let f = w.crear_funcion(3, env, Valor::NULO, Valor::NULO);
    assert_eq!(w.funcion(f).map(|(i, _, _)| i), Some(3));
    let n = w.crear_nativa(9, Valor::NULO);
    assert_eq!((w.nativa(n), w.funcion(n)), (Some(9), None));
    assert!(w.es_objeto(f) && w.es_objeto(n) && !w.es_objeto(env));
}
