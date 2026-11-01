var n = 3, o = {a: {b: [10, 20]}, f: function () { return this.a.b[0]; }};
console.log(`plantilla ${n} + ${n * 2} = ${n + n * 2}`, `sin nada`, `${'anidada ' + `dentro ${n}`}`, `línea
nueva`.length);
function etiqueta(trozos, x, y) { return trozos.join('|') + ':' + x + ',' + y; }
console.log(etiqueta`a${1}b${2}c`);
function suma(a, b, c) { return a + b + c; }
var lista = [1, 2];
console.log(suma(...lista, 3), Math.max(...[4, 9, 2]), [0, ...lista, ...'ab', 5].join(), o.f(...[]));
function resto(a, ...otros) { return a + ':' + otros.join('-'); }
var flecha = (...xs) => xs.length;
console.log(resto(1, 2, 3), resto(1), flecha(1, 2, 3));
var {a: {b: [primero, segundo = 7, tercero = 99]}, z = 'defecto', ...demas} = {a: {b: [1, undefined]}, q: 1, r: 2};
console.log(primero, segundo, tercero, z, JSON.stringify(demas));
var [x, , y, ...cola] = 'abcdef';
console.log(x, y, cola.join(''));
function params({nombre, edad = 30}, [p, q] = [8, 9]) { return nombre + edad + p + q; }
console.log(params({nombre: 'ana'}), params({nombre: 'luis', edad: 1}, [2, 3]));
var u = 1, v = 2;
[u, v] = [v, u];
var t = {};
({k: t.uno, l: t.dos = 'd'} = {k: 'k'});
console.log(u, v, t.uno, t.dos);
var pares = [];
for (const [clave, valor] of Object.entries({p: 1, q: 2})) pares.push(clave + valor);
for (const {id} of [{id: 'x'}, {id: 'y'}]) pares.push(id);
console.log(pares.join());
var nada = null;
console.log(o?.a?.b[1], nada?.a.b.c, nada?.[0], o.f?.(), o.noEsta?.(), nada ?? 'reserva', 0 ?? 1);
var obj = {...o.a, extra: 1, ...{b: 'pisado'}};
console.log(JSON.stringify(obj), {a: 1, ...null}.a);
class Animal {
  patas = 4;
  static creados = 0;
  constructor(nombre) { this.nombre = nombre; Animal.creados++; }
  hablar() { return `${this.nombre} hace ruido`; }
  static especie() { return 'animal'; }
}
class Perro extends Animal {
  #secreto = 'hueso';
  constructor(nombre) { super(nombre); this.patas += 0; }
  hablar() { return super.hablar() + ' (guau)'; }
  secreto() { return this.#secreto; }
  static especie() { return 'perro de ' + super.especie(); }
}
var p = new Perro('rex');
var enumeradas = []; for (var k in p) enumeradas.push(k);
console.log(p.hablar(), p.patas, p.secreto(), Perro.especie(), Animal.creados, p instanceof Animal, p.constructor === Perro);
console.log(enumeradas.sort().join(), typeof Perro, new (class { valor() { return 5; } })().valor());
class MiError extends Error { constructor(m) { super(m); this.name = 'MiError'; } }
try { throw new MiError('fallo'); } catch (e) { console.log(e.name, e.message, e instanceof Error, e instanceof MiError); }
class Lista extends Map { total() { return this.size; } }
console.log(new Lista([[1, 2], [3, 4]]).total(), new (Date)(0).getTime());
