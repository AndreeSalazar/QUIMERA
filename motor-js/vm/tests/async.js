function esperar(v) { return new Promise(function (r) { r(v); }); }
async function suma(a, b) {
  var x = await esperar(a);
  var y = await b;
  return x + y;
}
async function falla() { await null; throw new TypeError('mal'); }
const flecha = async (n) => (await esperar(n)) * 2;
const flechaSimple = async n => n + 1;
class Servicio {
  constructor() { this.base = 100; }
  async cargar(n) { const v = await esperar(n); return this.base + v; }
  static async crear() { return new Servicio(); }
}
var obj = { async metodo() { return 'metodo'; } };
console.log('inicio');
suma(1, 2).then(function (v) { console.log('suma', v); });
falla().catch(function (e) { console.log('falla', e.name, e.message); });
flecha(5).then(function (v) { console.log('flecha', v); });
flechaSimple(1).then(function (v) { console.log('simple', v); });
Servicio.crear().then(function (s) { return s.cargar(23); }).then(function (v) { console.log('servicio', v); });
obj.metodo().then(console.log);
(async function () {
  try { await falla(); } catch (e) { console.log('capturado con await', e.message); }
  const todos = await Promise.all([suma(1, 1), flecha(2), 3]);
  console.log('todos', todos.join());
  for (const p of [esperar('a'), esperar('b')]) console.log('bucle', await p);
  return 'listo';
})().then(function (v) { console.log(v); });
var p = suma(1, 1);
console.log('fin', p instanceof Promise, typeof suma);
