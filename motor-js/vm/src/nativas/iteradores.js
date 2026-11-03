// Los métodos de los iteradores (Iterator helpers, ES2025) sobre `Iterator.prototype`: `map`,
// `filter`, `take`, `drop`, `flatMap` dan otro iterador perezoso; `toArray`, `forEach`, `some`,
// `every`, `find` y `reduce` lo recorren. `Iterator.from` envuelve cualquier iterable.
(function (global) {
  var P = global.Iterator.prototype;
  // Un iterador nuevo que cuelga de Iterator.prototype y saca sus valores de `siguiente`.
  function nuevo(siguiente, fuente) {
    var it = Object.create(P);
    it.next = siguiente;
    it['return'] = function (v) {
      if (fuente && typeof fuente['return'] === 'function') fuente['return']();
      return { value: v, done: true };
    };
    __quimera_no_enumerar(it);
    return it;
  }
  function funcion(f, quien) {
    if (typeof f !== 'function') throw new TypeError(quien + ' necesita una función');
  }
  var metodos = {
    map: function (f) {
      funcion(f, 'map');
      var yo = this, i = 0;
      return nuevo(function () {
        var r = yo.next();
        return r.done ? r : { value: f(r.value, i++), done: false };
      }, yo);
    },
    filter: function (f) {
      funcion(f, 'filter');
      var yo = this, i = 0;
      return nuevo(function () {
        for (;;) {
          var r = yo.next();
          if (r.done || f(r.value, i++)) return r;
        }
      }, yo);
    },
    take: function (n) {
      var yo = this, quedan = Number(n);
      if (!(quedan >= 0)) throw new RangeError('take necesita un número no negativo');
      return nuevo(function () {
        if (quedan <= 0) { if (typeof yo['return'] === 'function') yo['return'](); return { value: undefined, done: true }; }
        quedan--;
        return yo.next();
      }, yo);
    },
    drop: function (n) {
      var yo = this, saltar = Number(n);
      if (!(saltar >= 0)) throw new RangeError('drop necesita un número no negativo');
      return nuevo(function () {
        for (; saltar > 0; saltar--) { var r = yo.next(); if (r.done) return r; }
        return yo.next();
      }, yo);
    },
    flatMap: function (f) {
      funcion(f, 'flatMap');
      var yo = this, dentro = null, i = 0;
      return nuevo(function () {
        for (;;) {
          if (dentro) { var d = dentro.next(); if (!d.done) return d; dentro = null; }
          var r = yo.next();
          if (r.done) return r;
          dentro = f(r.value, i++)[Symbol.iterator]();
        }
      }, yo);
    },
    toArray: function () { var a = [], r; while (!(r = this.next()).done) a.push(r.value); return a; },
    forEach: function (f) { funcion(f, 'forEach'); var r, i = 0; while (!(r = this.next()).done) f(r.value, i++); },
    some: function (f) { funcion(f, 'some'); var r, i = 0; while (!(r = this.next()).done) if (f(r.value, i++)) return true; return false; },
    every: function (f) { funcion(f, 'every'); var r, i = 0; while (!(r = this.next()).done) if (!f(r.value, i++)) return false; return true; },
    find: function (f) { funcion(f, 'find'); var r, i = 0; while (!(r = this.next()).done) if (f(r.value, i++)) return r.value; },
    reduce: function (f, inicial) {
      funcion(f, 'reduce');
      var r, acc = inicial, i = 0, hay = arguments.length > 1;
      while (!(r = this.next()).done) { acc = hay ? f(acc, r.value, i) : r.value; hay = true; i++; }
      if (!hay) throw new TypeError('reduce de un iterador vacío sin valor inicial');
      return acc;
    }
  };
  Object.keys(metodos).forEach(function (k) {
    Object.defineProperty(P, k, { value: metodos[k], writable: true, configurable: true, enumerable: false });
  });
  global.Iterator.from = function (x) {
    var it = typeof x[Symbol.iterator] === 'function' ? x[Symbol.iterator]() : x;
    if (it instanceof global.Iterator) return it;
    return nuevo(function () { return it.next(); }, it);
  };
})(this);
