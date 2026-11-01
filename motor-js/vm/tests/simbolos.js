var s = Symbol('clave'), t = Symbol('clave');
var o = { a: 1 };
o[s] = 'oculto';
console.log(typeof s, s === t, s.description, String(s), o[s], o[t], s in o, Object.keys(o).join(), JSON.stringify(o));
console.log(Symbol.for('app') === Symbol.for('app'), Symbol.keyFor(Symbol.for('app')), Symbol.keyFor(s), typeof Symbol.iterator);
class Rango {
  constructor(n) { this.n = n; }
  [Symbol.iterator]() {
    var i = 0, n = this.n;
    return { next: function () { return i < n ? { value: i++, done: false } : { value: undefined, done: true }; } };
  }
}
var r = [];
for (var x of new Rango(3)) r.push(x);
console.log(r.join(), [...new Rango(2)].join(), Array.from(new Rango(4)).length, Math.max(...new Rango(5)));
var it = [10, 20][Symbol.iterator]();
console.log(it.next().value, it.next().value, it.next().done, typeof new Map()[Symbol.iterator], typeof 'ab'[Symbol.iterator]);
var [p, q] = new Set(['x', 'y']);
console.log(p, q);
try { new Symbol(); } catch (e) { console.log('new Symbol falla'); }
