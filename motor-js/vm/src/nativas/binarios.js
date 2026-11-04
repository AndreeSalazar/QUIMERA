// Datos binarios: ArrayBuffer, los arreglos tipados (Uint8Array...), DataView, TextEncoder y
// TextDecoder. Se ejecuta tras preludio.js.
//
// Los bytes son de verdad (`nativas/bytes.rs` y `quimera-objetos/src/bytes.rs`):
// - Un ArrayBuffer es un objeto nativo con sus bytes en el montón del GC.
// - Un arreglo tipado es una **vista** nativa sobre un ArrayBuffer: `a[i]`, `a[i] = x` y
//   `a.length` van directos a los bytes, y `subarray` crea otra vista sin copiar nada.
// - DataView lee y escribe números en cualquier posición, en los dos órdenes de bytes.
(function (global) {
  var ocultar = global.__quimera_ocultar;
  var bytes = __quimera_bytes, blargo = __quimera_blargo, vista = __quimera_vista, vinfo = __quimera_vinfo;
  var bcopiar = __quimera_bcopiar;
  var TAM = [1, 1, 1, 2, 2, 4, 4, 4, 8];
  var NOMBRES = ['Int8Array', 'Uint8Array', 'Uint8ClampedArray', 'Int16Array', 'Uint16Array', 'Int32Array',
    'Uint32Array', 'Float32Array', 'Float64Array'];
  var CONSTRUCTORES = [];

  function entero(x) { x = Math.floor(+x || 0); return x < 0 ? 0 : x; }
  // Un índice relativo (los negativos cuentan desde el final), entre 0 y n.
  function relativo(x, n, porDefecto) {
    if (x === undefined) return porDefecto;
    x = Math.trunc(+x || 0);
    return x < 0 ? Math.max(n + x, 0) : Math.min(x, n);
  }

  // ---------------- ArrayBuffer ----------------
  function ArrayBuffer(n) {
    if (!(this instanceof ArrayBuffer)) throw new TypeError("ArrayBuffer necesita 'new'");
    return bytes(entero(n), ArrayBuffer.prototype);
  }
  Object.defineProperty(ArrayBuffer.prototype, 'byteLength', { get: function () {
    var n = blargo(this);
    if (n === undefined) throw new TypeError('byteLength necesita un ArrayBuffer');
    return n;
  } });
  ArrayBuffer.prototype.slice = function (a, b) {
    var n = blargo(this), i = relativo(a, n, 0), k = Math.max(relativo(b, n, n) - i, 0);
    var r = bytes(k, ArrayBuffer.prototype);
    bcopiar(this, i, r, 0, k);
    return r;
  };
  ArrayBuffer.prototype[Symbol.toStringTag] = 'ArrayBuffer';
  ArrayBuffer.isView = function (v) { return v != null && (vinfo(v, 3) !== undefined || v instanceof DataView); };
  ocultar.push(ArrayBuffer.prototype);

  // ---------------- DataView ----------------
  var B = '\u0001b', D = '\u0001d', L = '\u0001l';
  function DataView(buffer, desplazamiento, largo) {
    if (!(this instanceof DataView)) throw new TypeError("DataView necesita 'new'");
    var total = blargo(buffer);
    if (total === undefined) throw new TypeError('DataView necesita un ArrayBuffer');
    var d = entero(desplazamiento), l = largo === undefined ? total - d : entero(largo);
    if (d > total || d + l > total) throw new RangeError('la DataView se sale de su ArrayBuffer');
    this[B] = buffer; this[D] = d; this[L] = l;
  }
  Object.defineProperty(DataView.prototype, 'buffer', { get: function () { return this[B]; } });
  Object.defineProperty(DataView.prototype, 'byteOffset', { get: function () { return this[D]; } });
  Object.defineProperty(DataView.prototype, 'byteLength', { get: function () { return this[L]; } });
  [['Int8', 0], ['Uint8', 1], ['Int16', 3], ['Uint16', 4], ['Int32', 5], ['Uint32', 6], ['Float32', 7],
    ['Float64', 8]].forEach(function (t) {
    var e = t[1], n = TAM[e];
    DataView.prototype['get' + t[0]] = function (i, le) {
      i = entero(i);
      if (i + n > this[L]) throw new RangeError('lectura fuera de la DataView');
      return __quimera_dv(this[B], this[D] + i, e, !!le);
    };
    DataView.prototype['set' + t[0]] = function (i, x, le) {
      i = entero(i);
      if (i + n > this[L]) throw new RangeError('escritura fuera de la DataView');
      __quimera_dvset(this[B], this[D] + i, e, !!le, x);
    };
  });
  DataView.prototype[Symbol.toStringTag] = 'DataView';
  ocultar.push(DataView.prototype);

  // ---------------- Arreglos tipados ----------------
  // La base común (`%TypedArray%.prototype`).
  var Base = {};
  function especieDe(a) {
    var e = vinfo(a, 3);
    if (e === undefined) throw new TypeError('no es un arreglo tipado');
    return e;
  }
  function nuevoComo(a, n) { return new CONSTRUCTORES[especieDe(a)](n); }
  Object.defineProperty(Base, 'buffer', { get: function () { return vinfo(this, 0); } });
  Object.defineProperty(Base, 'byteOffset', { get: function () { return vinfo(this, 1); } });
  Object.defineProperty(Base, 'byteLength', { get: function () { return vinfo(this, 2) * TAM[especieDe(this)]; } });
  Base.set = function (origen, desde) {
    if (vinfo(origen, 3) === undefined && !Array.isArray(origen)) origen = Array.from(origen);
    __quimera_vset(this, origen, entero(desde));
  };
  Base.subarray = function (a, b) {
    var n = this.length, e = especieDe(this), i = relativo(a, n, 0), k = Math.max(relativo(b, n, n) - i, 0);
    return vista(vinfo(this, 0), vinfo(this, 1) + i * TAM[e], k, e, Object.getPrototypeOf(this));
  };
  Base.slice = function (a, b) {
    var n = this.length, e = especieDe(this), i = relativo(a, n, 0), k = Math.max(relativo(b, n, n) - i, 0);
    var r = nuevoComo(this, k);
    bcopiar(vinfo(this, 0), vinfo(this, 1) + i * TAM[e], vinfo(r, 0), 0, k * TAM[e]);
    return r;
  };
  Base.fill = function (x, a, b) {
    var n = this.length;
    return __quimera_vrellenar(this, x, relativo(a, n, 0), relativo(b, n, n));
  };
  Base.copyWithin = function (t, a, b) {
    var n = this.length, s = TAM[especieDe(this)], d = relativo(t, n, 0), i = relativo(a, n, 0);
    var k = Math.min(relativo(b, n, n) - i, n - d), buf = vinfo(this, 0), o = vinfo(this, 1);
    if (k > 0) bcopiar(buf, o + i * s, buf, o + d * s, k * s);
    return this;
  };
  // Los que solo recorren: los de Array, que la máquina aplica a una vista como a un arreglo.
  ['join', 'indexOf', 'lastIndexOf', 'includes', 'forEach', 'every', 'some', 'find', 'findIndex',
    'findLast', 'findLastIndex', 'reduce', 'reduceRight', 'at', 'keys', 'values', 'entries'].forEach(function (k) {
    if (Array.prototype[k]) Base[k] = Array.prototype[k];
  });
  Base.map = function (f, t) {
    var n = this.length, r = nuevoComo(this, n);
    for (var i = 0; i < n; i++) r[i] = f.call(t, this[i], i, this);
    return r;
  };
  Base.filter = function (f, t) {
    var v = [];
    for (var i = 0; i < this.length; i++) if (f.call(t, this[i], i, this)) v.push(this[i]);
    var r = nuevoComo(this, v.length);
    r.set(v);
    return r;
  };
  Base.reverse = function () {
    for (var i = 0, j = this.length - 1; i < j; i++, j--) { var x = this[i]; this[i] = this[j]; this[j] = x; }
    return this;
  };
  Base.sort = function (f) {
    var v = Array.prototype.slice.call(this);
    v.sort(f || function (a, b) { return a - b; });
    this.set(v);
    return this;
  };
  Base.toString = function () { return Array.prototype.join.call(this, ','); };
  Base.toLocaleString = Base.toString;
  Base[Symbol.iterator] = Base.values;
  ocultar.push(Base);

  NOMBRES.forEach(function (nombre, e) {
    var n = TAM[e];
    function T(a, desplazamiento, largo) {
      if (!(this instanceof T)) throw new TypeError(nombre + " necesita 'new'");
      var total = blargo(a);
      if (total !== undefined) {
        var d = entero(desplazamiento);
        if (d % n) throw new RangeError('el desplazamiento de un ' + nombre + ' debe ser múltiplo de ' + n);
        if (largo === undefined && (total - d) % n) throw new RangeError('el tamaño del ArrayBuffer debe ser múltiplo de ' + n);
        return vista(a, d, largo === undefined ? (total - d) / n : entero(largo), e, T.prototype);
      }
      if (a !== null && typeof a === 'object') {
        var origen = vinfo(a, 3) !== undefined || Array.isArray(a) ? a : Array.from(a);
        var r = vista(bytes(origen.length * n, ArrayBuffer.prototype), 0, origen.length, e, T.prototype);
        __quimera_vset(r, origen, 0);
        return r;
      }
      var k = entero(a);
      return vista(bytes(k * n, ArrayBuffer.prototype), 0, k, e, T.prototype);
    }
    T.prototype = Object.create(Base);
    T.prototype.constructor = T;
    T.prototype[Symbol.toStringTag] = nombre;
    T.BYTES_PER_ELEMENT = T.prototype.BYTES_PER_ELEMENT = n;
    T.from = function (x, f, t) { return new T(f ? Array.from(x, f, t) : x); };
    T.of = function () { return new T(Array.prototype.slice.call(arguments)); };
    ocultar.push(T.prototype);
    CONSTRUCTORES[e] = T;
    global[nombre] = T;
  });

  // `Math.fround`: lo que queda al guardarlo en un Float32Array.
  var f32 = new global.Float32Array(1);
  Math.fround = function (x) { f32[0] = x; return f32[0]; };

  // ---------------- TextEncoder y TextDecoder (UTF-8) ----------------
  function TextEncoder() {}
  TextEncoder.prototype.encoding = 'utf-8';
  TextEncoder.prototype.encode = function (s) {
    return new global.Uint8Array(__quimera_utf8(s === undefined ? '' : String(s), ArrayBuffer.prototype));
  };
  TextEncoder.prototype.encodeInto = function (s, destino) {
    var u = this.encode(s), n = Math.min(u.length, destino.length);
    destino.set(u.subarray(0, n));
    return { read: String(s).length, written: n };
  };
  function TextDecoder(codificacion, op) {
    var c = String(codificacion === undefined ? 'utf-8' : codificacion).toLowerCase();
    if (c !== 'utf-8' && c !== 'utf8' && c !== 'unicode-1-1-utf-8') throw new RangeError('TextDecoder: solo utf-8 en QUIMERA');
    this.encoding = 'utf-8';
    this.fatal = !!(op && op.fatal);
    this.ignoreBOM = !!(op && op.ignoreBOM);
  }
  // Lo que no es UTF-8 válido sale como U+FFFD, como en los navegadores.
  TextDecoder.prototype.decode = function (b) {
    if (b === undefined || b === null) return '';
    var buf = vinfo(b, 0), desde, n;
    if (buf !== undefined) { desde = vinfo(b, 1); n = vinfo(b, 2) * TAM[vinfo(b, 3)]; }
    else if (blargo(b) !== undefined) { buf = b; desde = 0; n = blargo(b); }
    else if (b instanceof DataView) { buf = b.buffer; desde = b.byteOffset; n = b.byteLength; }
    else throw new TypeError('decode necesita un ArrayBuffer o una vista');
    var s = __quimera_texto_utf8(buf, desde, n);
    return !this.ignoreBOM && s.charCodeAt(0) === 0xfeff ? s.slice(1) : s;
  };
  ocultar.push(TextEncoder.prototype, TextDecoder.prototype);

  // ---------------- crypto ----------------
  // El azar sale del sistema (lo da el anfitrión con `Motor::con_entropia`); sin él, lanza.
  var crypto = {
    getRandomValues: function (a) {
      var e = vinfo(a, 3);
      if (e === undefined || e >= 7) throw new TypeError('getRandomValues necesita un arreglo tipado de enteros');
      var n = vinfo(a, 2) * TAM[e];
      if (n > 65536) throw new RangeError('getRandomValues: más de 65536 bytes');
      vista(vinfo(a, 0), vinfo(a, 1), n, 1, global.Uint8Array.prototype).set(__quimera_aleatorios(n));
      return a;
    },
    randomUUID: function () {
      var b = __quimera_aleatorios(16), s = '';
      b[6] = (b[6] & 15) | 64;  // versión 4
      b[8] = (b[8] & 63) | 128; // variante RFC 4122
      for (var i = 0; i < 16; i++) {
        s += (b[i] < 16 ? '0' : '') + b[i].toString(16);
        if (i === 3 || i === 5 || i === 7 || i === 9) s += '-';
      }
      return s;
    }
  };
  crypto[Symbol.toStringTag] = 'Crypto';
  global.crypto = crypto;

  global.ArrayBuffer = ArrayBuffer;
  global.DataView = DataView;
  global.TextEncoder = TextEncoder;
  global.TextDecoder = TextDecoder;

  // ---------------- BigInt (mínimo) ----------------
  // QUIMERA aún no tiene el tipo `bigint`. Esto deja crear valores (Zod, en GitHub, crea
  // `BigInt("-9223372036854775808")` al cargar): guardan su texto exacto, que da `toString()`, y
  // `valueOf()` da un Number aproximado, así que operar con ellos es aproximado. `typeof` da
  // "object", no "bigint".
  function BigInt(v) {
    if (this instanceof BigInt) throw new TypeError('BigInt no es un constructor');
    var t;
    if (typeof v === 'number') {
      if (!isFinite(v) || Math.floor(v) !== v) throw new RangeError(v + ' no se puede convertir a BigInt porque no es un entero');
      t = Math.abs(v) < 1e21 ? v.toFixed(0) : String(v);
    } else if (typeof v === 'boolean') {
      t = v ? '1' : '0';
    } else if (v instanceof BigInt) {
      return v;
    } else {
      t = String(v).trim();
      if (/^0x[0-9a-f]+$/i.test(t)) t = String(parseInt(t, 16));
      if (t === '') t = '0';
      if (!/^[+-]?\d+$/.test(t)) throw new SyntaxError('No se puede convertir ' + t + ' a BigInt');
    }
    var negativo = t[0] === '-';
    t = t.replace(/^[+-]/, '').replace(/^0+(?=\d)/, '');
    var b = Object.create(BigInt.prototype);
    b['\u0001t'] = (negativo && t !== '0' ? '-' : '') + t;
    return b;
  }
  BigInt.prototype.toString = function (base) {
    return base === undefined || base === 10 ? this['\u0001t'] : Number(this['\u0001t']).toString(base);
  };
  BigInt.prototype.toLocaleString = function () { return Number(this['\u0001t']).toLocaleString(); };
  BigInt.prototype.valueOf = function () { return Number(this['\u0001t']); };
  BigInt.prototype.toJSON = undefined;
  BigInt.prototype[Symbol.toStringTag] = 'BigInt';
  BigInt.asIntN = function (bits, x) { return x; };
  BigInt.asUintN = function (bits, x) { return x; };
  __quimera_no_enumerar(BigInt.prototype);
  global.BigInt = BigInt;
})(this);
