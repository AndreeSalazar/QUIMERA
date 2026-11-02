//! JavaScript moderno: JSON, Map/Set, Promise y microtareas, y lo nuevo de la biblioteca.
//! Los resultados esperados son los de Chromium con el mismo código.

use quimera_objetos::Config;
use quimera_vm::Motor;

/// Ejecuta `codigo`, luego las microtareas, y devuelve lo escrito con `console.log`.
fn salida_con(mut m: Motor, codigo: &str) -> Vec<String> {
    m.ejecutar(codigo).unwrap_or_else(|e| panic!("{}", e.mensaje));
    let errores = m.correr_microtareas();
    assert!(errores.is_empty(), "{errores:?}");
    m.salida().to_vec()
}

fn salida(codigo: &str) -> Vec<String> {
    salida_con(Motor::default(), codigo)
}

#[test]
fn json() {
    let r = salida(
        r#"var o = JSON.parse(' {"a": [1, 2.5, -3e2, true, null], "b": {"c": "x\\"y\\u00f1\\ud83d\\ude00"}, "": 0} ');
           console.log(o.a.length, o.a[1], o.a[2], o.a[3], o.a[4], o.b.c, o[""]);
           console.log(JSON.stringify(o));
           console.log(JSON.stringify({x: 1, f: function () {}, u: undefined, n: NaN, a: [undefined, function () {}], s: "a\nb"}));
           console.log(JSON.stringify([1, {a: 2}], null, 2));
           console.log(JSON.stringify({d: {toJSON: function (k) { return 'clave ' + k; }}}), JSON.stringify('x'), JSON.stringify(undefined));
           console.log(JSON.stringify({a: 1, b: 2, c: 3}, ['a', 'c']));
           var errores = 0;
           ['{', '[1,]', '01', '"\x01"', 'tru', '{"a" 1}', '1 2'].forEach(function (t) { try { JSON.parse(t); } catch (e) { errores++; } });
           var ciclo = {}; ciclo.yo = ciclo;
           try { JSON.stringify(ciclo); } catch (e) { errores++; }
           console.log(errores);"#,
    );
    assert_eq!(
        r,
        [
            "5 2.5 -300 true null x\"yñ😀 0",
            r#"{"a":[1,2.5,-300,true,null],"b":{"c":"x\"yñ😀"},"":0}"#,
            r#"{"x":1,"n":null,"a":[null,null],"s":"a\nb"}"#,
            "[\n  1,\n  {\n    \"a\": 2\n  }\n]",
            r#"{"d":"clave d"} "x" undefined"#,
            r#"{"a":1,"c":3}"#,
            "8",
        ]
    );
}

#[test]
fn json_con_el_gc_moviendo_objetos() {
    let m = Motor::nuevo(Config { guarderia: 64, objeto_grande: 128, umbral_mayor_minimo: 256 });
    let r = salida_con(
        m,
        "var partes = [];
         for (var i = 0; i < 300; i++) partes.push('{\"i\":' + i + ',\"l\":[' + i + ',{\"d\":\"' + i + '\"}]}');
         var a = JSON.parse('[' + partes.join(',') + ']');
         var s = 0;
         for (var i = 0; i < a.length; i++) s += a[i].i + a[i].l[0] + +a[i].l[1].d;
         console.log(a.length, s, JSON.stringify(a).length === ('[' + partes.join(',') + ']').length);",
    );
    assert_eq!(r, ["300 134550 true"]);
}

#[test]
fn map_y_set() {
    let r = salida(
        "var m = new Map([[1, 'uno'], ['1', 'texto']]);
         var k = {}; m.set(k, 'objeto').set(NaN, 'nan').set(-0, 'cero');
         console.log(m.size, m.get(1), m.get('1'), m.get(k), m.get({}), m.get(NaN), m.get(0), m.has(2));
         m.delete('1');
         var claves = []; m.forEach(function (v, c) { claves.push(typeof c); });
         console.log(claves.join(), m.size);
         var pares = []; for (var p of m) pares.push(p[1]);
         console.log(pares.join(), Array.from(m.keys()).length, Object.keys(m).join());
         var s = new Set([3, 1, 3, 2, 1]);
         s.add(4); s.delete(1);
         console.log(s.size, Array.from(s).join(), s.has(3), s.has(1));
         var en = []; for (var k2 in s) en.push(k2);
         console.log(en.join(), JSON.stringify(s));
         var w = new WeakMap(); w.set(k, 5); console.log(w.get(k), w.has({}));",
    );
    assert_eq!(
        r,
        [
            "5 uno texto objeto undefined nan cero false",
            "number,object,number,number 4",
            "uno,objeto,nan,cero 4 ",
            "3 3,2,4 true false",
            " {}",
            "5 false",
        ]
    );
}

#[test]
fn promesas_y_microtareas() {
    let r = salida(
        "console.log('inicio');
         Promise.resolve(1).then(function (v) { console.log('then', v); return v + 1; })
           .then(function (v) { console.log('then', v); throw new TypeError('mal'); })
           .catch(function (e) { console.log('catch', e.name, e.message, e instanceof Error); })
           .finally(function () { console.log('finally'); });
         queueMicrotask(function () { console.log('microtarea'); });
         new Promise(function (res) { res(Promise.resolve('anidada')); }).then(function (v) { console.log(v); });
         Promise.all([1, Promise.resolve(2), new Promise(function (r) { r(3); })]).then(function (v) { console.log('all', v.join()); });
         Promise.allSettled([Promise.reject('no'), 1]).then(function (v) { console.log(v[0].status, v[1].value); });
         Promise.race([new Promise(function () {}), Promise.resolve('race')]).then(console.log);
         console.log('fin');",
    );
    assert_eq!(
        r,
        [
            "inicio",
            "fin",
            "then 1",
            "microtarea",
            "then 2",
            "all 1,2,3",
            "rejected 1",
            "race",
            "catch TypeError mal true",
            "anidada",
            "finally",
        ]
    );
}

#[test]
fn biblioteca_nueva() {
    let r = salida(
        "var o = Object.assign({a: 1}, {b: 2}, null, {a: 3});
         console.log(JSON.stringify(o), JSON.stringify(Object.entries(o)), Object.values(o).join());
         var a = [1, 2, 3, 4, 5];
         console.log(a.splice(1, 2, 'x', 'y', 'z').join(), a.join(), [1, [2, [3, [4]]]].flat(2).join('|'));
         console.log('5'.padStart(3, '0'), 'ab'.repeat(3), 'a-b-c'.replaceAll('-', '+'), [1, 2, 3].at(-1));
         console.log(Number.isInteger(5), Number.isInteger(5.5), Math.sign(-3), Math.trunc(-4.7), Object.is(NaN, NaN));
         var f = function (x, y) { return this.n + x + y; }.bind({n: 10}, 1);
         console.log(f(2), Array.from('hola').join('.'), Array.from({length: 2, 0: 'a', 1: 'b'}).join());
         var visto = []; for (var k in [7, 8]) visto.push(k);
         console.log(visto.join(), new RangeError('r') instanceof Error, String(new TypeError('t')));",
    );
    assert_eq!(
        r,
        [
            r#"{"a":3,"b":2} [["a",3],["b",2]] 3,2"#,
            "2,3 1,x,y,z,4,5 1|2|3|4",
            "005 ababab a+b+c 3",
            "true false -1 -4 true",
            "13 h.o.l.a a,b",
            "0,1 true TypeError: t",
        ]
    );
}

/// Plantillas, spread, rest, desestructuración, `?.`, `??` y clases (`tests/sintaxis.js`),
/// con la salida de Chromium.
#[test]
fn sintaxis_moderna() {
    let r = salida(include_str!("sintaxis.js"));
    assert_eq!(
        r,
        [
            "plantilla 3 + 6 = 9 sin nada anidada dentro 3 11",
            "a|b|c:1,2",
            "6 9 0,1,2,a,b,5 10",
            "1:2-3 1: 3",
            "1 7 99 defecto {\"q\":1,\"r\":2}",
            "a c def",
            "ana3089 luis123",
            "2 1 k d",
            "p1,q2,x,y",
            "20 undefined undefined 10 undefined reserva 0",
            "{\"b\":\"pisado\",\"extra\":1} 1",
            "rex hace ruido (guau) 4 hueso perro de animal 1 true true",
            "nombre,patas function 5",
            "MiError fallo true true",
            "2 0",
        ]
    );
}

/// `get`/`set` en objetos y clases, `defineProperty` y `getOwnPropertyDescriptor`
/// (`tests/accesores.js`), con la salida de Chromium.
#[test]
fn accesores() {
    let r = salida(include_str!("accesores.js"));
    assert_eq!(
        r,
        [
            "40 4 fijo 5 din true true",
            r#"_x,x,soloLectura,calculada,dinamico {"_x":4,"x":40,"soloLectura":"fijo","calculada":5,"dinamico":"din"}"#,
            "100 212 C  true false",
            "283 50",
            "definido function undefined 1",
            "datos true",
            "5 6",
            "70 true 40",
        ]
    );
}

/// `let`/`const`/`catch` de bloque que tapan a una variable de fuera (`tests/bloques.js`).
#[test]
fn ambitos_de_bloque() {
    let r = salida(include_str!("bloques.js"));
    assert_eq!(r, [r#"param fuera fuera fuera fuera 1,2 fuera {"a":1,"extra":"b"}"#]);
}

/// `Symbol`, `Symbol.for` y el protocolo de iteración con `Symbol.iterator`
/// (`tests/simbolos.js`), con la salida de Chromium.
#[test]
fn simbolos() {
    let r = salida(include_str!("simbolos.js"));
    assert_eq!(
        r,
        [
            r#"symbol false clave Symbol(clave) oculto undefined true a {"a":1}"#,
            "true app undefined symbol",
            "0,1,2 0,1 4 4",
            "10 20 true function function",
            "x y",
            "new Symbol falla",
        ]
    );
}

/// Generadores: `yield`, `next(x)`, `throw`, `return`, `yield*`, métodos `*m()` y
/// `Symbol.iterator` (`tests/generadores.js`), con la salida de Chromium.
#[test]
fn generadores() {
    let r = salida(include_str!("generadores.js"));
    assert_eq!(
        r,
        [
            "recibido hola",
            r#"{"value":0,"done":false} {"value":1,"done":false} {"value":2,"done":false} {"value":"fin","done":true} {"done":true}"#,
            "0,1,2,3 0,1",
            "of 0",
            "of 1",
            "capturado fallo",
            r#"1 tras catch {"done":true}"#,
            r#"{"value":"corto","done":true} {"done":true}"#,
            "0,a,b,r,7,8",
            "2,4 x 1,2,3,4",
            "13,2 function true",
            "0,1,2",
            "no es constructor",
        ]
    );
}

/// `async`/`await` en funciones, flechas, métodos y clases, con errores capturados y el orden
/// de las microtareas (`tests/async.js`), con la salida de Chromium.
#[test]
fn async_await() {
    let r = salida(include_str!("async.js"));
    assert_eq!(
        r,
        [
            "inicio",
            "fin true function",
            "simple 2",
            "metodo",
            "falla TypeError mal",
            "flecha 10",
            "capturado con await mal",
            "suma 3",
            "servicio 123",
            "todos 2,4,3",
            "bucle a",
            "bucle b",
            "listo",
        ]
    );
}
