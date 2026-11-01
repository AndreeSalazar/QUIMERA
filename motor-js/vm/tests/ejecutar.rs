//! La máquina virtual ejecuta JavaScript de verdad y da los resultados correctos.

use quimera_objetos::{Config, Valor};
use quimera_vm::{Motor, operaciones};

/// Ejecuta `codigo` y devuelve `resultado()` como texto.
fn ejecutar(codigo: &str) -> String {
    let mut m = Motor::default();
    m.ejecutar(codigo).unwrap_or_else(|e| panic!("{}", e.mensaje));
    let v = m.llamar_global("resultado", &[]).unwrap_or_else(|e| panic!("{}", e.mensaje));
    m.texto(v)
}

#[test]
fn aritmetica_y_conversiones() {
    assert_eq!(ejecutar("function resultado() { return 1 + 2 * 3 - 4 / 2 }"), "5");
    assert_eq!(ejecutar("function resultado() { return 2147483647 + 1 }"), "2147483648", "desborda a f64");
    assert_eq!(ejecutar("function resultado() { return 0.1 + 0.2 }"), "0.30000000000000004");
    assert_eq!(ejecutar("function resultado() { return 7 % 3 + (-7 % 3) }"), "0");
    assert_eq!(ejecutar("function resultado() { return 2 ** 10 }"), "1024");
    assert_eq!(ejecutar("function resultado() { return (5 & 3) | (1 << 4) }"), "17");
    assert_eq!(ejecutar("function resultado() { return -1 >>> 28 }"), "15");
    assert_eq!(ejecutar("function resultado() { return 1 / 0 }"), "Infinity");
    assert_eq!(ejecutar("function resultado() { return '5' * '2' }"), "10");
    assert_eq!(ejecutar("function resultado() { return 'a' + 1 + 2 }"), "a12");
    assert_eq!(ejecutar("function resultado() { return 1 + 2 + 'a' }"), "3a");
}

#[test]
fn superinstrucciones_respetan_javascript() {
    // ++ convierte a número; `+ 3` con una cadena concatena; las comparaciones fusionadas con
    // cadenas, NaN y números grandes salen por el camino general.
    let codigo = "function resultado() {
                    let s = '5'; s++; let t = '5'; ++t; let u = 'a'; let z = -0; z++;
                    let r = []; if ('b' > 'a') r.push('cad'); if (NaN < 1) r.push('nan'); else r.push('no-nan');
                    let g = 2147483647; g += 1; let k = 0; for (let i = 0; i < 3.5; i++) k++;
                    return [s, t, u + 3, z, r, g, k]
                  }";
    assert_eq!(ejecutar(codigo), "6,6,a3,1,cad,no-nan,2147483648,4");
}

#[test]
fn comparaciones_y_logica() {
    assert_eq!(
        ejecutar("function resultado() { return [1 == '1', 1 === '1', null == undefined, NaN === NaN, 'b' > 'a'] }"),
        "true,false,true,false,true"
    );
    assert_eq!(ejecutar("function resultado() { return (0 || 'x') + (1 && 'y') + (null ?? 'z') }"), "xyz");
    assert_eq!(
        ejecutar(
            "function resultado() { return typeof 1 + typeof 'a' + typeof {} + typeof resultado + typeof noExiste }"
        ),
        "numberstringobjectfunctionundefined"
    );
}

#[test]
fn cierres_y_entornos() {
    let codigo = "function contador() { let n = 0; return () => ++n }\n\
                  function resultado() { const c = contador(); c(); c(); return c() }";
    assert_eq!(ejecutar(codigo), "3");
    let anidado = "function a() { let x = 1; function b() { let y = 10; return function () { return x + y } } return b() }\n\
                   function resultado() { return a()() }";
    assert_eq!(ejecutar(anidado), "11");
}

#[test]
fn objetos_arreglos_y_prototipos() {
    let codigo = "function resultado() {\n\
                    const o = { a: 1, b: 'dos' }; o.c = 3; o['d'] = 4;\n\
                    const arr = [1, 2]; arr.push(3, 4); arr[10] = 9;\n\
                    const base = { saludo: 'hola' }; const hijo = Object.create(base);\n\
                    return [o.a + o.c + o.d, o.b, arr.length, arr.pop(), arr.length, hijo.saludo, 'x' in o, 'a' in o]\n\
                  }";
    assert_eq!(ejecutar(codigo), "8,dos,11,9,10,hola,false,true");
}

#[test]
fn new_prototype_this_y_call() {
    let codigo = "function Punto(x, y) { this.x = x; this.y = y }\n\
                  Punto.prototype.suma = function () { return this.x + this.y };\n\
                  function resultado() {\n\
                    const p = new Punto(3, 4);\n\
                    const otro = { x: 10, y: 20 };\n\
                    return [p.suma(), p instanceof Punto, Punto.prototype.suma.call(otro)]\n\
                  }";
    assert_eq!(ejecutar(codigo), "7,true,30");
}

#[test]
fn control_de_flujo_y_excepciones() {
    let codigo = "function clasificar(n) { switch (n % 3) { case 0: return 'cero'; case 1: return 'uno'; default: return 'otro' } }\n\
                  function falla() { throw 'mal' }\n\
                  function resultado() {\n\
                    let s = 0; let i = 0;\n\
                    while (true) { i++; if (i > 10) break; if (i % 2) continue; s += i }\n\
                    let capturado = '';\n\
                    try { falla(); capturado = 'no' } catch (e) { capturado = e }\n\
                    try { undefined.x } catch (e) { capturado += '|' + e }\n\
                    return [s, clasificar(4), clasificar(9), capturado]\n\
                  }";
    assert_eq!(ejecutar(codigo), "30,uno,cero,mal|TypeError: no se puede leer la propiedad 'x' de undefined");
}

#[test]
fn las_excepciones_sin_capturar_llegan_como_error() {
    let mut m = Motor::default();
    let e = m.ejecutar("function f() { return noExiste + 1 } f()").unwrap_err();
    assert!(e.mensaje.contains("ReferenceError") && e.mensaje.contains("noExiste"), "{}", e.mensaje);
    let e = m.ejecutar("let x = 1; x()").unwrap_err();
    assert!(e.mensaje.contains("no es una función"), "{}", e.mensaje);
    assert!(m.ejecutar("let = ;").unwrap_err().mensaje.contains("SyntaxError"));
}

#[test]
fn console_log_escribe_la_salida() {
    let mut m = Motor::default();
    m.ejecutar("console.log('hola', 1 + 1, [1, 2], null); print('fin')").unwrap();
    assert_eq!(m.salida(), ["hola 2 1,2 null", "fin"]);
}

#[test]
fn recursion_profunda_sin_agotar_la_pila_de_rust() {
    let codigo = "function f(n) { return n == 0 ? 0 : 1 + f(n - 1) } function resultado() { return f(20000) }";
    assert_eq!(ejecutar(codigo), "20000");
}

#[test]
fn el_gc_mueve_objetos_en_mitad_de_la_ejecucion() {
    // Guardería diminuta: casi cada objeto provoca una recolección mientras el código los usa.
    let mut m = Motor::nuevo(Config { guarderia: 32, objeto_grande: 64, umbral_mayor_minimo: 128 });
    let codigo = "function resultado() {\n\
                    const lista = [];\n\
                    for (let i = 0; i < 2000; i++) { lista.push({ i: i, doble: { v: i * 2 } }) }\n\
                    let s = 0; for (let i = 0; i < lista.length; i++) { s += lista[i].doble.v - lista[i].i }\n\
                    return s\n\
                  }";
    m.ejecutar(codigo).unwrap();
    let v = m.llamar_global("resultado", &[]).unwrap();
    assert_eq!(operaciones::a_numero(&m.mundo, v), 1999.0 * 2000.0 / 2.0);
    let e = m.estadisticas();
    assert!(e.menores > 50 && e.mayores > 0, "{e:?}");
}

/// Los benchmarks de Boa con su resultado de `main()`, calculado a mano.
#[test]
fn los_benchmarks_de_boa_dan_el_resultado_correcto() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend/tests/datos/");
    for (archivo, esperado) in [
        ("basic-call-loop.js", "undefined"),
        ("basic-nested-loop.js", "undefined"),
        ("closures-create.js", "19999900000"),
        ("closures-invoke.js", "50000000"),
        ("properties-access.js", "49650000"),
        ("prototypes-chain.js", "12600000"),
    ] {
        let codigo = std::fs::read_to_string(format!("{dir}{archivo}")).unwrap();
        let mut m = Motor::default();
        m.ejecutar(&codigo).unwrap_or_else(|e| panic!("{archivo}: {}", e.mensaje));
        let v = m.llamar_global("main", &[]).unwrap_or_else(|e| panic!("{archivo}: {}", e.mensaje));
        assert_eq!(m.texto(v), esperado, "{archivo}");
    }
    let _ = Valor::NULO;
}

#[test]
fn biblioteca_minima() {
    let codigo = "function resultado() {\n\
                    const e = new Error('mal'); let atrapado = '';\n\
                    try { throw new Error('dentro') } catch (x) { atrapado = x.message }\n\
                    const d = new Date(); const d2 = new Date();\n\
                    return [(1.23456).toFixed(2), (123456).toPrecision(3), Math.round(2.5), Math.pow(2, 0.5) > 1.414,\n\
                            Math.log(Math.E), Math.max(1, 7, 3), 'hola'.charAt(1), 'hola'.indexOf('la'), 'abc'.charCodeAt(0),\n\
                            'quimera'.slice(-4), [1, 2, 3].join('-'), [5, 6].indexOf(6), e + '', atrapado, d2 - d >= 0]\n\
                  }";
    assert_eq!(ejecutar(codigo), "1.23,1.23e+5,3,true,1,7,o,2,97,mera,1-2-3,1,Error: mal,dentro,true");
}

#[test]
fn richards_de_v8_corre_y_se_comprueba_a_si_mismo() {
    // runRichards() lanza un Error si las cuentas del planificador no son las esperadas.
    let codigo =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend/tests/datos/v8-benches-richards.js"))
            .unwrap();
    let mut m = Motor::default();
    m.ejecutar(&codigo).unwrap_or_else(|e| panic!("{}", e.mensaje));
    for _ in 0..3 {
        m.llamar_global("runRichards", &[]).unwrap_or_else(|e| panic!("{}", e.mensaje));
    }
}

#[test]
fn deltablue_de_v8_corre_y_se_comprueba_a_si_mismo() {
    // deltaBlue() lanza un Error si las restricciones no se resuelven como deben.
    let codigo = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../frontend/tests/datos/v8-benches-deltablue.js"
    ))
    .unwrap();
    let mut m = Motor::default();
    m.ejecutar(&codigo).unwrap_or_else(|e| panic!("{}", e.mensaje));
    for _ in 0..2 {
        m.llamar_global("deltaBlue", &[]).unwrap_or_else(|e| panic!("{}", e.mensaje));
    }
}

#[test]
fn to_primitive_llama_a_value_of_y_to_string() {
    let codigo = "function T(v) { this.v = v }\n\
                  T.prototype.valueOf = function () { return this.v };\n\
                  function N(n) { this.n = n }\n\
                  N.prototype.toString = function () { return 'N' + this.n };\n\
                  function Mal() {}\n\
                  Mal.prototype.valueOf = function () { throw 'valueOf falló' };\n\
                  let llamadas = 0; function Espia() {}\n\
                  Espia.prototype.valueOf = function () { llamadas++; return 1 };\n\
                  function resultado() {\n\
                    const a = new T(10), b = new T(4);\n\
                    let err = ''; try { new Mal() - 1 } catch (e) { err = e }\n\
                    const espia = new Espia(); const nulos = [espia == null, espia != undefined, llamadas];\n\
                    return [a - b, a * 2, Math.log(new T(1)), a > b, -a, 'x' + new N(7), [1, 2] + '', ({}) + '', err, nulos]\n\
                  }";
    assert_eq!(ejecutar(codigo), "6,20,0,true,-10,xN7,1,2,[object Object],valueOf falló,false,true,0");
}
