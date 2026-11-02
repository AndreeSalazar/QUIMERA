//! La biblioteca estándar frente a Chromium: `Math`, números a texto, huecos de arreglo y los
//! mensajes de error. Los resultados esperados son los de Chromium con el mismo código.

use quimera_vm::Motor;

/// Ejecuta `codigo`, luego las microtareas, y devuelve lo escrito con `console.log`.
fn salida(codigo: &str) -> Vec<String> {
    let mut m = Motor::default();
    m.ejecutar(codigo).unwrap_or_else(|e| panic!("{}", e.mensaje));
    let errores = m.correr_microtareas();
    assert!(errores.is_empty(), "{errores:?}");
    m.salida().to_vec()
}

/// Llamar a una variable que no es una función dice su nombre (local o de una función de fuera).
#[test]
fn error_con_el_nombre_de_lo_llamado() {
    let r = salida(
        r#"var global1;
          function f() { var local = 3; try { local(); } catch (e) { console.log(e.message); }
            return function () { try { global1(1, 2); } catch (e) { console.log(e.message); } }; }
          f()();"#,
    );
    assert_eq!(r, ["local no es una función (vale 3)", "global1 no es una función (vale undefined)"]);
}

/// Toda `Math`, con valores de Chromium en la trigonometría (fdlibm, como V8).
#[test]
fn math_completo() {
    let r = salida(
        r#"var n = ['abs','acos','acosh','asin','asinh','atan','atan2','atanh','cbrt','ceil','clz32','cos','cosh','exp','expm1','floor','fround','hypot','imul','log','log10','log1p','log2','max','min','pow','random','round','sign','sin','sinh','sqrt','tan','tanh','trunc'];
          console.log(n.filter(function (k) { return typeof Math[k] !== 'function'; }).join() || 'todas');
          console.log(Math.sin(1), Math.cos(1), Math.tan(1), Math.sin(100), Math.cos(30 * Math.PI / 180));
          console.log(Math.atan(0.5), Math.atan2(1, -1), Math.asin(0.3), Math.acos(-0.2), Math.atan2(-0, 1));
          console.log(Math.asinh(1), Math.acosh(2), Math.atanh(0.5), Math.sin(Math.PI));"#,
    );
    assert_eq!(
        r,
        [
            "todas",
            "0.8414709848078965 0.5403023058681398 1.5574077246549023 -0.5063656411097588 0.8660254037844387",
            "0.4636476090008061 2.356194490192345 0.3046926540153975 1.7721542475852274 0",
            "0.881373587019543 1.3169578969248166 0.5493061443340549 1.2246467991473532e-16",
        ]
    );
}

/// Números a texto como la especificación (`Number::toString`): exponente por debajo de 1e-6 y
/// desde 1e21. Valores de Chromium.
#[test]
fn numeros_a_texto() {
    let r = salida(
        r#"console.log([1.2246467991473532e-16, 0.000001, 0.0000001, 1e21, 1e20, 123456789012345680000, 1.5e-7, -2.5e-10, 0.1 + 0.2, 1 / 3, 5e-324, 1.7976931348623157e308, 123.456, -0.0000012].join(' '));"#,
    );
    assert_eq!(
        r,
        [
            "1.2246467991473532e-16 0.000001 1e-7 1e+21 100000000000000000000 123456789012345680000 1.5e-7 -2.5e-10 0.30000000000000004 0.3333333333333333 5e-324 1.7976931348623157e+308 123.456 -0.0000012"
        ]
    );
}

/// Huecos de arreglo: `Array(n)`, `delete`, escribir más allá del final y `length` mayor. Los
/// saltan `forEach`, `map` (que los conserva), `filter`, `reduce`, `in`, `Object.keys` y `for-in`.
/// Valores de Chromium.
#[test]
fn huecos_de_arreglo() {
    let r = salida(
        r#"var a = Array(3).concat('x', 'y'), visto = [];
          a.forEach(function (v, i) { visto.push(i); });
          var m = a.map(function (v) { return v + '!'; });
          console.log(a.length, visto.join(), m.length, 0 in m, m[3], JSON.stringify(m), Object.keys(a).join());
          var b = [1, 2, 3]; delete b[1]; var c = []; c[3] = 'z'; var d = [1]; d.length = 3;
          var k = []; for (var i in c) k.push(i);
          console.log(1 in b, b[1], b.filter(function () { return true; }).length, k.join(), 2 in d, d.reduce(function (s, x) { return s + x; }), a.indexOf(undefined), a.includes(undefined));
          console.log(c.hasOwnProperty(0), c.hasOwnProperty(3), [, 'b'].some(function (x) { return x === undefined; }), a.slice(1, 4).length, 1 in a.slice(1, 4), String(Array(3)), Array(2).join('-'));
          console.log([...Array(3)].map((_, i) => i).join(), 0 in [...Array(2)], Array.from(Array(2)).length, 0 in Array.from(Array(2)));"#,
    );
    assert_eq!(
        r,
        [
            r#"5 3,4 5 false x! [null,null,null,"x!","y!"] 3,4"#,
            "false undefined 2 3 false 1 -1 true",
            "false true false 3 false ,, -",
            "0,1,2 true 2 true",
        ]
    );
}

/// `Object.getOwnPropertyNames` incluye lo no enumerable (accesores de clases, `defineProperty`).
/// Valores de Chromium.
#[test]
fn nombres_propios() {
    let r = salida(
        r#"var o = {a: 1}; Object.defineProperty(o, 'b', {value: 2}); Object.defineProperty(o, 'c', {get: function () { return 3; }});
          class K { m() {} get g() { return 1; } }
          console.log(Object.getOwnPropertyNames(o).join(), Object.getOwnPropertyNames(K.prototype).join(), Object.getOwnPropertyNames([5, 6]).join(), Object.getOwnPropertyNames(Math).indexOf('PI') >= 0);"#,
    );
    assert_eq!(r, ["a,b,c constructor,m,g 0,1,length true"]);
}

/// Un decorador de TypeScript que borra un getter: `defineProperty` con `get: undefined` lo
/// quita; sin la clave `get`, se conserva (`get x` y `set x` por separado). Valores de Chromium.
#[test]
fn decorador_que_borra_el_getter() {
    let r = salida(
        r#"function borrar(proto, nombre, desc) { proto['__compute__' + nombre] = desc.get; desc.get = void 0; }
          var __decorate = function (ds, t, k, d) { var r = d === null ? Object.getOwnPropertyDescriptor(t, k) : d;
            for (var i = ds.length - 1; i >= 0; i--) r = ds[i](t, k, r) || r; Object.defineProperty(t, k, r); return r; };
          function C() {} Object.defineProperty(C.prototype, 'x', { get: function () { return 'getter'; }, configurable: true });
          __decorate([borrar], C.prototype, 'x', null);
          var o = new C(), d = Object.getOwnPropertyDescriptor(C.prototype, 'x');
          var p = {}; Object.defineProperty(p, 'y', { get: function () { return 1; }, configurable: true }); Object.defineProperty(p, 'y', { set: function () {} });
          console.log(o.x, typeof d.get, typeof C.prototype.__compute__x, p.y, typeof Object.getOwnPropertyDescriptor(p, 'y').set);"#,
    );
    assert_eq!(r, ["undefined undefined function 1 function"]);
}

/// `name` y `length` de las funciones (también el nombre inferido de `var f = function`),
/// `obj.__proto__ = p`, y `toString` que no hace pasar por nativas a las funciones de la página.
/// Valores de Chromium (salvo el `length` de las nativas, que QUIMERA no sabe: da 0).
#[test]
fn nombres_de_funciones_y_proto() {
    let r = salida(
        r#"function A(x, y) {} class K { constructor(a) {} m() {} } var f = function () {}, g = (a, b, c) => 1, o = { met() {} };
          console.log(A.name, A.length, K.name, K.length, f.name, g.name, g.length, o.met.name, Math.max.name, (function () {}).name === '');
          var P = { m: function () { return 'm'; } }, q = {}; q.__proto__ = P;
          console.log(q.m(), q.__proto__ === P, Object.getPrototypeOf(q) === P, /\[native code\]/.test(String(A)), /\[native code\]/.test(String(Math.max)));"#,
    );
    assert_eq!(r, ["A 2 K 1 f g 3 met max true", "m true true false true"]);
}

/// `new` sobre una función atada con `bind` ignora el `this` atado pero conserva los argumentos.
#[test]
fn new_sobre_bind() {
    let r = salida(
        r#"function C(a, b) { this.a = a; this.b = b; }
          C.prototype.suma = function () { return this.a + this.b; };
          var B = Function.prototype.bind.apply(C, [null, 1]);
          var o = new B(2);
          console.log(o.suma(), o instanceof C, o instanceof B);
          class K { constructor(x) { this.x = x; } }
          var k = new (K.bind(null, 7))();
          console.log(k.x, k instanceof K);"#,
    );
    assert_eq!(r, ["3 true true", "7 true"]);
}
