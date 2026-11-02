function tarde(v) { return new Promise(function (r) { r(v); }); }
async function* numeros(n) {
  for (let i = 0; i < n; i++) {
    const v = await tarde(i * 10);
    yield v;
  }
  return 'fin';
}
async function* fallido() { yield 1; throw new Error('roto'); }
(async function () {
  const vistos = [];
  for await (const x of numeros(3)) vistos.push(x);
  console.log('for await', vistos.join());
  const it = numeros(2);
  console.log(JSON.stringify(await it.next()), JSON.stringify(await it.next()), JSON.stringify(await it.next()));
  try { for await (const v of fallido()) console.log('valor', v); } catch (e) { console.log('error', e.message); }
  const sincronos = [];
  for await (const v of [tarde('a'), 'b', tarde('c')]) sincronos.push(v);
  console.log('sobre promesas', sincronos.join());
  const obj = { async *[Symbol.asyncIterator]() { yield 'propio'; } };
  for await (const v of obj) console.log(v);
  console.log(typeof numeros(1)[Symbol.asyncIterator], typeof numeros(1).next);
})();
console.log(/a/gi.flags, /a/.source, /b/s.dotAll, /./.dotAll === false, /x/y.sticky, new RegExp('').source);
console.log((function () { try { new Function('try { async function* x() {} } catch {}'); return true; } catch (e) { return false; } }()));
