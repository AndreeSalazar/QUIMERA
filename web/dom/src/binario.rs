//! Datos binarios en la red y en los archivos: `Blob`, `File`, un `ReadableStream` sencillo, y
//! `arrayBuffer()`, `blob()` y `body` en las respuestas de `fetch` (YouTube recibe su vídeo en
//! binario, por trozos de un `ReadableStream`).
//!
//! Los bytes viajan como el arreglo oculto de un `ArrayBuffer` (ver `binarios.js` en la VM): un
//! `Uint8Array` sobre un `ArrayBuffer` entero es ese mismo arreglo, sin copiar.

/// El código (tras `complementos.rs`).
pub(crate) const BINARIO_JS: &str = r#"(function (w) {
  var BYTES = '\u0001bytes', B = '\u0001b';
  // Los bytes (un arreglo de 0 a 255) de cualquier cosa binaria; el texto, en UTF-8.
  function bytesDe(x) {
    if (x == null) return [];
    if (x instanceof ArrayBuffer) return x[BYTES];
    if (ArrayBuffer.isView(x)) {
      if (x instanceof Uint8Array && x.buffer && x.byteOffset === 0 && x.byteLength === x.buffer.byteLength) return x.buffer[BYTES];
      return Array.prototype.slice.call(new Uint8Array(x.buffer, x.byteOffset, x.byteLength));
    }
    if (x instanceof Blob) return x[B];
    var u = new TextEncoder().encode(String(x));
    return u.buffer ? u.buffer[BYTES] : Array.prototype.slice.call(u);
  }
  function bufferDe(bytes) { var ab = new ArrayBuffer(0); ab[BYTES] = bytes; return ab; }
  function textoDe(bytes) { return new TextDecoder().decode(new Uint8Array(bufferDe(bytes))); }
  w.__quimera_bytes_de = bytesDe;
  w.__quimera_buffer_de = bufferDe;

  function Blob(partes, op) {
    var b = [];
    (partes || []).forEach(function (p) { var x = bytesDe(p); for (var i = 0; i < x.length; i++) b.push(x[i]); });
    this[B] = b;
    this.size = b.length;
    this.type = op && op.type ? String(op.type).toLowerCase() : '';
  }
  Blob.prototype.arrayBuffer = function () { return Promise.resolve(bufferDe(this[B].slice())); };
  Blob.prototype.bytes = function () { return Promise.resolve(new Uint8Array(bufferDe(this[B].slice()))); };
  Blob.prototype.text = function () { return Promise.resolve(textoDe(this[B])); };
  Blob.prototype.slice = function (a, z, tipo) {
    var r = new Blob([], { type: tipo });
    r[B] = this[B].slice(a, z);
    r.size = r[B].length;
    return r;
  };
  Blob.prototype.stream = function () {
    var b = this[B];
    return new ReadableStream({ start: function (c) { c.enqueue(new Uint8Array(bufferDe(b.slice()))); c.close(); } });
  };
  w.Blob = Blob;
  function File(partes, nombre, op) {
    Blob.call(this, partes, op);
    this.name = String(nombre);
    this.lastModified = op && op.lastModified !== undefined ? op.lastModified : Date.now();
  }
  File.prototype = Object.create(Blob.prototype);
  File.prototype.constructor = File;
  w.File = File;

  // Un flujo que lee lo que su fuente va encolando (`start`, `pull`, `cancel`).
  function ReadableStream(fuente) {
    var cola = [], cerrado = false, fallo = null, esperan = [];
    function atender() {
      while (esperan.length && (cola.length || cerrado || fallo)) {
        var e = esperan.shift();
        if (cola.length) e.res({ value: cola.shift(), done: false });
        else if (fallo) e.rej(fallo);
        else e.res({ value: undefined, done: true });
      }
    }
    var control = {
      enqueue: function (v) { if (!cerrado) { cola.push(v); atender(); } },
      close: function () { cerrado = true; atender(); },
      error: function (e) { fallo = e; atender(); },
      desiredSize: 1
    };
    this.locked = false;
    this['\u0001leer'] = function () {
      return new Promise(function (res, rej) {
        esperan.push({ res: res, rej: rej });
        if (!cola.length && !cerrado && !fallo && fuente && fuente.pull) {
          try { fuente.pull(control); } catch (e) { fallo = e; }
        }
        atender();
      });
    };
    this['\u0001cancelar'] = function (m) {
      cerrado = true; cola = [];
      if (fuente && fuente.cancel) { try { fuente.cancel(m); } catch (e) {} }
      atender();
      return Promise.resolve();
    };
    if (fuente && fuente.start) { try { fuente.start(control); } catch (e) { fallo = e; } }
  }
  ReadableStream.prototype.getReader = function () {
    var s = this;
    s.locked = true;
    return {
      read: function () { return s['\u0001leer'](); },
      releaseLock: function () { s.locked = false; },
      cancel: function (m) { return s['\u0001cancelar'](m); },
      closed: new Promise(function () {})
    };
  };
  ReadableStream.prototype.cancel = function (m) { return this['\u0001cancelar'](m); };
  w.ReadableStream = ReadableStream;

  // `Response`: los bytes, si llegaron en binario o se construyó con algo binario.
  var R = w.Response.prototype;
  function bytesRespuesta(r) { return r['\u0001bytes'] || bytesDe(r['\u0001cuerpo']); }
  R.arrayBuffer = function () { this.bodyUsed = true; return Promise.resolve(bufferDe(bytesRespuesta(this).slice())); };
  R.bytes = function () { this.bodyUsed = true; return Promise.resolve(new Uint8Array(bufferDe(bytesRespuesta(this).slice()))); };
  R.blob = function () {
    this.bodyUsed = true;
    var b = new Blob([], { type: this.headers.get('content-type') || '' });
    b[B] = bytesRespuesta(this).slice();
    b.size = b[B].length;
    return Promise.resolve(b);
  };
  var texto = R.text;
  R.text = function () {
    if (this['\u0001bytes'] && this['\u0001cuerpo'] === '') { this.bodyUsed = true; return Promise.resolve(textoDe(this['\u0001bytes'])); }
    return texto.call(this);
  };
  Object.defineProperty(R, 'body', { configurable: true, get: function () {
    if (!this['\u0001flujo']) {
      var b = bytesRespuesta(this);
      this['\u0001flujo'] = new ReadableStream({ start: function (c) { c.enqueue(new Uint8Array(bufferDe(b))); c.close(); } });
    }
    return this['\u0001flujo'];
  } });

})(this);"#;
