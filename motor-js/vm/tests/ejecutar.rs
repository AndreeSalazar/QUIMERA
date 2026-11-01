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

#[test]
fn finally_se_ejecuta_siempre() {
    let codigo = "let log = [];\n\
                  function conReturn() { try { return 'r' } finally { log.push('f1') } }\n\
                  function conThrow() { try { throw 'x' } finally { log.push('f2') } }\n\
                  function capturaYFinal() { try { throw 'y' } catch (e) { log.push('c:' + e); return 'cr' } finally { log.push('f3') } }\n\
                  function relanza() { try { try { throw 'z' } catch (e) { throw e + '!' } finally { log.push('f4') } } catch (e) { return e } }\n\
                  function anidados() { try { try { return 1 } finally { log.push('in') } } finally { log.push('out') } }\n\
                  function resultado() {\n\
                    let t = ''; try { conThrow() } catch (e) { t = e }\n\
                    let s = 0; for (let i = 0; i < 5; i++) { try { if (i == 3) break; if (i == 1) continue; s += i } finally { log.push('b' + i) } }\n\
                    let k = 0; for (let i = 0; i < 3; i++) { try { if (i == 0) break } catch (e) {} } try { throw 'tras-break' } catch (e) { k = e }\n\
                    return [conReturn(), t, capturaYFinal(), relanza(), anidados(), s, k, log.join(' ')]\n\
                  }";
    assert_eq!(ejecutar(codigo), "r,x,cr,z!,1,2,tras-break,f2 b0 b1 b2 b3 f1 c:y f3 f4 in out");
}

#[test]
fn for_in_for_of_y_delete() {
    let codigo = "function P() { this.a = 1; this.b = 2 }\n\
                  P.prototype.heredada = 3;\n\
                  function resultado() {\n\
                    const claves = []; for (const k in new P()) claves.push(k);\n\
                    const indices = []; for (let i in ['x', 'y']) indices.push(i);\n\
                    let suma = 0; for (const v of [1, 2, 3]) suma += v;\n\
                    let letras = ''; for (const c of 'hola') letras = c + letras;\n\
                    const o = { a: 1, b: 2, c: 3 }; const borrado = delete o.b; delete o['zz'];\n\
                    const resto = []; for (const k in o) resto.push(k + '=' + o[k]);\n\
                    const arr = [1, 2, 3]; delete arr[1];\n\
                    let hasta = 0; for (const v of [5, 6, 7, 8]) { if (v == 7) break; hasta += v }\n\
                    let err = ''; try { for (const x of 5) {} } catch (e) { err = e }\n\
                    return [claves.join(''), indices.join(''), suma, letras, borrado, 'b' in o, resto.join(';'),\n\
                            arr[1], arr.length, hasta, err]\n\
                  }";
    assert_eq!(ejecutar(codigo), "abheredada,01,6,aloh,true,false,a=1;c=3,,3,11,TypeError: 5 no es iterable");
}

#[test]
fn biblioteca_de_colecciones_y_cadenas() {
    let codigo = "function resultado() {\n\
                    const o = { x: 1, y: 2 }; const a = [3, 1, 2];\n\
                    const dobles = a.map(function (v, i) { return v * 2 + i });\n\
                    const pares = [1, 2, 3, 4, 5, 6].filter(v => v % 2 == 0);\n\
                    let visto = 0; a.forEach(function (v) { visto += v });\n\
                    const suma = a.reduce((s, v) => s + v, 10); const sinInicial = a.reduce((s, v) => s * v);\n\
                    const orden = [10, 9, 1, 100].sort(); const num = [10, 9, 1, 100].sort((p, q) => q - p);\n\
                    const q = [1, 2, 3, 4]; const cola = q.shift(); q.unshift(0, 0.5);\n\
                    const max = Math.max.apply(null, [4, 8, 2]);\n\
                    return [Object.keys(o), o.hasOwnProperty('x'), o.hasOwnProperty('toString'), Array.isArray(a),\n\
                            Array.isArray(o), dobles, pares, visto, suma, sinInicial, orden, num, [1, 2, 3, 4].slice(1, -1),\n\
                            [1].concat([2, 3], 4), [1, 2, 3].reverse(), cola, q, [NaN].includes(NaN), a.some(v => v > 2),\n\
                            a.every(v => v > 2), a.find(v => v < 3), a.findIndex(v => v == 9), max,\n\
                            'a,b,,c'.split(',').length, 'Hola'.toUpperCase(), 'Hola'.toLowerCase(), '  x '.trim() + '|',\n\
                            'quimera'.includes('mer'), 'quimera'.startsWith('qui'), 'quimera'.endsWith('x'),\n\
                            String.fromCharCode(72, 105), String(12) + 1, 'abc'.split('').join('-')].join(' ')\n\
                  }";
    assert_eq!(
        ejecutar(codigo),
        "x,y true false true false 6,3,6 2,4,6 6 16 6 1,10,100,9 100,10,9,1 2,3 1,2,3,4 3,2,1 1 0,0.5,2,3,4 \
         true true false 1 -1 8 4 HOLA hola x| true true false Hi 121 a-b-c"
    );
}

#[test]
fn map_y_sort_sobreviven_al_gc() {
    // Guardería diminuta: cada llamada a la función crea objetos y mueve el arreglo.
    let mut m = Motor::nuevo(Config { guarderia: 32, objeto_grande: 64, umbral_mayor_minimo: 128 });
    let codigo = "function resultado() {\n\
                    const base = []; for (let i = 0; i < 300; i++) base.push({ v: (i * 37) % 300 });\n\
                    const copia = base.map(o => ({ v: o.v, basura: [o.v, { w: 1 }] }));\n\
                    copia.sort((a, b) => { const t = { x: a.v }; return t.x - b.v });\n\
                    let ok = copia.length == 300; for (let i = 0; i < 300; i++) ok = ok && copia[i].v == i;\n\
                    return ok && copia.filter(o => o.basura[1].w == 1).length == 300\n\
                  }";
    m.ejecutar(codigo).unwrap();
    let v = m.llamar_global("resultado", &[]).unwrap_or_else(|e| panic!("{}", e.mensaje));
    assert_eq!(m.texto(v), "true");
    assert!(m.estadisticas().menores > 50);
}

/// El resto de V8 (Octane): cada uno se comprueba a sí mismo y lanza un Error si el resultado no es el esperado.
#[test]
fn los_demas_benchmarks_de_v8_corren_y_se_comprueban() {
    for (archivo, llamadas) in [
        ("v8-benches-crypto.js", &["encrypt", "decrypt"][..]),
        ("v8-benches-raytrace.js", &["renderScene"]),
        ("v8-benches-earley-boyer.js", &["BgL_earleyzd2benchmarkzd2", "BgL_nboyerzd2benchmarkzd2"]),
        ("v8-benches-navier-stokes.js", &["setupNavierStokes", "runNavierStokes", "tearDownNavierStokes"]),
        ("v8-benches-splay.js", &["SplaySetup", "SplayRun", "SplayTearDown"]),
        ("v8-benches-regexp.js", &["RegExpSetup"]),
    ] {
        let ruta = format!("{}/../frontend/tests/datos/{archivo}", env!("CARGO_MANIFEST_DIR"));
        let codigo = std::fs::read_to_string(ruta).unwrap();
        let mut m = Motor::default();
        m.ejecutar(&codigo).unwrap_or_else(|e| panic!("{archivo}: {}", e.mensaje));
        for f in llamadas {
            m.llamar_global(f, &[]).unwrap_or_else(|e| panic!("{archivo} {f}: {}", e.mensaje));
        }
    }
}

#[test]
fn arguments_parse_int_y_var_global() {
    let codigo = "var sinValor; var conValor = 1;\n\
                  function suma() { let s = 0; for (let i = 0; i < arguments.length; i++) s += arguments[i]; return s }\n\
                  function capturado() { return () => arguments[0] }\n\
                  function envolver() { return function () { return suma.apply(this, arguments) } }\n\
                  function resultado() {\n\
                    return [typeof sinValor, sinValor == null, conValor, suma(1, 2, 3), capturado('x')(), envolver()(4, 5),\n\
                            parseInt('  42px'), parseInt('-0x1f'), parseInt('z', 36), parseInt('101', 2), isNaN(parseInt('a')),\n\
                            parseFloat('3.5e2abc'), parseFloat('.5'), (255).toString(16), (-10).toString(2), Number('7') + 1,\n\
                            isFinite(1 / 0)]\n\
                  }";
    assert_eq!(ejecutar(codigo), "undefined,true,1,6,x,9,42,-31,35,5,true,350,0.5,ff,-1010,8,false");
}

#[test]
fn expresiones_regulares() {
    let codigo = r#"function resultado() {
        const m = /(\d+)-(\d+)?/.exec('ab 12- cd');
        const g = /o/g; const posiciones = []; let r; while ((r = g.exec('foo boo')) != null) posiciones.push(r.index + ':' + g.lastIndex);
        const fecha = '2026-10-31'.replace(/(\d+)-(\d+)-(\d+)/, '$3/$2/$1 [$&] $$ $9');
        const mayus = 'uno dos'.replace(/\b\w/g, function (c, i) { return c.toUpperCase() + i });
        const re = new RegExp('a+', 'gi');
        return [m[0], m[1], m[2], m.index, m.input, m.length, posiciones.join(' '), fecha, mayus,
                'a1b22c333'.split(/\d+/).join('|'), 'a,b;c'.split(/([,;])/).join('|'), 'abc'.split(/(?:)/).join('|'),
                ''.split(/x/).length, 'aAbAA'.match(re).join('|'), 'xyz'.match(/q/), 'xaay'.match(/a+/)[0],
                'hola mundo'.search(/m/), /^\s*$/.test('   '), /^\s*$/.test(' x '), 'a.b.c'.replace('.', '-'),
                'ñandú-ñ'.replace(/ñ/g, 'n'), 'ññx'.search(/x/), String(/a\/b/g), re.lastIndex,
                '  recorta  '.replace(/^\s*|\s*$/g, '') + '|']
      }"#;
    assert_eq!(
        ejecutar(codigo),
        "12-,12,,3,ab 12- cd,3,1:2 2:3 5:6 6:7,31/10/2026 [2026-10-31] $ $9,U0no D4os,a|b|c|,a|,|b|;|c,a|b|c,1,\
         aA|AA,,aa,5,true,false,a-b.c,nandú-n,2,/a\\/b/g,0,recorta|"
    );
}

#[test]
fn regex_mal_escrita_es_syntax_error() {
    let mut m = Motor::default();
    assert!(m.ejecutar("let r = /(/;").unwrap_err().mensaje.contains("SyntaxError"));
    let e = m.ejecutar("new RegExp('[a')").unwrap_err();
    assert!(e.mensaje.contains("SyntaxError"), "{}", e.mensaje);
}

#[test]
fn las_cadenas_que_nadie_usa_se_liberan() {
    // Un millón de textos distintos, de los que solo se guardan unos pocos: la tabla de cadenas
    // no debe crecer con todos. Las claves escritas (o[k] = ...) sí se quedan, y siguen valiendo.
    let mut m = Motor::default();
    let codigo = "function resultado() {\n\
                    const guardadas = []; const o = {};\n\
                    for (let i = 0; i < 1000000; i++) {\n\
                      const s = 'texto-' + i;\n\
                      if (i % 100000 == 0) { guardadas.push(s); o['k' + i] = s.length }\n\
                    }\n\
                    let claves = 0; for (const k in o) claves += o[k];\n\
                    return guardadas.join(',') + '|' + claves + '|' + o.k500000 + '|' + o['k' + 900000]\n\
                  }";
    m.ejecutar(codigo).unwrap();
    let v = m.llamar_global("resultado", &[]).unwrap_or_else(|e| panic!("{}", e.mensaje));
    let esperado = (0..10).map(|i| format!("texto-{}", i * 100000)).collect::<Vec<_>>().join(",");
    assert_eq!(m.texto(v), format!("{esperado}|115|12|12"));
    let (vivas, liberadas) = (m.mundo.cadenas.total(), m.mundo.cadenas.liberadas());
    assert!(liberadas > 900_000, "liberadas: {liberadas}");
    assert!(vivas < 200_000, "vivas: {vivas}");
}
