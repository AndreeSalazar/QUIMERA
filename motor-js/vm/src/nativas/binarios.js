// Datos binarios: ArrayBuffer, los arreglos tipados (Uint8Array...), DataView, TextEncoder y
// TextDecoder. Se ejecuta tras preludio.js.
//
// Una aproximación en JavaScript, sin memoria compartida de verdad:
// - Un ArrayBuffer guarda sus bytes en un arreglo normal (oculto).
// - Un arreglo tipado ES un arreglo de la VM con otro prototipo: lee y escribe tan rápido como
//   un arreglo, pero escribir `u8[0] = 300` no recorta el valor (los métodos `set`, `fill` y el
//   constructor sí). `Array.isArray` da false, como en los navegadores.
// - Un Uint8Array sobre un ArrayBuffer entero es el mismo arreglo que sus bytes, así que
//   DataView y Uint8Array sobre el mismo buffer ven los mismos datos. Las demás vistas (con
//   desplazamiento, o de 2, 4 u 8 bytes) son copias.
(function (global) {
  var ocultar = global.__quimera_ocultar;
  var BYTES = '\u0001bytes', BUFFER = '\u0001buffer', DATOS = '\u0001vista';

  function ceros(n) { var r = []; for (var i = 0; i < n; i++) r.push(0); return r; }
  function entero(x) { x = Math.floor(+x || 0); return x < 0 ? 0 : x; }

  // ---------------- ArrayBuffer ----------------
  function ArrayBuffer(n) {
    if (!(this instanceof ArrayBuffer)) throw new TypeError("ArrayBuffer necesita 'new'");
    this[BYTES] = ceros(entero(n));
  }
  Object.defineProperty(ArrayBuffer.prototype, 'byteLength', { get: function () { return this[BYTES].length; } });
  ArrayBuffer.prototype.slice = function (a, b) {
    var r = new ArrayBuffer(0);
    r[BYTES] = Array.prototype.slice.call(this[BYTES], a, b);
    return r;
  };
  ArrayBuffer.prototype[Symbol.toStringTag] = 'ArrayBuffer';
  ArrayBuffer.isView = function (v) { return v != null && (v instanceof DataView || !!v['\u0001tipado']); };
  ocultar.push(ArrayBuffer.prototype);

  // ---------------- Números en coma flotante (IEEE 754) ----------------
  // Un número → sus bits como enteros sin signo: [palabra] (32) o [alta, baja] (64).
  function codificar(x, eb, mb) {
    var max = Math.pow(2, eb) - 1, sesgo = (max - 1) / 2, s = 0, e, m;
    if (x !== x) return [0, max, Math.pow(2, mb - 1)];
    if (x < 0 || (x === 0 && 1 / x < 0)) { s = 1; x = -x; }
    if (x === Infinity) return [s, max, 0];
    if (x === 0) return [s, 0, 0];
    e = Math.floor(Math.log(x) / Math.LN2);
    if (Math.pow(2, e) > x) e--;
    if (Math.pow(2, e + 1) <= x) e++;
    if (e + sesgo >= 1) {
      m = Math.round((x / Math.pow(2, e) - 1) * Math.pow(2, mb));
      if (m >= Math.pow(2, mb)) { m = 0; e++; }
      if (e + sesgo >= max) return [s, max, 0];
      return [s, e + sesgo, m];
    }
    m = Math.round(x / Math.pow(2, 1 - sesgo - mb));
    if (m >= Math.pow(2, mb)) return [s, 1, 0];
    return [s, 0, m];
  }
  function decodificar(s, e, m, eb, mb) {
    var max = Math.pow(2, eb) - 1, sesgo = (max - 1) / 2, signo = s ? -1 : 1;
    if (e === max) return m ? NaN : signo * Infinity;
    if (e === 0) return signo * m * Math.pow(2, 1 - sesgo - mb);
    return signo * (1 + m / Math.pow(2, mb)) * Math.pow(2, e - sesgo);
  }
  function aBits32(x) { var p = codificar(x, 8, 23); return p[0] * 2147483648 + p[1] * 8388608 + p[2]; }
  function deBits32(u) { return decodificar(u >>> 31, (u >>> 23) & 255, u & 0x7fffff, 8, 23); }
  function aBits64(x) {
    var p = codificar(x, 11, 52), alta = Math.floor(p[2] / 4294967296);
    return [p[0] * 2147483648 + p[1] * 1048576 + alta, p[2] - alta * 4294967296];
  }
  function deBits64(alta, baja) {
    return decodificar(alta >>> 31, (alta >>> 20) & 0x7ff, (alta & 0xfffff) * 4294967296 + baja, 11, 52);
  }
  Math.fround = function (x) { return deBits32(aBits32(+x)); };

  // ---------------- DataView ----------------
  function DataView(buffer, desplazamiento, largo) {
    if (!(this instanceof DataView)) throw new TypeError("DataView necesita 'new'");
    if (!(buffer instanceof ArrayBuffer)) throw new TypeError('DataView necesita un ArrayBuffer');
    var d = entero(desplazamiento), total = buffer[BYTES].length;
    if (d > total) throw new RangeError('desplazamiento fuera del ArrayBuffer');
    this[DATOS] = { buffer: buffer, desde: d, largo: largo === undefined ? total - d : entero(largo) };
  }
  ['buffer', 'byteOffset', 'byteLength'].forEach(function (nombre, i) {
    Object.defineProperty(DataView.prototype, nombre, {
      get: function () { var v = this[DATOS]; return i === 0 ? v.buffer : i === 1 ? v.desde : v.largo; }
    });
  });
  // Un entero sin signo de `n` bytes en la posición `i`.
  function leerSinSigno(vista, i, n, le) {
    var v = vista[DATOS], b = v.buffer[BYTES], o = v.desde + entero(i), x = 0;
    if (entero(i) + n > v.largo) throw new RangeError('lectura fuera del DataView');
    for (var k = 0; k < n; k++) x += b[o + (le ? k : n - 1 - k)] * Math.pow(256, k);
    return x;
  }
  function escribirSinSigno(vista, i, n, x, le) {
    var v = vista[DATOS], b = v.buffer[BYTES], o = v.desde + entero(i);
    if (entero(i) + n > v.largo) throw new RangeError('escritura fuera del DataView');
    for (var k = 0; k < n; k++) {
      b[o + (le ? k : n - 1 - k)] = x % 256;
      x = Math.floor(x / 256);
    }
  }
  function modulo(x, n) { var m = Math.pow(2, 8 * n); x = Math.trunc(+x || 0) % m; return x < 0 ? x + m : x; }
  function conSigno(x, n) { var m = Math.pow(2, 8 * n); return x >= m / 2 ? x - m : x; }
  [['Uint8', 1, false], ['Int8', 1, true], ['Uint16', 2, false], ['Int16', 2, true],
    ['Uint32', 4, false], ['Int32', 4, true]].forEach(function (t) {
    var n = t[1], signo = t[2];
    DataView.prototype['get' + t[0]] = function (i, le) {
      var x = leerSinSigno(this, i, n, le);
      return signo ? conSigno(x, n) : x;
    };
    DataView.prototype['set' + t[0]] = function (i, x, le) { escribirSinSigno(this, i, n, modulo(x, n), le); };
  });
  DataView.prototype.getFloat32 = function (i, le) { return deBits32(leerSinSigno(this, i, 4, le)); };
  DataView.prototype.setFloat32 = function (i, x, le) { escribirSinSigno(this, i, 4, aBits32(+x), le); };
  DataView.prototype.getFloat64 = function (i, le) {
    var a = leerSinSigno(this, le ? i + 4 : i, 4, le), b = leerSinSigno(this, le ? i : i + 4, 4, le);
    return deBits64(a, b);
  };
  DataView.prototype.setFloat64 = function (i, x, le) {
    var p = aBits64(+x);
    escribirSinSigno(this, le ? i + 4 : i, 4, p[0], le);
    escribirSinSigno(this, le ? i : i + 4, 4, p[1], le);
  };
  DataView.prototype[Symbol.toStringTag] = 'DataView';
  ocultar.push(DataView.prototype);

  // ---------------- Arreglos tipados ----------------
  // La base común (`%TypedArray%`): los métodos que devuelven otro arreglo tipado.
  var Base = Object.create(Array.prototype);
  Base['\u0001tipado'] = true;
  function tipoDe(a) { return Object.getPrototypeOf(a).constructor; }
  Base.set = function (origen, desde) {
    var c = tipoDe(this).convertir, d = entero(desde);
    if (d + origen.length > this.length) throw new RangeError('set fuera del arreglo');
    for (var i = 0; i < origen.length; i++) this[d + i] = c(origen[i]);
  };
  Base.fill = function (x, a, b) {
    x = tipoDe(this).convertir(x);
    return Array.prototype.fill.call(this, x, a, b);
  };
  Base.subarray = Base.slice = function (a, b) { return new (tipoDe(this))(Array.prototype.slice.call(this, a, b)); };
  Base.map = function (f, t) { return new (tipoDe(this))(Array.prototype.map.call(this, f, t)); };
  Base.filter = function (f, t) { return new (tipoDe(this))(Array.prototype.filter.call(this, f, t)); };
  Base.toString = function () { return Array.prototype.join.call(this, ','); };
  Object.defineProperty(Base, 'byteLength', { get: function () { return this.length * tipoDe(this).BYTES_PER_ELEMENT; } });
  Object.defineProperty(Base, 'byteOffset', { get: function () { return 0; } });
  Object.defineProperty(Base, 'buffer', {
    get: function () {
      if (this[BUFFER]) return this[BUFFER];
      var T = tipoDe(this), n = T.BYTES_PER_ELEMENT, b = new ArrayBuffer(0);
      if (n === 1) b[BYTES] = this; // un arreglo de bytes es sus propios datos
      else {
        b[BYTES] = ceros(this.length * n);
        var v = new DataView(b);
        for (var i = 0; i < this.length; i++) v[T.escribir](i * n, this[i], true);
      }
      this[BUFFER] = b;
      return b;
    }
  });
  ocultar.push(Base);

  function tipado(nombre, n, convertir, lector) {
    function T(a, desplazamiento, largo) {
      if (!(this instanceof T)) throw new TypeError(nombre + " necesita 'new'");
      var r, i;
      if (a instanceof ArrayBuffer) {
        var bytes = a[BYTES], d = entero(desplazamiento);
        var cuantos = largo === undefined ? Math.floor((bytes.length - d) / n) : entero(largo);
        if (n === 1 && d === 0 && cuantos === bytes.length) r = bytes;
        else {
          r = [];
          var vista = new DataView(a);
          for (i = 0; i < cuantos; i++) r.push(vista[lector](d + i * n, true));
        }
        r[BUFFER] = a;
      } else if (a !== null && typeof a === 'object') {
        r = Array.from(a, function (x) { return convertir(x); });
      } else {
        r = ceros(entero(a));
      }
      Object.setPrototypeOf(r, T.prototype);
      return r;
    }
    T.prototype = Object.create(Base);
    T.prototype.constructor = T;
    T.prototype[Symbol.toStringTag] = nombre;
    T.BYTES_PER_ELEMENT = T.prototype.BYTES_PER_ELEMENT = n;
    T.convertir = convertir;
    T.escribir = 's' + lector.slice(1);
    T.from = function (x, f) { return new T(f ? Array.from(x, f) : x); };
    T.of = function () { return new T(Array.prototype.slice.call(arguments)); };
    ocultar.push(T.prototype);
    global[nombre] = T;
  }
  function recortar(n, signo) {
    return function (x) { x = modulo(x, n); return signo ? conSigno(x, n) : x; };
  }
  tipado('Uint8Array', 1, recortar(1, false), 'getUint8');
  tipado('Int8Array', 1, recortar(1, true), 'getInt8');
  tipado('Uint8ClampedArray', 1, function (x) { x = +x || 0; return x < 0 ? 0 : x > 255 ? 255 : Math.round(x); }, 'getUint8');
  tipado('Uint16Array', 2, recortar(2, false), 'getUint16');
  tipado('Int16Array', 2, recortar(2, true), 'getInt16');
  tipado('Uint32Array', 4, recortar(4, false), 'getUint32');
  tipado('Int32Array', 4, recortar(4, true), 'getInt32');
  tipado('Float32Array', 4, Math.fround, 'getFloat32');
  tipado('Float64Array', 8, function (x) { return +x; }, 'getFloat64');

  var esArreglo = Array.isArray;
  Array.isArray = function (x) { return esArreglo(x) && !x['\u0001tipado']; };

  // ---------------- TextEncoder y TextDecoder (UTF-8) ----------------
  function TextEncoder() {}
  TextEncoder.prototype.encoding = 'utf-8';
  TextEncoder.prototype.encode = function (s) {
    s = String(s === undefined ? '' : s);
    var r = [];
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      if (c >= 0xd800 && c < 0xdc00 && i + 1 < s.length) {
        var d = s.charCodeAt(i + 1);
        if (d >= 0xdc00 && d < 0xe000) { c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00); i++; }
      }
      if (c >= 0xd800 && c < 0xe000) c = 0xfffd; // suelto: el carácter de reemplazo
      if (c < 0x80) r.push(c);
      else if (c < 0x800) r.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000) r.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else r.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return new Uint8Array(r);
  };
  function TextDecoder(codificacion) {
    var c = String(codificacion === undefined ? 'utf-8' : codificacion).toLowerCase();
    if (c !== 'utf-8' && c !== 'utf8') throw new RangeError('TextDecoder: solo utf-8 en QUIMERA');
    this.encoding = 'utf-8';
  }
  // Lo que no es UTF-8 válido sale como U+FFFD, como en los navegadores.
  TextDecoder.prototype.decode = function (b) {
    if (b === undefined) return '';
    if (b instanceof ArrayBuffer) b = new Uint8Array(b);
    else if (b instanceof DataView) b = new Uint8Array(b.buffer, b.byteOffset, b.byteLength);
    var s = '', i = 0, n = b.length;
    while (i < n) {
      var c = b[i] & 255, extra = c < 0x80 ? 0 : c >= 0xf0 && c < 0xf5 ? 3 : c >= 0xe0 ? 2 : c >= 0xc2 && c < 0xe0 ? 1 : -1;
      if (extra < 0) { s += '�'; i++; continue; }
      var cp = extra === 0 ? c : c & (0x3f >> extra), ok = i + extra < n;
      for (var k = 1; ok && k <= extra; k++) {
        var d = b[i + k] & 255;
        if ((d & 0xc0) !== 0x80) ok = false; else cp = cp * 64 + (d & 63);
      }
      if (!ok) { s += '�'; i++; continue; }
      i += extra + 1;
      if (cp >= 0x10000) { cp -= 0x10000; s += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 1023)); }
      else s += String.fromCharCode(cp);
    }
    return s;
  };
  ocultar.push(TextEncoder.prototype, TextDecoder.prototype);

  // ---------------- crypto ----------------
  // El azar sale del sistema (lo da el anfitrión con `Motor::con_entropia`); sin él, lanza.
  var crypto = {
    getRandomValues: function (a) {
      if (!a || !a['\u0001tipado'] || a instanceof Float32Array || a instanceof Float64Array) {
        throw new TypeError('getRandomValues necesita un arreglo tipado de enteros');
      }
      var T = tipoDe(a), n = T.BYTES_PER_ELEMENT;
      if (a.length * n > 65536) throw new RangeError('getRandomValues: más de 65536 bytes');
      var b = __quimera_aleatorios(a.length * n);
      for (var i = 0; i < a.length; i++) {
        var v = 0;
        for (var k = 0; k < n; k++) v = v * 256 + b[i * n + k];
        a[i] = T.convertir(v);
      }
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
