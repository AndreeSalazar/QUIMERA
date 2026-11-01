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
    getOwnPropertyNames: function (o) { return Object.keys(o); },
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
    if (Array.isArray(x)) return x.slice();
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
  // Si el padre devuelve otro objeto (Error, que se crea solo), se copian sus propiedades.
  global.__quimera_super = function (P, este, args, campos) {
    if (P === null) throw new TypeError('super() en una clase que extiende null');
    var r = P.apply(este, args);
    if (r !== este && r !== null && typeof r === 'object') {
      Object.keys(r).forEach(function (k) { este[k] = r[k]; });
    }
    if (typeof campos === 'function') campos.call(este);
  };

  global.__quimera_ocultar = ocultar;
})(this);
