// La biblioteca de QUIMERA escrita en JavaScript (como el «self-hosting» de SpiderMonkey):
// lo que no necesita ser rápido ni tocar el montón por dentro. Se ejecuta al crear cada motor.
//
// - Solo usa JavaScript que la VM ya entiende (ES5 y poco más).
// - Las ranuras internas son propiedades cuyo nombre empieza por U+0001: no salen en for-in,
//   Object.keys ni JSON.stringify, y desde JavaScript normal no se escriben por casualidad.
// - Al terminar deja en __quimera_ocultar los prototipos que crea: sus métodos no salen en
//   for-in (como los de la biblioteca en Rust).
(function (global) {
  var slice = Array.prototype.slice;
  var ocultar = [];

  function definir(objeto, metodos) {
    for (var nombre in metodos) objeto[nombre] = metodos[nombre];
    // for-in no ve las ranuras internas: la única que llevan los métodos se copia aparte.
    if (metodos['\u0001lista']) objeto['\u0001lista'] = metodos['\u0001lista'];
  }

  // ---------------- errores ----------------

  function claseDeError(nombre) {
    function E(mensaje) {
      var e = this instanceof E ? this : Object.create(E.prototype);
      if (mensaje !== undefined) e.message = String(mensaje);
      e.stack = String(e) + __quimera_traza();
      return e;
    }
    E.prototype = Object.create(Error.prototype);
    E.prototype.constructor = E;
    E.prototype.name = nombre;
    ocultar.push(E.prototype);
    global[nombre] = E;
  }
  ['TypeError', 'RangeError', 'SyntaxError', 'ReferenceError', 'EvalError', 'URIError'].forEach(claseDeError);

  // ---------------- Function, Object ----------------

  Function.prototype.bind = function (este) {
    var f = this, previos = slice.call(arguments, 1);
    if (typeof f !== 'function') throw new TypeError('bind necesita una función');
    return function () {
      return f.apply(este, previos.concat(slice.call(arguments)));
    };
  };

  definir(Object, {
    assign: function (destino) {
      if (destino == null) throw new TypeError('Object.assign necesita un objeto');
      for (var i = 1; i < arguments.length; i++) {
        var fuente = arguments[i];
        if (fuente == null) continue;
        var claves = Object.keys(fuente);
        for (var j = 0; j < claves.length; j++) destino[claves[j]] = fuente[claves[j]];
      }
      return destino;
    },
    entries: function (o) {
      return Object.keys(o).map(function (k) { return [k, o[k]]; });
    },
    values: function (o) {
      return Object.keys(o).map(function (k) { return o[k]; });
    },
    fromEntries: function (pares) {
      var o = {};
      aLista(pares).forEach(function (p) { o[p[0]] = p[1]; });
      return o;
    },
    // QUIMERA no tiene atributos de propiedad: congelar no hace nada (y se dice que no lo está).
    freeze: function (o) { return o; },
    seal: function (o) { return o; },
    preventExtensions: function (o) { return o; },
    isFrozen: function () { return false; },
    isSealed: function () { return false; },
    isExtensible: function () { return true; },
    is: function (a, b) {
      if (a === b) return a !== 0 || 1 / a === 1 / b;
      return a !== a && b !== b;
    },
    defineProperties: function (o, ds) {
      Object.keys(ds).forEach(function (k) { Object.defineProperty(o, k, ds[k]); });
      return o;
    }
  });

  // ---------------- listas e iteradores ----------------

  // Lo que se puede recorrer, como arreglo: arreglos, cadenas, Map/Set y sus iteradores,
  // objetos con next(), y los que tienen length.
  function aLista(x) {
    if (x == null) throw new TypeError(x + ' no es iterable');
    // Una copia sin huecos: `[...Array(3)]` son tres `undefined` de verdad.
    if (Array.isArray(x)) return __quimera_densa(x);
    if (typeof x === 'string') return x.split('');
    var lista = x['\u0001lista'];
    if (typeof lista === 'function') return lista.call(x);
    var iterar = x[Symbol.iterator];
    if (typeof iterar === 'function') x = iterar.call(x);
    if (typeof x.next === 'function') {
      var r = [], paso;
      while (!(paso = x.next()).done) r.push(paso.value);
      return r;
    }
    var n = x.length >>> 0, a = [];
    for (var i = 0; i < n; i++) a.push(x[i]);
    return a;
  }
  global.__quimera_lista = aLista;

  function Iterador(lista) {
    this['\u0001elementos'] = lista;
    this['\u0001i'] = 0;
  }
  definir(Iterador.prototype, {
    next: function () {
      var l = this['\u0001elementos'], i = this['\u0001i'];
      if (i >= l.length) return { value: undefined, done: true };
      this['\u0001i'] = i + 1;
      return { value: l[i], done: false };
    },
    '\u0001lista': function () { return this['\u0001elementos'].slice(this['\u0001i']); }
  });
  Iterador.prototype[Symbol.iterator] = function () { return this; };
  ocultar.push(Iterador.prototype);

  definir(Array, {
    from: function (x, f, este) {
      var a = aLista(x);
      return typeof f === 'function' ? a.map(f, este) : a;
    },
    of: function () { return slice.call(arguments); }
  });

  definir(Array.prototype, {
    splice: function (inicio, cuantos) {
      var n = this.length;
      inicio = inicio < 0 ? Math.max(n + inicio, 0) : Math.min(inicio | 0, n);
      cuantos = arguments.length < 2 ? n - inicio : Math.min(Math.max(cuantos | 0, 0), n - inicio);
      var quitados = this.slice(inicio, inicio + cuantos);
      var nuevos = slice.call(arguments, 2);
      var resto = this.slice(inicio + cuantos);
      this.length = inicio;
      for (var i = 0; i < nuevos.length; i++) this.push(nuevos[i]);
      for (var j = 0; j < resto.length; j++) this.push(resto[j]);
      return quitados;
    },
    lastIndexOf: function (x) {
      for (var i = this.length - 1; i >= 0; i--) if (this[i] === x) return i;
      return -1;
    },
    fill: function (v, inicio, fin) {
      var n = this.length;
      inicio = inicio === undefined ? 0 : inicio < 0 ? n + inicio : inicio;
      fin = fin === undefined ? n : fin < 0 ? n + fin : fin;
      for (var i = inicio; i < fin; i++) this[i] = v;
      return this;
    },
    flat: function (nivel) {
      nivel = nivel === undefined ? 1 : nivel;
      var r = [];
      (function aplanar(a, d) {
        for (var i = 0; i < a.length; i++) {
          if (Array.isArray(a[i]) && d > 0) aplanar(a[i], d - 1);
          else r.push(a[i]);
        }
      })(this, nivel);
      return r;
    },
    flatMap: function (f, este) { return this.map(f, este).flat(1); },
    findLast: function (f, este) {
      for (var i = this.length - 1; i >= 0; i--) if (f.call(este, this[i], i, this)) return this[i];
      return undefined;
    },
    findLastIndex: function (f, este) {
      for (var i = this.length - 1; i >= 0; i--) if (f.call(este, this[i], i, this)) return i;
      return -1;
    },
    at: function (i) {
      i = i | 0;
      return this[i < 0 ? this.length + i : i];
    },
    keys: function () {
      var r = [];
      for (var i = 0; i < this.length; i++) r.push(i);
      return new Iterador(r);
    },
    values: function () { return new Iterador(this.slice()); },
    entries: function () {
      var r = [];
      for (var i = 0; i < this.length; i++) r.push([i, this[i]]);
      return new Iterador(r);
    }
  });

  // ---------------- String, Number, Math ----------------

  function relleno(s, largo, con) {
    con = con === undefined ? ' ' : String(con);
    var falta = largo - s.length;
    if (falta <= 0 || con === '') return '';
    var r = '';
    while (r.length < falta) r += con;
    return r.slice(0, falta);
  }
  definir(String.prototype, {
    padStart: function (largo, con) { var s = String(this); return relleno(s, largo, con) + s; },
    padEnd: function (largo, con) { var s = String(this); return s + relleno(s, largo, con); },
    repeat: function (n) {
      n = n | 0;
      if (n < 0) throw new RangeError('repeat necesita un número positivo');
      var r = '', s = String(this);
      for (var i = 0; i < n; i++) r += s;
      return r;
    },
    trimStart: function () { return String(this).replace(/^\s+/, ''); },
    trimEnd: function () { return String(this).replace(/\s+$/, ''); },
    at: function (i) {
      var s = String(this);
      i = i | 0;
      return s.charAt(i < 0 ? s.length + i : i) || undefined;
    },
    replaceAll: function (buscar, poner) {
      if (typeof buscar !== 'string') return String(this).replace(buscar, poner);
      return String(this).split(buscar).join(poner);
    }
  });

  definir(Number, {
    isInteger: function (n) { return typeof n === 'number' && isFinite(n) && Math.floor(n) === n; },
    isSafeInteger: function (n) { return Number.isInteger(n) && Math.abs(n) <= 9007199254740991; },
    isFinite: function (n) { return typeof n === 'number' && isFinite(n); },
    isNaN: function (n) { return typeof n === 'number' && n !== n; },
    parseFloat: parseFloat,
    parseInt: parseInt,
    EPSILON: 2.220446049250313e-16,
    MAX_SAFE_INTEGER: 9007199254740991,
    MIN_SAFE_INTEGER: -9007199254740991
  });

  definir(Math, {
    sign: function (x) { x = +x; return x > 0 ? 1 : x < 0 ? -1 : x; },
    trunc: function (x) { x = +x; return x < 0 ? Math.ceil(x) : Math.floor(x); },
    log2: function (x) { return Math.log(x) / Math.LN2; },
    log10: function (x) { return Math.log(x) / Math.LN10; },
    cbrt: function (x) { return x < 0 ? -Math.pow(-x, 1 / 3) : Math.pow(x, 1 / 3); },
    hypot: function () {
      var s = 0;
      for (var i = 0; i < arguments.length; i++) s += arguments[i] * arguments[i];
      return Math.sqrt(s);
    },
    LN2: 0.6931471805599453,
    LN10: 2.302585092994046
  });

  // ---------------- Map, Set, WeakMap, WeakSet ----------------

  var HUECO = {};
  var siguienteId = 0;

  // La clave de índice de un valor. Los primitivos se distinguen por su tipo; los objetos,
  // por un número que se les guarda en una ranura la primera vez.
  function claveDe(k) {
    switch (typeof k) {
      case 'string': return 's' + k;
      case 'number': return 'n' + k;
      case 'boolean': return k ? 't' : 'f';
      case 'undefined': return 'u';
    }
    if (k === null) return 'l';
    var id = k['\u0001id'];
    if (id === undefined) {
      id = ++siguienteId;
      k['\u0001id'] = id;
    }
    return 'o' + id;
  }

  function Tabla() {
    this.claves = [];
    this.valores = [];
    this.indice = Object.create(null);
    this.huecos = 0;
    this.recorriendo = 0;
  }
  definir(Tabla.prototype, {
    buscar: function (k) {
      var i = this.indice[claveDe(k)];
      return i === undefined ? -1 : i;
    },
    poner: function (k, v) {
      var c = claveDe(k), i = this.indice[c];
      if (i !== undefined) { this.valores[i] = v; return false; }
      this.indice[c] = this.claves.length;
      this.claves.push(k === 0 ? 0 : k);
      this.valores.push(v);
      return true;
    },
    quitar: function (k) {
      var c = claveDe(k), i = this.indice[c];
      if (i === undefined) return false;
      delete this.indice[c];
      this.claves[i] = HUECO;
      this.valores[i] = undefined;
      this.huecos++;
      if (this.huecos > 32 && this.huecos * 2 > this.claves.length && !this.recorriendo) this.compactar();
      return true;
    },
    compactar: function () {
      var ks = this.claves, vs = this.valores;
      this.claves = [];
      this.valores = [];
      this.indice = Object.create(null);
      this.huecos = 0;
      for (var i = 0; i < ks.length; i++) if (ks[i] !== HUECO) this.poner(ks[i], vs[i]);
    },
    recorrer: function (f) {
      this.recorriendo++;
      try {
        for (var i = 0; i < this.claves.length; i++) if (this.claves[i] !== HUECO) f(this.claves[i], this.valores[i]);
      } finally {
        this.recorriendo--;
      }
    }
  });

  function tabla(o, nombre) {
    var t = o == null ? undefined : o['\u0001tabla'];
    if (!t) throw new TypeError(nombre + ' llamado sobre algo que no lo es');
    return t;
  }

  function Map(iniciales) {
    if (!(this instanceof Map)) throw new TypeError('Map necesita new');
    this['\u0001tabla'] = new Tabla();
    this.size = 0;
    if (iniciales != null) {
      var a = aLista(iniciales);
      for (var i = 0; i < a.length; i++) this.set(a[i][0], a[i][1]);
    }
  }
  definir(Map.prototype, {
    get: function (k) { var t = tabla(this, 'Map.get'), i = t.buscar(k); return i < 0 ? undefined : t.valores[i]; },
    set: function (k, v) { if (tabla(this, 'Map.set').poner(k, v)) this.size++; return this; },
    has: function (k) { return tabla(this, 'Map.has').buscar(k) >= 0; },
    'delete': function (k) {
      if (!tabla(this, 'Map.delete').quitar(k)) return false;
      this.size--;
      return true;
    },
    clear: function () { this['\u0001tabla'] = new Tabla(); this.size = 0; },
    forEach: function (f, este) {
      var m = this;
      tabla(this, 'Map.forEach').recorrer(function (k, v) { f.call(este, v, k, m); });
    },
    keys: function () { var r = []; tabla(this, 'Map.keys').recorrer(function (k) { r.push(k); }); return new Iterador(r); },
    values: function () { var r = []; tabla(this, 'Map.values').recorrer(function (k, v) { r.push(v); }); return new Iterador(r); },
    entries: function () { return new Iterador(this['\u0001lista']()); },
    '\u0001lista': function () {
      var r = [];
      tabla(this, 'Map').recorrer(function (k, v) { r.push([k, v]); });
      return r;
    }
  });

  function Set(iniciales) {
    if (!(this instanceof Set)) throw new TypeError('Set necesita new');
    this['\u0001tabla'] = new Tabla();
    this.size = 0;
    if (iniciales != null) {
      var a = aLista(iniciales);
      for (var i = 0; i < a.length; i++) this.add(a[i]);
    }
  }
  definir(Set.prototype, {
    add: function (v) { if (tabla(this, 'Set.add').poner(v, v)) this.size++; return this; },
    has: function (v) { return tabla(this, 'Set.has').buscar(v) >= 0; },
    'delete': function (v) {
      if (!tabla(this, 'Set.delete').quitar(v)) return false;
      this.size--;
      return true;
    },
    clear: function () { this['\u0001tabla'] = new Tabla(); this.size = 0; },
    forEach: function (f, este) {
      var s = this;
      tabla(this, 'Set.forEach').recorrer(function (k) { f.call(este, k, k, s); });
    },
    values: function () { return new Iterador(this['\u0001lista']()); },
    entries: function () { return new Iterador(this['\u0001lista']().map(function (v) { return [v, v]; })); },
    '\u0001lista': function () {
      var r = [];
      tabla(this, 'Set').recorrer(function (k) { r.push(k); });
      return r;
    }
  });
  Set.prototype.keys = Set.prototype.values;
  // Lo iterable lo dice Symbol.iterator (las librerías lo miran para saber si algo se recorre).
  Map.prototype[Symbol.iterator] = Map.prototype.entries;
  Set.prototype[Symbol.iterator] = Set.prototype.values;
  Array.prototype[Symbol.iterator] = Array.prototype.values;
  String.prototype[Symbol.iterator] = function () { return new Iterador(String(this).split('')); };

  // Débiles solo de nombre: guardan sus claves vivas (falta un GC con efemérides).
  function WeakMap() {
    if (!(this instanceof WeakMap)) throw new TypeError('WeakMap necesita new');
    this['\u0001tabla'] = new Tabla();
  }
  definir(WeakMap.prototype, {
    get: Map.prototype.get,
    set: function (k, v) { tabla(this, 'WeakMap.set').poner(k, v); return this; },
    has: Map.prototype.has,
    'delete': function (k) { return tabla(this, 'WeakMap.delete').quitar(k); }
  });
  function WeakSet() {
    if (!(this instanceof WeakSet)) throw new TypeError('WeakSet necesita new');
    this['\u0001tabla'] = new Tabla();
  }
  definir(WeakSet.prototype, {
    add: function (v) { tabla(this, 'WeakSet.add').poner(v, v); return this; },
    has: Set.prototype.has,
    'delete': function (v) { return tabla(this, 'WeakSet.delete').quitar(v); }
  });

  global.Map = Map;
  global.Set = Set;
  global.WeakMap = WeakMap;
  global.WeakSet = WeakSet;
  ocultar.push(Tabla.prototype, Map.prototype, Set.prototype, WeakMap.prototype, WeakSet.prototype);
  // `size` es un accesor en los navegadores: no se enumera. Aquí es una propiedad, oculta por la
  // forma de los Map y Set recién creados.
  ocultar.push(new Map(), new Set());

  // ---------------- Promise y microtareas ----------------

  var cola = [];
  function encolar(f) { cola.push(f); }
  global.queueMicrotask = function (f) {
    if (typeof f !== 'function') throw new TypeError('queueMicrotask necesita una función');
    encolar(f);
  };
  // El anfitrión la llama tras cada script, evento y temporizador. Devuelve los errores.
  global.__quimera_microtareas = function () {
    var errores = [];
    while (cola.length) {
      var lote = cola;
      cola = [];
      for (var i = 0; i < lote.length; i++) {
        try { lote[i](); } catch (e) { errores.push(String(e)); }
      }
    }
    return errores;
  };

  var PENDIENTE = 0, CUMPLIDA = 1, RECHAZADA = 2;

  function estado(p) {
    var s = p == null ? undefined : p['\u0001promesa'];
    if (!s) throw new TypeError('no es una promesa');
    return s;
  }

  function funcionesDe(p) {
    var hecho = false;
    return [
      function (v) { if (!hecho) { hecho = true; resolver(p, v); } },
      function (e) { if (!hecho) { hecho = true; asentar(p, RECHAZADA, e); } }
    ];
  }

  function resolver(p, v) {
    if (v === p) return asentar(p, RECHAZADA, new TypeError('una promesa no puede resolverse consigo misma'));
    if (v !== null && (typeof v === 'object' || typeof v === 'function')) {
      var then;
      try { then = v.then; } catch (e) { return asentar(p, RECHAZADA, e); }
      if (typeof then === 'function') {
        encolar(function () {
          var fs = funcionesDe(p);
          try { then.call(v, fs[0], fs[1]); } catch (e) { fs[1](e); }
        });
        return;
      }
    }
    asentar(p, CUMPLIDA, v);
  }

  function asentar(p, nuevo, valor) {
    var s = estado(p);
    if (s.estado !== PENDIENTE) return;
    s.estado = nuevo;
    s.valor = valor;
    var rs = s.reacciones;
    s.reacciones = null;
    for (var i = 0; i < rs.length; i++) programar(s, rs[i]);
  }

  function programar(s, r) {
    encolar(function () {
      var f = s.estado === CUMPLIDA ? r.cumplida : r.rechazada;
      if (typeof f !== 'function') {
        if (s.estado === CUMPLIDA) r.resolver(s.valor);
        else r.rechazar(s.valor);
        return;
      }
      var x;
      try { x = f(s.valor); } catch (e) { r.rechazar(e); return; }
      r.resolver(x);
    });
  }

  function Promise(ejecutor) {
    if (!(this instanceof Promise)) throw new TypeError('Promise necesita new');
    if (typeof ejecutor !== 'function') throw new TypeError('Promise necesita una función');
    this['\u0001promesa'] = { estado: PENDIENTE, valor: undefined, reacciones: [] };
    var fs = funcionesDe(this);
    try { ejecutor(fs[0], fs[1]); } catch (e) { fs[1](e); }
  }
  definir(Promise.prototype, {
    then: function (cumplida, rechazada) {
      var s = estado(this);
      var r = { cumplida: cumplida, rechazada: rechazada, resolver: null, rechazar: null };
      var nueva = new Promise(function (res, rej) { r.resolver = res; r.rechazar = rej; });
      if (s.estado === PENDIENTE) s.reacciones.push(r);
      else programar(s, r);
      return nueva;
    },
    'catch': function (rechazada) { return this.then(undefined, rechazada); },
    'finally': function (f) {
      if (typeof f !== 'function') return this.then(f, f);
      return this.then(
        function (v) { return Promise.resolve(f()).then(function () { return v; }); },
        function (e) { return Promise.resolve(f()).then(function () { throw e; }); }
      );
    }
  });
  definir(Promise, {
    resolve: function (v) {
      if (v instanceof Promise) return v;
      return new Promise(function (res) { res(v); });
    },
    reject: function (e) { return new Promise(function (res, rej) { rej(e); }); },
    all: function (lista) {
      return new Promise(function (res, rej) {
        var a = aLista(lista), r = [], faltan = a.length;
        if (!faltan) return res(r);
        a.forEach(function (x, i) {
          Promise.resolve(x).then(function (v) { r[i] = v; if (--faltan === 0) res(r); }, rej);
        });
      });
    },
    allSettled: function (lista) {
      return new Promise(function (res) {
        var a = aLista(lista), r = [], faltan = a.length;
        if (!faltan) return res(r);
        function hecho(i, x) {
          r[i] = x;
          if (--faltan === 0) res(r);
        }
        a.forEach(function (x, i) {
          Promise.resolve(x).then(
            function (v) { hecho(i, { status: 'fulfilled', value: v }); },
            function (e) { hecho(i, { status: 'rejected', reason: e }); }
          );
        });
      });
    },
    race: function (lista) {
      return new Promise(function (res, rej) {
        aLista(lista).forEach(function (x) { Promise.resolve(x).then(res, rej); });
      });
    },
    any: function (lista) {
      return new Promise(function (res, rej) {
        var a = aLista(lista), faltan = a.length;
        if (!faltan) return rej(new Error('todas las promesas fueron rechazadas'));
        a.forEach(function (x) {
          Promise.resolve(x).then(res, function () { if (--faltan === 0) rej(new Error('todas las promesas fueron rechazadas')); });
        });
      });
    }
  });
  global.Promise = Promise;
  ocultar.push(Promise.prototype);

  // ---------------- ayudantes de la sintaxis moderna (parser/azucar.rs y clases.rs) ----------------

  // `o.m(...a)` y `o[k](...a)`: el método con su `this`.
  global.__quimera_metodo = function (o, k, args) {
    var f = o[k];
    if (typeof f !== 'function') throw new TypeError(String(k) + ' no es una función');
    return f.apply(o, args);
  };
  // `new F(...a)`.
  global.__quimera_nuevo = function (F, args) {
    if (typeof F !== 'function') throw new TypeError(F + ' no es un constructor');
    var o = Object.create(F.prototype), r = F.apply(o, args);
    return r !== null && (typeof r === 'object' || typeof r === 'function') ? r : o;
  };
  // `let {a, ...resto} = o`.
  global.__quimera_resto = function (o, quitar) {
    var r = {};
    Object.keys(o).forEach(function (k) { if (quitar.indexOf(k) < 0) r[k] = o[k]; });
    return r;
  };
  // `class C extends P`: el prototipo y los métodos estáticos (copiados al definir la clase).
  global.__quimera_heredar = function (C, P) {
    if (P !== null && typeof P !== 'function') throw new TypeError('extends necesita un constructor o null');
    C.prototype = Object.create(P === null ? null : P.prototype);
    C.prototype.constructor = C;
    if (P) Object.keys(P).forEach(function (k) { if (k !== 'prototype') C[k] = P[k]; });
  };
  // `super(...args)`: el constructor del padre sobre `this`, y luego los campos de la clase.
  // Devuelve el `this` del constructor: si el padre crea su propio objeto (un Error, un
  // elemento del DOM, un arreglo), ese pasa a ser el `this`, con el prototipo de la subclase.
  global.__quimera_super = function (P, este, args, campos) {
    if (P === null) throw new TypeError('super() en una clase que extiende null');
    var r = P.apply(este, args);
    var real = este;
    if (r !== este && r !== null && (typeof r === 'object' || typeof r === 'function')) {
      Object.setPrototypeOf(r, Object.getPrototypeOf(este));
      real = r;
    }
    if (typeof campos === 'function') campos.call(real);
    return real;
  };

  // ---------------- más cadenas, arreglos, Boolean ----------------

  definir(String.prototype, {
    lastIndexOf: function (buscar, desde) {
      var s = String(this), b = String(buscar);
      var i = desde === undefined ? s.length : Math.max(0, Math.min(desde | 0, s.length));
      for (; i >= 0; i--) if (s.substring(i, i + b.length) === b) return i;
      return -1;
    },
    concat: function () {
      var s = String(this);
      for (var i = 0; i < arguments.length; i++) s += String(arguments[i]);
      return s;
    },
    substr: function (inicio, largo) {
      var s = String(this);
      inicio = inicio < 0 ? Math.max(s.length + inicio, 0) : inicio | 0;
      return s.substring(inicio, largo === undefined ? s.length : inicio + Math.max(largo | 0, 0));
    },
    codePointAt: function (i) {
      var s = String(this), a = s.charCodeAt(i | 0);
      if (a !== a) return undefined;
      if (a >= 0xD800 && a < 0xDC00) {
        var b = s.charCodeAt((i | 0) + 1);
        if (b >= 0xDC00 && b < 0xE000) return (a - 0xD800) * 0x400 + (b - 0xDC00) + 0x10000;
      }
      return a;
    },
    normalize: function () { return String(this); },
    localeCompare: function (otra) {
      var a = String(this), b = String(otra);
      return a < b ? -1 : a > b ? 1 : 0;
    },
    toLocaleLowerCase: function () { return String(this).toLowerCase(); },
    toLocaleUpperCase: function () { return String(this).toUpperCase(); }
  });
  String.prototype.trimLeft = String.prototype.trimStart;
  String.prototype.trimRight = String.prototype.trimEnd;
  String.fromCodePoint = function () {
    var s = '';
    for (var i = 0; i < arguments.length; i++) {
      var c = arguments[i];
      if (c > 0xFFFF) { c -= 0x10000; s += String.fromCharCode(0xD800 + (c >> 10), 0xDC00 + (c & 0x3FF)); }
      else s += String.fromCharCode(c);
    }
    return s;
  };

  definir(Array.prototype, {
    reduceRight: function (f, inicial) {
      var i = this.length - 1, acc = inicial;
      if (arguments.length < 2) { acc = this[i]; i--; }
      for (; i >= 0; i--) acc = f(acc, this[i], i, this);
      return acc;
    },
    toSorted: function (f) { return this.slice().sort(f); },
    toReversed: function () { return this.slice().reverse(); },
    'with': function (i, v) { var c = this.slice(); c[i < 0 ? c.length + i : i] = v; return c; },
    toString: function () { return this.join(','); }
  });

  global.Boolean = function Boolean(v) { return !!v; };
  global.Boolean.prototype.toString = function () { return this ? 'true' : 'false'; };
  global.Boolean.prototype.valueOf = function () { return !!this; };
  ocultar.push(global.Boolean.prototype);

  // Un método de arreglo sobre algo que lo parece (lo llaman las nativas de Rust).
  var MUTAN = ['push', 'pop', 'shift', 'unshift', 'reverse', 'sort'];
  global.__quimera_generico = function (nombre, o, args) {
    if (o == null) throw new TypeError('Array.prototype.' + nombre + ' sobre ' + o);
    var n = o.length >>> 0, a = [];
    for (var i = 0; i < n; i++) a.push(o[i]);
    var r = Array.prototype[nombre].apply(a, args);
    if (MUTAN.indexOf(nombre) >= 0) {
      for (var j = 0; j < a.length; j++) o[j] = a[j];
      for (var k = a.length; k < n; k++) delete o[k];
      o.length = a.length;
    }
    return r;
  };

  definir(Math, {
    imul: function (a, b) {
      var ah = (a >>> 16) & 0xffff, al = a & 0xffff, bh = (b >>> 16) & 0xffff, bl = b & 0xffff;
      return ((al * bl) + (((ah * bl + al * bh) << 16) >>> 0)) | 0;
    },
    clz32: function (x) {
      x = x >>> 0;
      if (x === 0) return 32;
      var n = 0;
      while ((x & 0x80000000) === 0) { x = x << 1; n++; }
      return n;
    },
    fround: function (x) { return +x; },
    expm1: function (x) { return Math.exp(x) - 1; },
    log1p: function (x) { return Math.log(1 + x); },
    sinh: function (x) { return (Math.exp(x) - Math.exp(-x)) / 2; },
    cosh: function (x) { return (Math.exp(x) + Math.exp(-x)) / 2; },
    tanh: function (x) { var a = Math.exp(x), b = Math.exp(-x); return (a - b) / (a + b); }
  });

  definir(Object.prototype, {
    propertyIsEnumerable: function (k) {
      return Object.prototype.hasOwnProperty.call(this, k) && Object.keys(this).indexOf(String(k)) >= 0;
    },
    isPrototypeOf: function (o) {
      while (o !== null && (typeof o === 'object' || typeof o === 'function')) {
        o = Object.getPrototypeOf(o);
        if (o === this) return true;
      }
      return false;
    }
  });

  // El resto de `console` (lo que escribe va por `console.log`).
  var cuentas = {}, relojes = {};
  definir(global.console, {
    assert: function (c) {
      if (!c) console.error.apply(console, ['Assertion failed:'].concat(Array.prototype.slice.call(arguments, 1)));
    },
    groupEnd: function () {},
    clear: function () {},
    count: function (k) { k = k === undefined ? 'default' : String(k); cuentas[k] = (cuentas[k] || 0) + 1; console.log(k + ': ' + cuentas[k]); },
    countReset: function (k) { cuentas[k === undefined ? 'default' : String(k)] = 0; },
    time: function (k) { relojes[k === undefined ? 'default' : String(k)] = Date.now(); },
    timeLog: function (k) { k = k === undefined ? 'default' : String(k); console.log(k + ': ' + (Date.now() - relojes[k]) + ' ms'); },
    timeEnd: function (k) { k = k === undefined ? 'default' : String(k); console.log(k + ': ' + (Date.now() - relojes[k]) + ' ms'); delete relojes[k]; }
  });

  // `obj.__proto__` (leer y asignar): el accesor del anexo B, que usan librerías viejas y
  // polyfills (ShadyDOM convierte un fragmento en ShadowRoot con `f.__proto__ = ...`).
  Object.defineProperty(Object.prototype, '__proto__', {
    get: function () { return Object.getPrototypeOf(this); },
    set: function (p) { if (p === null || typeof p === 'object' || typeof p === 'function') Object.setPrototypeOf(this, p); },
    configurable: true
  });

  // `Reflect`: las operaciones de los objetos como funciones (lo que hace un Proxy sin trampa).
  global.Reflect = {
    get: function (o, k) { return o[k]; },
    set: function (o, k, v) { o[k] = v; return true; },
    has: function (o, k) { return k in o; },
    deleteProperty: function (o, k) { return delete o[k]; },
    ownKeys: function (o) { return Object.getOwnPropertyNames(o); },
    getPrototypeOf: Object.getPrototypeOf,
    setPrototypeOf: function (o, p) { Object.setPrototypeOf(o, p); return true; },
    defineProperty: function (o, k, d) { Object.defineProperty(o, k, d); return true; },
    getOwnPropertyDescriptor: Object.getOwnPropertyDescriptor,
    apply: function (f, este, args) { return Function.prototype.apply.call(f, este, args); },
    // Con `nuevo` (el `new.target` de las clases compiladas por Babel/TypeScript): el objeto
    // nace con su prototipo, y si `F` devuelve otro objeto (un elemento), lo toma también.
    construct: function (F, args, nuevo) {
      if (nuevo === undefined || nuevo === F) return __quimera_nuevo(F, args);
      var o = Object.create(nuevo.prototype), r = F.apply(o, args);
      if (r === null || (typeof r !== 'object' && typeof r !== 'function')) return o;
      if (Object.getPrototypeOf(r) !== nuevo.prototype) Object.setPrototypeOf(r, nuevo.prototype);
      return r;
    }
  };
  global.Reflect[Symbol.toStringTag] = 'Reflect';

  // `Object.prototype.toString` (nativo) mira Symbol.toStringTag.
  Map.prototype[Symbol.toStringTag] = 'Map';
  Set.prototype[Symbol.toStringTag] = 'Set';
  WeakMap.prototype[Symbol.toStringTag] = 'WeakMap';
  Promise.prototype[Symbol.toStringTag] = 'Promise';

  // ---------------- Date ----------------
  // Sobre la nativa (que guarda los milisegundos): constructores con texto ISO o con partes, y
  // los métodos. La hora local es UTC por ahora (getTimezoneOffset() da 0).
  var DateNativa = global.Date;
  var DIA = 86400000;
  function diasDesdeCivil(a, m, d) {
    a -= m <= 2 ? 1 : 0;
    var era = Math.floor(a / 400), ae = a - era * 400;
    var dy = Math.floor((153 * (m + (m > 2 ? -3 : 9)) + 2) / 5) + d - 1;
    var de = ae * 365 + Math.floor(ae / 4) - Math.floor(ae / 100) + dy;
    return era * 146097 + de - 719468;
  }
  function civilDesdeDias(z) {
    z += 719468;
    var era = Math.floor(z / 146097), de = z - era * 146097;
    var ae = Math.floor((de - Math.floor(de / 1460) + Math.floor(de / 36524) - Math.floor(de / 146096)) / 365);
    var a = ae + era * 400, dy = de - (365 * ae + Math.floor(ae / 4) - Math.floor(ae / 100));
    var mp = Math.floor((5 * dy + 2) / 153), d = dy - Math.floor((153 * mp + 2) / 5) + 1;
    var m = mp + (mp < 10 ? 3 : -9);
    return [a + (m <= 2 ? 1 : 0), m, d];
  }
  function desdePartes(a, m, d, h, mi, s, ms) {
    a = +a; m = +m || 0;
    a += Math.floor(m / 12); m = ((m % 12) + 12) % 12;
    return diasDesdeCivil(a, m + 1, 1) * DIA + ((d === undefined ? 1 : +d) - 1) * DIA +
      (+h || 0) * 3600000 + (+mi || 0) * 60000 + (+s || 0) * 1000 + (+ms || 0);
  }
  function parsear(t) {
    var r = /^(\d{4})(?:-(\d\d)(?:-(\d\d))?)?(?:[T ](\d\d):(\d\d)(?::(\d\d)(?:\.(\d{1,3}))?)?)?(Z|[+-]\d\d:?\d\d)?$/.exec(String(t).trim());
    if (!r) return NaN;
    var ms = desdePartes(r[1], (r[2] || 1) - 1, r[3] || 1, r[4], r[5], r[6], r[7] ? (r[7] + '00').slice(0, 3) : 0);
    if (r[8] && r[8] !== 'Z') {
      var signo = r[8][0] === '-' ? -1 : 1, hh = +r[8].slice(1, 3), mm = +r[8].slice(-2);
      ms -= signo * (hh * 60 + mm) * 60000;
    }
    return ms;
  }
  function FechaJs(a, m, d, h, mi, s, ms) {
    if (!(this instanceof FechaJs)) return new DateNativa(DateNativa.now()).toString();
    var n = arguments.length, valor;
    if (n === 0) valor = DateNativa.now();
    else if (n === 1) valor = typeof a === 'string' ? parsear(a) : a instanceof DateNativa ? a.getTime() : +a;
    else valor = desdePartes(a, m, d, h, mi, s, ms);
    return new DateNativa(valor);
  }
  FechaJs.prototype = DateNativa.prototype;
  FechaJs.prototype.constructor = FechaJs;
  FechaJs.now = DateNativa.now;
  FechaJs.parse = parsear;
  FechaJs.UTC = function (a, m, d, h, mi, s, ms) { return desdePartes(a, m, d, h, mi, s, ms); };
  global.Date = FechaJs;
  function partes(f) { var t = f.getTime(); var dias = Math.floor(t / DIA); var c = civilDesdeDias(dias); var resto = t - dias * DIA; return { a: c[0], m: c[1] - 1, d: c[2], h: Math.floor(resto / 3600000), mi: Math.floor(resto / 60000) % 60, s: Math.floor(resto / 1000) % 60, ms: resto % 1000, dia: ((dias % 7) + 11) % 7 }; }
  function dos(n) { return (n < 10 ? '0' : '') + n; }
  var DIAS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  var MESES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  var metodos = {
    getFullYear: function () { return partes(this).a; },
    getMonth: function () { return partes(this).m; },
    getDate: function () { return partes(this).d; },
    getDay: function () { return partes(this).dia; },
    getHours: function () { return partes(this).h; },
    getMinutes: function () { return partes(this).mi; },
    getSeconds: function () { return partes(this).s; },
    getMilliseconds: function () { return partes(this).ms; },
    getTimezoneOffset: function () { return 0; },
    setTime: function (t) { this['[[Valor]]'] = +t; return +t; },
    toISOString: function () {
      var p = partes(this);
      if (this.getTime() !== this.getTime()) throw new RangeError('Invalid time value');
      return p.a + '-' + dos(p.m + 1) + '-' + dos(p.d) + 'T' + dos(p.h) + ':' + dos(p.mi) + ':' + dos(p.s) + '.' + ('00' + p.ms).slice(-3) + 'Z';
    },
    toJSON: function () { return this.getTime() !== this.getTime() ? null : this.toISOString(); },
    toUTCString: function () {
      var p = partes(this);
      return DIAS[p.dia] + ', ' + dos(p.d) + ' ' + MESES[p.m] + ' ' + p.a + ' ' + dos(p.h) + ':' + dos(p.mi) + ':' + dos(p.s) + ' GMT';
    },
    toGMTString: function () { return this.toUTCString(); }, // el nombre antiguo, que aún usan
    toDateString: function () { var p = partes(this); return DIAS[p.dia] + ' ' + MESES[p.m] + ' ' + dos(p.d) + ' ' + p.a; },
    toTimeString: function () { var p = partes(this); return dos(p.h) + ':' + dos(p.mi) + ':' + dos(p.s) + ' GMT+0000 (Coordinated Universal Time)'; },
    toString: function () {
      if (this.getTime() !== this.getTime()) return 'Invalid Date';
      return this.toDateString() + ' ' + this.toTimeString();
    },
    toLocaleDateString: function () { var p = partes(this); return (p.m + 1) + '/' + p.d + '/' + p.a; },
    toLocaleTimeString: function () { var p = partes(this); return dos(p.h) + ':' + dos(p.mi) + ':' + dos(p.s); },
    toLocaleString: function () { return this.toLocaleDateString() + ', ' + this.toLocaleTimeString(); }
  };
  ['FullYear', 'Month', 'Date', 'Day', 'Hours', 'Minutes', 'Seconds', 'Milliseconds'].forEach(function (n) {
    metodos['getUTC' + n] = metodos['get' + n];
  });
  function setter(cual) {
    return function (v) {
      var p = partes(this), args = arguments;
      var orden = ['a', 'm', 'd', 'h', 'mi', 's', 'ms'], i = orden.indexOf(cual);
      for (var k = 0; k < args.length && i + k < orden.length; k++) p[orden[i + k]] = +args[k];
      return this.setTime(desdePartes(p.a, p.m, p.d, p.h, p.mi, p.s, p.ms));
    };
  }
  [['FullYear', 'a'], ['Month', 'm'], ['Date', 'd'], ['Hours', 'h'], ['Minutes', 'mi'], ['Seconds', 's'], ['Milliseconds', 'ms']].forEach(function (par) {
    metodos['set' + par[0]] = setter(par[1]);
    metodos['setUTC' + par[0]] = metodos['set' + par[0]];
  });
  definir(DateNativa.prototype, metodos);
  ocultar.push(DateNativa.prototype);

  // ---------------- generadores y async ----------------

  // Los generadores (su prototipo lo crea la máquina) son iterables: su iterador son ellos.
  var protoGenerador = global.__quimera_generador;
  protoGenerador[Symbol.iterator] = function () { return this; };
  ocultar.push(protoGenerador);

  // El iterador de `yield* x`.
  global.__quimera_iterador = function (x) {
    if (x != null && typeof x[Symbol.iterator] === 'function') return x[Symbol.iterator]();
    if (x != null && typeof x.next === 'function') return x;
    return new Iterador(aLista(x));
  };

  // Una función `async` es un generador que va entregando lo que espera: cada `yield` (un
  // `await`) se resuelve como promesa y lo obtenido vuelve al generador con next (o throw).
  global.__quimera_async = function (genFn, este, args) {
    return new Promise(function (resolver, rechazar) {
      var g = genFn.apply(este, args);
      function paso(metodo, v) {
        var r;
        try { r = g[metodo](v); } catch (e) { rechazar(e); return; }
        if (r.done) { resolver(r.value); return; }
        Promise.resolve(r.value).then(
          function (x) { paso('next', x); },
          function (e) { paso('throw', e); }
        );
      }
      paso('next', undefined);
    });
  };

  // Un `async function*`: sus `await` llegan como Espera (y se esperan dentro); sus `yield`,
  // como valores que cada next() entrega en una promesa. Las peticiones van en cola, en orden.
  function Espera(v) { this.v = v; }
  global.__quimera_espera = function (v) { return new Espera(v); };
  global.__quimera_async_gen = function (genFn, este, args) {
    var g = genFn.apply(este, args), cola = [], corriendo = false;
    function siguiente() {
      if (corriendo || !cola.length) return;
      corriendo = true;
      var t = cola.shift();
      paso(t, t.metodo, t.valor);
    }
    function acabar(t, ok, v) {
      corriendo = false;
      if (ok) t.res(v); else t.rej(v);
      siguiente();
    }
    function paso(t, metodo, v) {
      var r;
      try { r = g[metodo](v); } catch (e) { acabar(t, false, e); return; }
      if (!r.done && r.value instanceof Espera) {
        Promise.resolve(r.value.v).then(function (x) { paso(t, 'next', x); }, function (e) { paso(t, 'throw', e); });
        return;
      }
      Promise.resolve(r.value).then(
        function (x) { acabar(t, true, { value: x, done: r.done }); },
        function (e) { acabar(t, false, e); }
      );
    }
    var it = {};
    ['next', 'return', 'throw'].forEach(function (m) {
      it[m] = function (v) {
        return new Promise(function (res, rej) { cola.push({ metodo: m, valor: v, res: res, rej: rej }); siguiente(); });
      };
    });
    it[Symbol.asyncIterator] = function () { return this; };
    return it;
  };
  // El iterador de `for await`: uno asíncrono, o uno normal cuyos valores se esperan.
  global.__quimera_iterador_async = function (x) {
    if (x != null && typeof x[Symbol.asyncIterator] === 'function') return x[Symbol.asyncIterator]();
    var it = global.__quimera_iterador(x);
    return {
      next: function (v) {
        var r = it.next(v);
        return Promise.resolve(r.value).then(function (valor) { return { value: valor, done: r.done }; });
      }
    };
  };

  global.__quimera_ocultar = ocultar;
})(this);
