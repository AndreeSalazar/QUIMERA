//! `ResizeObserver` e `IntersectionObserver`, con las medidas de la maquetación.
//!
//! Se comprueban en el fotograma (16 ms) que sigue a una tarea que cambió el árbol (un script,
//! un temporizador, un evento), como hacen los navegadores tras maquetar:
//!
//! - **`ResizeObserver`** avisa la primera vez que observa un elemento y cada vez que cambia el
//!   tamaño de su caja (`contentRect`, `borderBoxSize`, `contentBoxSize`; por ahora todos con la
//!   caja de borde).
//! - **`IntersectionObserver`** (contra la ventana, con `rootMargin` en píxeles) avisa la primera
//!   vez y cuando un elemento entra o sale, o cruza uno de sus `threshold`.
//!
//! Si los avisos cambian el árbol, se vuelve a mirar en el siguiente fotograma (16 ms).
//! `MutationObserver` sigue sin avisar.

/// El código (tras `complementos.rs`).
pub(crate) const OBSERVADORES_JS: &str = r#"(function (w, d) {
  var vivos = [];
  function caja(el) {
    var r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, ancho: r.width, alto: r.height };
  }
  function quitar(o) { var i = vivos.indexOf(o); if (i >= 0) vivos.splice(i, 1); }

  function ResizeObserver(f) {
    if (typeof f !== 'function') throw new TypeError('ResizeObserver necesita una función');
    this['\u0001f'] = f; this['\u0001obs'] = [];
  }
  ResizeObserver.prototype.observe = function (el) {
    var obs = this['\u0001obs'];
    for (var i = 0; i < obs.length; i++) if (obs[i].el === el) return;
    obs.push({ el: el, ancho: -1, alto: -1 });
    if (vivos.indexOf(this) < 0) vivos.push(this);
    pronto();
  };
  ResizeObserver.prototype.unobserve = function (el) {
    this['\u0001obs'] = this['\u0001obs'].filter(function (o) { return o.el !== el; });
    if (!this['\u0001obs'].length) quitar(this);
  };
  ResizeObserver.prototype.disconnect = function () { this['\u0001obs'] = []; quitar(this); };
  ResizeObserver.prototype['\u0001mirar'] = function () {
    var avisos = [];
    this['\u0001obs'].forEach(function (o) {
      if (!o.el.isConnected) return;
      var c = caja(o.el);
      if (c.ancho === o.ancho && c.alto === o.alto) return;
      o.ancho = c.ancho; o.alto = c.alto;
      var tam = [{ inlineSize: c.ancho, blockSize: c.alto }];
      avisos.push({ target: o.el, contentRect: new DOMRect(0, 0, c.ancho, c.alto), borderBoxSize: tam,
        contentBoxSize: tam, devicePixelContentBoxSize: tam });
    });
    if (avisos.length) this['\u0001f'].call(this, avisos, this);
    return avisos.length > 0;
  };

  function IntersectionObserver(f, op) {
    if (typeof f !== 'function') throw new TypeError('IntersectionObserver necesita una función');
    op = op || {};
    var umbrales = op.threshold === undefined ? [0] : [].concat(op.threshold);
    var margen = String(op.rootMargin || '0px').split(/\s+/).map(function (m) { return parseFloat(m) || 0; });
    while (margen.length < 4) margen.push(margen[margen.length > 1 ? margen.length - 2 : 0]);
    this.root = op.root || null; this.rootMargin = String(op.rootMargin || '0px 0px 0px 0px');
    this.thresholds = umbrales.slice().sort();
    this['\u0001f'] = f; this['\u0001obs'] = []; this['\u0001margen'] = margen;
  }
  IntersectionObserver.prototype.observe = function (el) {
    var obs = this['\u0001obs'];
    for (var i = 0; i < obs.length; i++) if (obs[i].el === el) return;
    obs.push({ el: el, nivel: -2 });
    if (vivos.indexOf(this) < 0) vivos.push(this);
    pronto();
  };
  IntersectionObserver.prototype.unobserve = ResizeObserver.prototype.unobserve;
  IntersectionObserver.prototype.disconnect = ResizeObserver.prototype.disconnect;
  IntersectionObserver.prototype.takeRecords = function () { return []; };
  IntersectionObserver.prototype['\u0001mirar'] = function () {
    var yo = this, m = this['\u0001margen'], avisos = [];
    var raiz = this.root && this.root.getBoundingClientRect ? this.root.getBoundingClientRect()
      : { left: 0, top: 0, right: w.innerWidth, bottom: w.innerHeight, width: w.innerWidth, height: w.innerHeight };
    var r0 = { left: raiz.left - m[3], top: raiz.top - m[0], right: raiz.right + m[1], bottom: raiz.bottom + m[2] };
    this['\u0001obs'].forEach(function (o) {
      var b = o.el.getBoundingClientRect();
      var x0 = Math.max(b.left, r0.left), y0 = Math.max(b.top, r0.top);
      var x1 = Math.min(b.right, r0.right), y1 = Math.min(b.bottom, r0.bottom);
      var corta = o.el.isConnected && x1 >= x0 && y1 >= y0 && (b.width > 0 || b.height > 0);
      var area = b.width * b.height, dentro = corta ? (x1 - x0) * (y1 - y0) : 0;
      var razon = corta ? (area > 0 ? dentro / area : 1) : 0;
      // Qué umbral se ha cruzado (-1: fuera).
      var nivel = -1;
      if (corta) for (var i = 0; i < yo.thresholds.length; i++) if (razon >= yo.thresholds[i]) nivel = i;
      if (nivel === o.nivel) return;
      o.nivel = nivel;
      var inter = corta ? new DOMRect(x0, y0, x1 - x0, y1 - y0) : new DOMRect(0, 0, 0, 0);
      avisos.push({ target: o.el, isIntersecting: corta, intersectionRatio: razon, time: performance.now(),
        boundingClientRect: b, intersectionRect: inter, rootBounds: new DOMRect(r0.left, r0.top, r0.right - r0.left, r0.bottom - r0.top) });
    });
    if (avisos.length) this['\u0001f'].call(this, avisos, this);
    return avisos.length > 0;
  };

  // Tras una tarea que cambió el árbol (lo llama el DOM) o al empezar a observar.
  var programado = false;
  function pronto() {
    if (programado) return;
    programado = true;
    setTimeout(function () { programado = false; w.__quimera_observar(); }, 16);
  }
  // Lo llama el DOM al acabar una tarea que cambió el árbol: se mira en el siguiente fotograma.
  w.__quimera_tras_cambio = function () { if (vivos.length) pronto(); };
  w.__quimera_observar = function () {
    if (!vivos.length) return;
    var hubo = false;
    vivos.slice().forEach(function (o) {
      try { hubo = o['\u0001mirar']() || hubo; } catch (e) { console.error(String(e && e.stack || e)); }
    });
    if (hubo) pronto();
  };
  w.ResizeObserver = ResizeObserver;
  w.IntersectionObserver = IntersectionObserver;
  w.IntersectionObserverEntry = function IntersectionObserverEntry() {};
  w.ResizeObserverEntry = function ResizeObserverEntry() {};
  __quimera_no_enumerar(ResizeObserver.prototype);
  __quimera_no_enumerar(IntersectionObserver.prototype);
})(this, this.document);"#;
