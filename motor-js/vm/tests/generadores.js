function* contar(n) {
  for (let i = 0; i < n; i++) {
    var recibido = yield i;
    if (recibido) console.log('recibido', recibido);
  }
  return 'fin';
}
var g = contar(3);
console.log(JSON.stringify(g.next()), JSON.stringify(g.next('hola')), JSON.stringify(g.next()), JSON.stringify(g.next()), JSON.stringify(g.next()));
console.log([...contar(4)].join(), Array.from(contar(2)).join());
for (const x of contar(2)) console.log('of', x);
function* errores() {
  try { yield 1; yield 2; } catch (e) { console.log('capturado', e); yield 'tras catch'; }
}
var e = errores();
console.log(e.next().value, e.throw('fallo').value, JSON.stringify(e.next()));
var r = contar(5); r.next();
console.log(JSON.stringify(r.return('corto')), JSON.stringify(r.next()));
function* dentro() { yield 'a'; yield 'b'; return 'r'; }
function* fuera() { yield 0; var v = yield* dentro(); yield v; yield* [7, 8]; }
console.log([...fuera()].join());
var obj = { *pares() { yield 2; yield 4; }, [Symbol.iterator]: function* () { yield 'x'; } };
class Arbol { constructor(v, h) { this.v = v; this.h = h || []; } *[Symbol.iterator]() { yield this.v; for (const h of this.h) yield* h; } }
console.log([...obj.pares()].join(), [...obj].join(), [...new Arbol(1, [new Arbol(2), new Arbol(3, [new Arbol(4)])])].join());
var anon = function* (a, b) { yield this.k + a + b; yield arguments.length; };
console.log([...anon.call({ k: 10 }, 1, 2)].join(), typeof g.next, g[Symbol.iterator]() === g);
function* infinito() { let n = 0; while (true) yield n++; }
var inf = infinito(), primeros = [];
for (const v of [1, 2, 3]) primeros.push(inf.next().value);
console.log(primeros.join());
try { new contar(1); } catch (err) { console.log('no es constructor'); }
