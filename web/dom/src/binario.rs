//! Datos binarios en la red y en los archivos: `Blob`, `File`, un `ReadableStream` sencillo, y
//! `arrayBuffer()`, `blob()` y `body` en las respuestas de `fetch` (YouTube recibe su vídeo en
//! binario, por trozos de un `ReadableStream`).
//!
//! Los bytes viajan en `ArrayBuffer` nativos de la VM: un `Uint8Array` sobre uno es una vista,
//! sin copiar.

/// El código (tras `complementos.rs`).
pub(crate) const BINARIO_JS: &str = r#"(function (w) {
  var B = '\u0001b';
  // Un ArrayBuffer con los bytes de cualquier cosa binaria (el mismo, si ya lo es entero); el
  // texto, en UTF-8.
  function bytesDe(x) {
    if (x == null) return new ArrayBuffer(0);
    if (x instanceof ArrayBuffer) return x;
    if (ArrayBuffer.isView(x)) {
      if (x.byteOffset === 0 && x.byteLength === x.buffer.byteLength) return x.buffer;
      return x.buffer.slice(x.byteOffset, x.byteOffset + x.byteLength);
    }
    if (x instanceof Blob) return x[B];
    return new TextEncoder().encode(String(x)).buffer;
  }
  function textoDe(buffer) { return new TextDecoder().decode(buffer); }
  w.__quimera_bytes_de = bytesDe;

  function Blob(partes, op) {
    var trozos = (partes || []).map(bytesDe), total = 0, i;
    for (i = 0; i < trozos.length; i++) total += trozos[i].byteLength;
    var u = new Uint8Array(total), o = 0;
    for (i = 0; i < trozos.length; i++) { u.set(new Uint8Array(trozos[i]), o); o += trozos[i].byteLength; }
    this[B] = u.buffer;
    this.size = total;
    this.type = op && op.type ? String(op.type).toLowerCase() : '';
  }
  Blob.prototype.arrayBuffer = function () { return Promise.resolve(this[B].slice(0)); };
  Blob.prototype.bytes = function () { return Promise.resolve(new Uint8Array(this[B].slice(0))); };
  Blob.prototype.text = function () { return Promise.resolve(textoDe(this[B])); };
  Blob.prototype.slice = function (a, z, tipo) {
    var r = new Blob([], { type: tipo });
    r[B] = this[B].slice(a, z);
    r.size = r[B].byteLength;
    return r;
  };
  Blob.prototype.stream = function () {
    var b = this[B];
    return new ReadableStream({ start: function (c) { c.enqueue(new Uint8Array(b.slice(0))); c.close(); } });
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
  R.arrayBuffer = function () { this.bodyUsed = true; return Promise.resolve(bytesRespuesta(this).slice(0)); };
  R.bytes = function () { this.bodyUsed = true; return Promise.resolve(new Uint8Array(bytesRespuesta(this).slice(0))); };
  R.blob = function () {
    this.bodyUsed = true;
    var b = new Blob([], { type: this.headers.get('content-type') || '' });
    b[B] = bytesRespuesta(this).slice(0);
    b.size = b[B].byteLength;
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
      this['\u0001flujo'] = new ReadableStream({ start: function (c) { c.enqueue(new Uint8Array(b)); c.close(); } });
    }
    return this['\u0001flujo'];
  } });

})(this);"#;
