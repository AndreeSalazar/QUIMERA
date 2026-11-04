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

/// Generadores asíncronos, `for await` y las banderas de las expresiones regulares
/// (`tests/async_gen.js`), con la salida de Chromium.
#[test]
fn generadores_asincronos() {
    let r = salida(include_str!("async_gen.js"));
    assert_eq!(
        r,
        [
            "gi a true true true (?:)",
            "true",
            "for await 0,10,20",
            r#"{"value":0,"done":false} {"value":10,"done":false} {"value":"fin","done":true}"#,
            "valor 1",
            "error roto",
            "sobre promesas a,b,c",
            "propio",
            "function function",
        ]
    );
}

/// `Date` con partes, texto ISO y métodos; `Object.prototype.toString`; más métodos de cadenas
/// y arreglos; URI y base64 (`tests/biblioteca2.js`), con la salida de Chromium.
#[test]
fn biblioteca_segunda_tanda() {
    let r = salida(include_str!("biblioteca2.js"));
    assert_eq!(
        r,
        [
            "2024-02-29T13:05:09.007Z 2024 1 29 4 13 1709211909007",
            "1583058030000 2020-03-01T08:20:30.500Z 946598400000",
            r#"{"cuando":"1970-01-01T00:00:00.000Z"} Thu, 01 Jan 1970 00:00:00 GMT true"#,
            "2023-03-03T00:00:00.000Z true number",
            "[object Array] [object Object] [object Null] [object Undefined] [object Number] [object String] [object Function] [object RegExp] [object Map] [object Error] [object Date] [object Promise]",
            "1,2,3 [object Object] 4 b ab1 128512 true",
            "6 1,2,3 9,2 false true",
            "0 a%20b%26%C3%B1 ñ aG9sYQ== hola",
        ]
    );
}

/// `this` de una llamada sin objeto: el global fuera del modo estricto, `undefined` con
/// `"use strict"` y en las clases (que siempre son estrictas), y el de fuera en las flechas.
#[test]
fn this_y_modo_estricto() {
    let r = salida(
        "function a() { return this === globalThis; }
         function b() { 'use strict'; return this; }
         function c() { 'use strict'; return (function () { return this; })(); }
         class C { m() { return this; } }
         var m = new C().m;
         console.log(a(), b(), c(), m(), (function () { return typeof this; })(), (() => this === globalThis)());",
    );
    assert_eq!(r, ["true undefined undefined undefined object true"]);
}

/// Etiquetas: `break`/`continue` a un bucle de fuera, y `break` de un bloque o `switch`.
#[test]
fn etiquetas() {
    let r = salida(
        "var r = [];
         fuera: for (var i = 0; i < 3; i++) {
           for (var j = 0; j < 3; j++) {
             if (j === 1) continue fuera;
             if (i === 2) break fuera;
             r.push(i + '' + j);
           }
         }
         bloque: { r.push('a'); if (true) break bloque; r.push('nunca'); }
         sw: switch (1) { case 1: for (;;) { break sw; } }
         d: do { r.push('do'); while (true) { break d; } } while (false);
         r.push('fin');
         console.log(r.join());",
    );
    assert_eq!(r, ["00,10,a,do,fin"]);
}

/// `super(...)` sustituye el `this`: subclases de `Array` y `Error` de verdad, y `return;` en un
/// constructor derivado (valores de Chromium).
#[test]
fn super_sustituye_this() {
    let r = salida(
        "class Pila extends Array { tope() { return this[this.length - 1]; } }
         var p = new Pila(); p.push(1, 2);
         class E2 extends Error { constructor(m) { super(m); this.code = 7; } }
         var e = new E2('x');
         class A { constructor() { this.a = 1; } }
         class B extends A { constructor() { super(); if (this.a) return; this.b = 2; } }
         class C extends A { constructor() { super(); const f = () => this.a + 1; this.c = f(); } }
         console.log(Array.isArray(p), p.length, p.tope(), p instanceof Pila, p instanceof Array, e instanceof E2, e instanceof Error,
                     e.message, e.code, Object.prototype.toString.call(e), JSON.stringify(new B()), JSON.stringify(new C()));",
    );
    assert_eq!(r, [r#"true 2 2 true true true true x 7 [object Error] {"a":1} {"a":1,"c":2}"#]);
}

/// `...rest` en asignaciones con patrón y escapes `\u` en los nombres (valores de Chromium).
#[test]
fn resto_en_asignaciones_y_escapes() {
    let r = salida(
        r"var t, i, a, b;
          ({ref: i, ...t} = {ref: 1, x: 2, y: 3});
          [a, ...b] = [7, 8, 9];
          var caf\u{e9} = 'café';
          console.log(i, JSON.stringify(t), a, b.join(), café, typeof 1);",
    );
    assert_eq!(r, [r#"1 {"x":2,"y":3} 7 8,9 café number"#]);
}

/// Tras el `)` de un `if`/`while`/`for`, un `/` empieza una regex; tras otro `)`, divide.
#[test]
fn regex_tras_cabecera_de_control() {
    let r = salida(
        r"var o = 65, r = [], a = 8, b = 2, g = 2;
          if (o < 128) /[A-Z]/.test(String.fromCharCode(o)) && r.push('regex');
          for (var i = 0; i < 1; i++) /x/g.test('x') && r.push('for');
          r.push((a) / b / g, (a + 0) / 2);
          console.log(r.join());",
    );
    assert_eq!(r, ["regex,for,2,4"]);
}

/// `Object.create(arreglo)` hereda sus elementos y su `length` (como usa selector-observer).
#[test]
fn objeto_sobre_arreglo_hereda_elementos() {
    let r = salida(
        r"var a = []; a.push({n: 1}, {n: 2}); a.default = {n: 9};
          var o = Object.create(a);
          var s = o.slice(0);
          console.log(o.length, s.length, s.concat(o.default).map(function (x) { return x && x.n; }).join(), o[1].n,
            a.isPrototypeOf(o), Array.prototype.isPrototypeOf(o), o.isPrototypeOf(a), Object.keys(o).length);",
    );
    assert_eq!(r, ["2 2 1,2,9 2 true true false 0"]);
}

/// Los errores de la máquina son objetos de su clase, con `message` y `stack`.
#[test]
fn errores_como_objetos_con_traza() {
    let r = salida(
        r"function leer(o) { return o.x; }
          function paso(o) { return leer(o); }
          try { paso(undefined); } catch (e) {
            console.log(e instanceof TypeError, e instanceof Error, e.name, e.message);
            console.log(e.stack.split('\n').slice(0, 3).join('|'));
          }
          try { null(); } catch (e) { console.log(String(e).slice(0, 10), e.constructor === TypeError); }
          try { noExiste; } catch (e) { console.log(e.name); }
          var f = new Error('hola');
          console.log(f.message, f.stack.split('\n')[0], 'stack' in new RangeError('x'));",
    );
    assert_eq!(
        r,
        [
            "true true TypeError no se puede leer la propiedad 'x' de undefined",
            "TypeError: no se puede leer la propiedad 'x' de undefined|    at leer (<anónimo>:1:1)|    at paso (<anónimo>:2:11)",
            "TypeError: true",
            "ReferenceError",
            "hola Error: hola true",
        ]
    );
}

/// Un `let` de un bucle que se llama como una variable de una función de fuera no la tapa antes
/// del bucle (código minificado de GitHub).
#[test]
fn let_del_for_no_tapa_a_la_de_fuera() {
    let r = salida(
        r"var r = {a: 1, b: 2};
          function f() { var t = Object.keys(r); for (let n = t.length, r = 0; r < n; r++) t[r] += r; return t.join(); }
          function g() { var s = r.a; for (const r of [5]) s += r; return s; }
          console.log(f(), g(), r.a);",
    );
    assert_eq!(r, ["a0,b1 6 1"]);
}

/// `ArrayBuffer`, arreglos tipados, `DataView` y `TextEncoder`/`TextDecoder` (valores de Chromium).
#[test]
fn datos_binarios() {
    let r = salida(
        r"var b = new ArrayBuffer(8), v = new DataView(b), u = new Uint8Array(b);
          v.setUint16(0, 0x1234); v.setInt32(4, -2, true);
          console.log(u.join(), v.getUint16(0, true), v.getInt32(4, true), b.byteLength, u.length);
          v.setFloat64(0, Math.PI); console.log(v.getFloat64(0), u[0], u[7]);
          v.setFloat32(0, 1.1, true); console.log(v.getFloat32(0, true), Math.fround(1.1), Math.fround(5.5));
          var i16 = new Int16Array([70000, -1, 3.7]); console.log(i16.join(), i16.byteLength, Int16Array.BYTES_PER_ELEMENT);
          var c = new Uint8ClampedArray([300, -5, 1.5]); console.log(c.join());
          console.log(Array.isArray(u), Array.isArray([]), ArrayBuffer.isView(u), u instanceof Uint8Array, Object.prototype.toString.call(i16));
          var e = new TextEncoder().encode('añ€😀'); console.log(e.length, e.join(), new TextDecoder().decode(e));
          console.log(new TextDecoder().decode(new Uint8Array([104, 0xff, 105])), u.subarray(1, 3) instanceof Uint8Array, new Uint8Array(3).fill(258).join());",
    );
    assert_eq!(
        r,
        [
            "18,52,0,0,254,255,255,255 13330 -2 8 8",
            "3.141592653589793 64 24",
            "1.100000023841858 1.100000023841858 5.5",
            "4464,-1,3 6 2",
            "255,0,2",
            "false true true true [object Int16Array]",
            "10 97,195,177,226,130,172,240,159,152,128 añ€😀",
            "h\u{fffd}i true 2,2,2",
        ]
    );
}

/// `enumerable` de `Object.defineProperty` (por defecto `false` al crear), getters de objetos
/// literales (enumerables) y de clases (no). Valores de Chromium.
#[test]
fn propiedades_no_enumerables() {
    let r = salida(
        r#"var o = {a: 1};
          Object.defineProperty(o, 'b', {value: 2, configurable: true});
          Object.defineProperty(o, 'c', {value: 3, enumerable: true});
          Object.defineProperty(o, 'd', {get: function () { return 4; }});
          var k = []; for (var x in o) k.push(x);
          console.log(Object.keys(o).join(), k.join(), JSON.stringify(o), o.b, o.propertyIsEnumerable('b'), o.propertyIsEnumerable('a'));
          Object.defineProperty(o, 'a', {value: 9});
          Object.defineProperty(o, 'b', {enumerable: true});
          console.log(Object.keys(o).join(), Object.getOwnPropertyDescriptor(o, 'd').enumerable, Object.getOwnPropertyDescriptor(o, 'a').enumerable);
          var l = {get x() { return 1; }, y: 2};
          class C { get z() { return 3; } }
          var h = Object.create({p: 1}); Object.defineProperty(h, 'p', {value: 2}); var hk = []; for (var q in h) hk.push(q);
          console.log(Object.keys(l).join(), Object.keys(C.prototype).length, Object.assign({}, o).b, hk.length);"#,
    );
    assert_eq!(r, [r#"a,c a,c {"a":1,"c":3} 2 false true"#, "a,b,c false true", "x,y 0 2 0",]);
}

/// `Proxy` con trampas `get`, `set`, `has`, `deleteProperty` y `ownKeys`, y `Reflect`.
/// Valores de Chromium.
#[test]
fn proxy_y_reflect() {
    let r = salida(
        r#"var vacio = new Proxy({}, {get: () => ''});
          console.log(JSON.stringify(vacio.loquesea), typeof vacio, vacio[5] === '');
          var log = [], obj = {a: 1};
          var p = new Proxy(obj, {
            get(t, k, r) { log.push('get ' + String(k)); return k in t ? Reflect.get(t, k) : 'falta'; },
            set(t, k, v) { log.push('set ' + k); t[k] = v * 2; return true; },
            has(t, k) { return k.charAt(0) === 'a'; },
            deleteProperty(t, k) { log.push('delete ' + k); delete t[k]; return true; }
          });
          p.b = 5;
          console.log(p.a, p.b, p.c, obj.b, 'abc' in p, 'b' in p, delete p.a, obj.a);
          console.log(log.join('|'));
          var q = new Proxy({x: 1, y: 2}, {ownKeys: () => ['y']});
          var s = new Proxy([1, 2], {});
          console.log(Object.keys(q).join(), Object.keys(new Proxy({m: 1, n: 2}, {})).join(), s.length, s[1], s instanceof Array);
          console.log(Reflect.has({z: 1}, 'z'), Reflect.ownKeys({k: 1}).join(), Reflect.apply(Math.max, null, [1, 3]));"#,
    );
    assert_eq!(
        r,
        [
            "\"\" object true",
            "1 10 falta 10 true false true undefined",
            "set b|get a|get b|get c|delete a",
            "y m,n 2 2 true",
            "true k 3",
        ]
    );
}

/// `new String(x)` y `new Number(x)`: objetos con su valor dentro (Sizzle de jQuery 1.8).
/// Valores de Chromium.
#[test]
fn envoltorios_de_primitivos() {
    let r = salida(
        r#"var n = new String('ab>c'); n.type = 'TAG';
          console.log(typeof n, n.length, n + '!', n.slice(1, 3), n == 'ab>c', n === 'ab>c', n.type, n.toUpperCase(), String(n), JSON.stringify(Object.keys(n)));
          var x = new Number(5);
          console.log(typeof x, x + 1, x * 2, x.toFixed(1), x == 5, Number(x));"#,
    );
    assert_eq!(r, [r#"object 4 ab>c! b> true false TAG AB>C ab>c ["0","1","2","3","type"]"#, "object 6 10 5.0 true 5"]);
}

/// Escribir `length` en un arreglo lo recorta aunque la caché de esa línea haya aprendido
/// «añadir `length`» con un objeto normal (colgaba jQuery 1.8 en Steam). Y `eval` indirecto.
#[test]
fn length_de_arreglos_y_eval() {
    let r = salida(
        r#"function f(o, n) { o.length = n; }
          var o = {}; f(o, 1);
          var p = {length: 5}; f(p, 2);
          var a = [1, 2, 3]; f(a, 1);
          var b = [1, 2, 3]; b.x = 1; f(b, 2);
          var c = [1, 2, 3, 4]; c.length = 2;
          function g(o) { return o.length; }
          var lecturas = [g({}), g({}), g([1, 2]), g([1, 2, 3]), g({length: 7}), g([])];
          console.log(o.length, p.length, a.length, a.join(), b.length, b.join(), c.join(), lecturas.join());
          var global = this; global.eval.call(global, 'var desdeEval = 6 * 7;');
          console.log(desdeEval, eval(5), typeof eval);"#,
    );
    assert_eq!(r, ["1 2 1 1 2 1,2 1,2 ,,2,3,7,0", "42 5 function"]);
}

/// `Intl` en español e inglés (valores de Chromium; la hora es UTC).
#[test]
fn intl_basico() {
    let r = salida(
        r#"var d = new Date(Date.UTC(2024, 0, 2, 13, 5, 9));
          console.log(new Intl.NumberFormat('en-US').format(1234567.891), new Intl.NumberFormat('es-ES').format(1234.5), new Intl.NumberFormat('es-ES').format(12345.678));
          console.log(new Intl.NumberFormat('en-US', {style: 'currency', currency: 'USD'}).format(-9.5), new Intl.NumberFormat('es-ES', {style: 'currency', currency: 'EUR'}).format(1234.5), new Intl.NumberFormat('en', {style: 'percent'}).format(0.256));
          console.log(new Intl.DateTimeFormat('en-US', {timeZone: 'UTC'}).format(d), new Intl.DateTimeFormat('es-ES', {timeZone: 'UTC'}).format(d));
          console.log(new Intl.DateTimeFormat('en-US', {year: 'numeric', month: 'long', day: 'numeric', timeZone: 'UTC'}).format(d), '|', new Intl.DateTimeFormat('es-ES', {year: 'numeric', month: 'long', day: 'numeric', timeZone: 'UTC'}).format(d));
          console.log(new Intl.DateTimeFormat('en-US', {hour: 'numeric', minute: '2-digit', timeZone: 'UTC'}).format(d), new Intl.DateTimeFormat('es-ES', {hour: 'numeric', minute: '2-digit', timeZone: 'UTC'}).format(d));
          console.log(new Intl.PluralRules('en').select(1), new Intl.PluralRules('en').select(2), new Intl.PluralRules('en', {type: 'ordinal'}).select(22));
          console.log(new Intl.ListFormat('en').format(['a', 'b', 'c']), '|', new Intl.ListFormat('es').format(['a', 'b', 'c']), '|', new Intl.ListFormat('en', {type: 'disjunction'}).format(['x', 'y']));
          console.log(new Intl.RelativeTimeFormat('en').format(-3, 'day'), '|', new Intl.RelativeTimeFormat('es').format(2, 'hours'), '|', new Intl.RelativeTimeFormat('es', {numeric: 'auto'}).format(-1, 'day'));
          console.log(typeof Intl.DateTimeFormat().resolvedOptions().timeZone, (1234.5).toLocaleString('en-US'), new Intl.Locale('es-PE').region);"#,
    );
    assert_eq!(
        r,
        [
            "1,234,567.891 1234,5 12.345,678",
            "-$9.50 1234,50\u{a0}€ 26%",
            "1/2/2024 2/1/2024",
            "January 2, 2024 | 2 de enero de 2024",
            "1:05 PM 13:05",
            "one other two",
            "a, b, and c | a, b y c | x or y",
            "3 days ago | dentro de 2 horas | ayer",
            "string 1,234.5 PE",
        ]
    );
}

/// Un objeto con miles de claves (modo diccionario): leer, escribir, borrar y recorrer desde
/// los mismos sitios del código, para que una caché equivocada se notara.
#[test]
fn objetos_enormes_en_modo_diccionario() {
    let r = salida(
        r#"function leer(o, k) { return o[k]; }
          function x(o) { return o.k5; }
          var o = {}, p = {};
          for (var i = 0; i < 3000; i++) { o['k' + i] = i; if (i < 10) p['k' + i] = -i; }
          var s = 0;
          for (var j = 0; j < 3000; j++) s += leer(o, 'k' + j);
          var antes = [x(o), x(p), x(o)];
          delete o.k5; delete o.k0;
          o.nueva = 'n';
          var despues = [x(o), x(p), leer(o, 'k6'), leer(o, 'k2999'), o.nueva];
          o.k5 = 55;
          var keys = Object.keys(o), n = 0; for (var q in o) n++;
          console.log(s, antes.join(), despues.join(), x(o), keys.length, n, keys[0], keys[keys.length - 1], JSON.stringify(o).length > 30000);"#,
    );
    assert_eq!(r, ["4498500 5,-5,5 ,-5,6,2999,n 55 3000 3000 k1 k5 true"]);
}

/// Un `set` definido en un prototipo después de que una caché aprendiera «añadir `x`» se llama
/// igual: el stub guarda las formas de la cadena. Y un módulo con miles de `export`.
#[test]
fn setter_nuevo_tras_cache_de_anadir() {
    let r = salida(
        r#"function poner(o) { o.x = 1; }
          var P = {}, log = [];
          poner(Object.create(P)); poner(Object.create(P)); poner(Object.create(P));
          Object.defineProperty(P, 'x', { set: function (v) { log.push('set ' + v); } });
          var b = Object.create(P); poner(b);
          function poner2(o) { o.y = 2; }
          poner2({}); poner2({});
          Object.defineProperty(Object.prototype, 'y', { set: function (v) { log.push('y ' + v); }, configurable: true });
          var c = {}; poner2(c);
          delete Object.prototype.y;
          console.log(log.join(), b.hasOwnProperty('x'), c.hasOwnProperty('y'));"#,
    );
    assert_eq!(r, ["set 1,y 2 false false"]);
}

/// Los arreglos tipados son vistas sobre los bytes de su ArrayBuffer (sin copiar): `subarray`
/// escribe en el original, `DataView` y las vistas ven lo mismo. El resultado es el de Chromium.
#[test]
fn vistas_sobre_los_mismos_bytes() {
    let r = salida(
        r#"var out = [];
var b = new ArrayBuffer(8), u8 = new Uint8Array(b), dv = new DataView(b);
dv.setUint32(0, 0x01020304); dv.setFloat32(4, 1.5, true);
out.push(Array.from(u8).join(','));
var s = u8.subarray(2, 6); s[0] = 99;
out.push(u8[2] + ' ' + s.length + ' ' + s.byteOffset + ' ' + (s.buffer === b));
var u16 = new Uint16Array(b, 2, 2); out.push(u16[0] + ' ' + u16.length + ' ' + u16.byteLength);
var i8 = new Int8Array([200, -1, 3.7]); out.push(Array.from(i8).join(','));
var c = new Uint8ClampedArray([300, -5, 2.5, 3.5]); out.push(c.join(','));
var f = new Float32Array([0.1]); out.push(f[0]);
var sl = u8.slice(1, 3); sl[0] = 7; out.push(u8[1] + ' ' + sl.join(','));
var t = new Uint8Array(4); t.set([1, 2], 1); t.set(new Uint8Array([9]), 3); out.push(t.join(','));
out.push(new Uint8Array(5).fill(4, 1, -1).join(','));
out.push(Array.isArray(u8) + ' ' + ArrayBuffer.isView(u8) + ' ' + ArrayBuffer.isView(dv) + ' ' + Object.prototype.toString.call(u8));
out.push([...new Uint16Array([5, 6])].join(','));
out.push(new TextDecoder().decode(new TextEncoder().encode('héllo €')) + ' ' + new TextEncoder().encode('é').length);
out.push(u8.indexOf(99) + ' ' + u8.includes(4) + ' ' + new Uint8Array([3,1,2]).sort().join(','));
out.push(new Uint8Array([1,2,3]).map(function (x) { return x * 100; }).join(','));
out.push(dv.getUint16(0) + ' ' + dv.getUint16(0, true) + ' ' + dv.getInt8(0) + ' ' + dv.getFloat32(4, true));
var big = new Uint8Array(1 << 20); for (var k = 0; k < big.length; k += 4096) big[k] = k & 255;
var acc = 0; for (var r = 0; r < 200; r++) { var v = big.subarray(r * 10, r * 10 + 500000); acc += v[0] + v.length; }
out.push(acc);
out.push(new Uint8Array(b, 1).length + ' ' + b.slice(-2).byteLength + ' ' + Math.fround(5.5) + ' ' + Math.fround(0.1));
u8.length = 3; out.push(u8.length + ' ' + u8[100]);
out.push(JSON.stringify(Object.keys(new Uint8Array(2))) + ' ' + ('1' in new Uint8Array(2)) + ' ' + (5 in new Uint8Array(2)));
console.log(out.join(' | '));
"#,
    );
    assert_eq!(
        r,
        [
            r#"1,2,3,4,0,0,192,63 | 99 4 2 true | 1123 2 4 | -56,-1,3 | 255,0,2,4 | 0.10000000149011612 | 2 7,99 | 0,1,2,9 | 0,4,4,4,0 | false true true [object Uint8Array] | 5,6 | héllo € 2 | 2 true 1,2,3 | 100,200,44 | 258 513 1 1.5 | 100000000 | 7 2 5.5 0.10000000149011612 | 8 undefined | ["0","1"] true false"#
        ]
    );
}
