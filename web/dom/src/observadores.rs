//! `ResizeObserver` e `IntersectionObserver`, con las medidas de la maquetación.
//!
//! Se comprueban en el fotograma (16 ms) que sigue a una tarea que cambió el árbol (un script,
//! un temporizador, un evento), como hacen los navegadores tras maquetar:
//!
//! - **`ResizeObserver`** avisa la primera vez que observa un elemento y cada vez que cambia el
//!   tamaño de su caja (`contentRect`, `borderBoxSize`, `contentBoxSize`; por ahora todos con la
//!   caja de borde).
//! - **`IntersectionObserver`** (contra la ventana, con `rootMargin` en píxeles o porcentaje) avisa la primera
//!   vez y cuando un elemento entra o sale, o cruza uno de sus `threshold`.
//!
//! Si los avisos cambian el árbol, se vuelve a mirar en el siguiente fotograma (16 ms).
//!
//! **`MutationObserver`**: el árbol anota sus cambios mientras hay alguno observando
//! (`mutaciones.rs`); al acabar cada tarea llegan aquí y se reparten según `childList`,
//! `attributes` (con `attributeFilter`), `characterData`, `subtree` y los `...OldValue`. Polymer
//! encola sus microtareas cambiando el texto de un nodo observado.

/// El código (tras `complementos.rs`).
pub(crate) const OBSERVADORES_JS: &str = r#"(function (w, d) {
  var vivos = [];
  function caja(el) {
    var r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, ancho: r.width, alto: r.height };
  }
  // Los avisos son instancias, con sus campos en el prototipo: el polyfill de la W3C (que usa
  // YouTube) solo se aparta si ve `'intersectionRatio' in IntersectionObserverEntry.prototype`.
  function IntersectionObserverEntry(d) { for (var k in d) this[k] = d[k]; }
  ['time', 'target', 'rootBounds', 'boundingClientRect', 'intersectionRect', 'intersectionRatio',
    'isIntersecting', 'isVisible'].forEach(function (k) { IntersectionObserverEntry.prototype[k] = null; });
  function ResizeObserverEntry(d) { for (var k in d) this[k] = d[k]; }
  ['target', 'contentRect', 'borderBoxSize', 'contentBoxSize', 'devicePixelContentBoxSize']
    .forEach(function (k) { ResizeObserverEntry.prototype[k] = null; });
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
      avisos.push(new ResizeObserverEntry({ target: o.el, contentRect: new DOMRect(0, 0, c.ancho, c.alto), borderBoxSize: tam,
        contentBoxSize: tam, devicePixelContentBoxSize: tam }));
    });
    if (avisos.length) this['\u0001f'].call(this, avisos, this);
    return avisos.length > 0;
  };

  function IntersectionObserver(f, op) {
    if (typeof f !== 'function') throw new TypeError('IntersectionObserver necesita una función');
    op = op || {};
    var umbrales = op.threshold === undefined ? [0] : [].concat(op.threshold);
    // Cada lado en píxeles o en porcentaje (del alto de la raíz arriba y abajo, del ancho a los lados).
    var margen = String(op.rootMargin || '0px').trim().split(/\s+/).map(function (m) {
      return { v: parseFloat(m) || 0, pc: /%$/.test(m) };
    });
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
    m = m.map(function (x, i) { return x.pc ? x.v * (i % 2 ? raiz.width : raiz.height) / 100 : x.v; });
    var r0 = { left: raiz.left - m[3], top: raiz.top - m[0], right: raiz.right + m[1], bottom: raiz.bottom + m[2] };
    this['\u0001obs'].forEach(function (o) {
      var b = o.el.getBoundingClientRect();
      var x0 = Math.max(b.left, r0.left), y0 = Math.max(b.top, r0.top);
      var x1 = Math.min(b.right, r0.right), y1 = Math.min(b.bottom, r0.bottom);
      // Un elemento sin área (una imagen sin cargar) también cruza si está dentro; uno sin caja
      // (`display: none`), no.
      var corta = o.el.isConnected && o.el.__quimera_con_caja !== false && x1 >= x0 && y1 >= y0;
      var area = b.width * b.height, dentro = corta ? (x1 - x0) * (y1 - y0) : 0;
      var razon = corta ? (area > 0 ? dentro / area : 1) : 0;
      // Qué umbral se ha cruzado (-1: fuera).
      var nivel = -1;
      if (corta) for (var i = 0; i < yo.thresholds.length; i++) if (razon >= yo.thresholds[i]) nivel = i;
      if (nivel === o.nivel) return;
      o.nivel = nivel;
      var inter = corta ? new DOMRect(x0, y0, x1 - x0, y1 - y0) : new DOMRect(0, 0, 0, 0);
      avisos.push(new IntersectionObserverEntry({ target: o.el, isIntersecting: corta, isVisible: false, intersectionRatio: razon, time: performance.now(),
        boundingClientRect: b, intersectionRect: inter, rootBounds: new DOMRect(r0.left, r0.top, r0.right - r0.left, r0.bottom - r0.top) }));
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
  // ---------------- MutationObserver ----------------
  var mutantes = [];
  function MutationObserver(f) {
    if (typeof f !== 'function') throw new TypeError('MutationObserver necesita una función');
    this['\u0001f'] = f; this['\u0001obs'] = []; this['\u0001cola'] = [];
  }
  MutationObserver.prototype.observe = function (nodo, op) {
    if (!nodo || typeof nodo.nodeType !== 'number') throw new TypeError('observe necesita un nodo');
    op = op || {};
    var o = { nodo: nodo, hijos: !!op.childList, subtree: !!op.subtree,
      atributos: op.attributes !== undefined ? !!op.attributes : !!(op.attributeOldValue || op.attributeFilter),
      texto: op.characterData !== undefined ? !!op.characterData : !!op.characterDataOldValue,
      viejoAtributo: !!op.attributeOldValue, viejoTexto: !!op.characterDataOldValue,
      filtro: op.attributeFilter ? [].concat(op.attributeFilter).map(function (a) { return String(a).toLowerCase(); }) : null };
    if (!o.hijos && !o.atributos && !o.texto) throw new TypeError('observe necesita childList, attributes o characterData');
    this['\u0001obs'] = this['\u0001obs'].filter(function (x) { return x.nodo !== nodo; }).concat([o]);
    if (mutantes.indexOf(this) < 0) mutantes.push(this);
    __quimera_observar_mutaciones(nodo, o.subtree);
  };
  MutationObserver.prototype.disconnect = function () {
    this['\u0001obs'] = []; this['\u0001cola'] = [];
    var i = mutantes.indexOf(this);
    if (i >= 0) mutantes.splice(i, 1);
  };
  MutationObserver.prototype.takeRecords = function () { var c = this['\u0001cola']; this['\u0001cola'] = []; return c; };
  var TIPOS = ['childList', 'attributes', 'characterData'];
  // Cada cambio llega como [tipo, objetivo, anterior, siguiente, atributo, viejo, añadidos, quitados].
  w.__quimera_mutaciones = function (lista) {
    lista.forEach(function (c) {
      var tipo = TIPOS[c[0]], objetivo = c[1];
      mutantes.forEach(function (mo) {
        var o = null;
        mo['\u0001obs'].forEach(function (x) {
          if (o) return;
          if (x.nodo !== objetivo && !(x.subtree && x.nodo.contains && x.nodo.contains(objetivo))) return;
          if (tipo === 'childList' && !x.hijos) return;
          if (tipo === 'attributes' && (!x.atributos || (x.filtro && x.filtro.indexOf(c[4]) < 0))) return;
          if (tipo === 'characterData' && !x.texto) return;
          o = x;
        });
        if (!o) return;
        var viejo = (tipo === 'attributes' && o.viejoAtributo) || (tipo === 'characterData' && o.viejoTexto) ? c[5] : null;
        mo['\u0001cola'].push({ type: tipo, target: objetivo, addedNodes: c[6], removedNodes: c[7], previousSibling: c[2],
          nextSibling: c[3], attributeName: c[4], attributeNamespace: null, oldValue: viejo });
      });
    });
    mutantes.slice().forEach(function (mo) {
      var cola = mo.takeRecords();
      if (cola.length) { try { mo['\u0001f'].call(mo, cola, mo); } catch (e) { console.error(String(e && e.stack || e)); } }
    });
  };
  // La microtarea que avisa, encolada una vez tras el primer cambio (la pide el DOM).
  var encolada = false;
  w.__quimera_programar_mutaciones = function () {
    if (encolada) return;
    encolada = true;
    queueMicrotask(function () {
      encolada = false;
      var l = __quimera_tomar_mutaciones();
      if (l.length) w.__quimera_mutaciones(l);
    });
  };
  w.MutationObserver = MutationObserver;
  w.WebKitMutationObserver = MutationObserver;
  w.MutationRecord = function MutationRecord() {};
  __quimera_no_enumerar(MutationObserver.prototype);

  w.ResizeObserver = ResizeObserver;
  w.IntersectionObserver = IntersectionObserver;
  w.IntersectionObserverEntry = IntersectionObserverEntry;
  w.ResizeObserverEntry = ResizeObserverEntry;
  __quimera_no_enumerar(ResizeObserver.prototype);
  __quimera_no_enumerar(IntersectionObserver.prototype);
})(this, this.document);"#;
