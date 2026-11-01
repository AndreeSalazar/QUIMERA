var o = {
  _x: 1,
  get x() { return this._x * 10; },
  set x(v) { this._x = v; },
  get soloLectura() { return 'fijo'; },
  ['calc' + 'ulada']: 5,
  get ['din' + 'amico']() { return 'din'; }
};
o.x = 4;
o.soloLectura = 'cambiar';
console.log(o.x, o._x, o.soloLectura, o.calculada, o.dinamico, 'x' in o, o.hasOwnProperty('x'));
console.log(Object.keys(o).join(), JSON.stringify(o));
class Temperatura {
  #c = 0;
  static get unidad() { return 'C'; }
  get celsius() { return this.#c; }
  set celsius(v) { this.#c = v; }
  get fahrenheit() { return this.#c * 9 / 5 + 32; }
  set fahrenheit(f) { this.celsius = (f - 32) * 5 / 9; }
}
var t = new Temperatura();
t.fahrenheit = 212;
var claves = []; for (var k in t) claves.push(k);
console.log(t.celsius, t.fahrenheit, Temperatura.unidad, claves.join(), 'celsius' in t, t.hasOwnProperty('celsius'));
class Kelvin extends Temperatura { get kelvin() { return this.celsius + 273; } }
var kv = new Kelvin(); kv.celsius = 10;
console.log(kv.kelvin, kv.fahrenheit);
var p = {};
Object.defineProperty(p, 'v', { get: function () { return 'definido'; }, enumerable: true, configurable: true });
var d = Object.getOwnPropertyDescriptor(p, 'v');
console.log(p.v, typeof d.get, d.set, Object.getOwnPropertyDescriptor({a: 1}, 'a').value);
Object.defineProperty(p, 'v', { value: 'datos' });
console.log(p.v, Object.getPrototypeOf(kv) === Kelvin.prototype);
var veces = 0, q = { get cuenta() { return ++veces; } };
for (var i = 0; i < 5; i++) q.cuenta;
console.log(veces, q.cuenta);
var hijo = Object.create(o);
hijo.x = 7;
console.log(hijo.x, hijo.hasOwnProperty('_x'), o.x);
