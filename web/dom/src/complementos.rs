//! Piezas del DOM que piden las aplicaciones grandes (YouTube) al arrancar, en JavaScript:
//!
//! - `NodeFilter`, `document.createTreeWalker` y `createNodeIterator` (el *polyfill* de Web
//!   Components recorre el árbol con ellos), con el algoritmo de la especificación.
//! - `new Image(ancho, alto)`.
//! - `canvas.getContext('2d')`: un contexto que **no dibuja** pero guarda su estado (`fillStyle`,
//!   `font`...) y mide texto aproximado, para que las librerías que lo usan para leer colores o
//!   medir no se caigan. WebGL da `null`, como un navegador sin GPU para la página.
//! - Los `init...Event` antiguos (`initCustomEvent`, `initMouseEvent`...).
//! - `Window` (de quien hereda `window`), `Range` (`createContextualFragment`...), una selección
//!   vacía (`getSelection`) y `MessageChannel`.

/// El código (tras `interfaces.rs`).
pub(crate) const COMPLEMENTOS_JS: &str = r#"(function (w, d, P) {
  // ---------------- NodeFilter, TreeWalker, NodeIterator ----------------
  var NodeFilter = {
    FILTER_ACCEPT: 1, FILTER_REJECT: 2, FILTER_SKIP: 3,
    SHOW_ALL: 0xFFFFFFFF, SHOW_ELEMENT: 0x1, SHOW_ATTRIBUTE: 0x2, SHOW_TEXT: 0x4, SHOW_CDATA_SECTION: 0x8,
    SHOW_PROCESSING_INSTRUCTION: 0x40, SHOW_COMMENT: 0x80, SHOW_DOCUMENT: 0x100, SHOW_DOCUMENT_TYPE: 0x200,
    SHOW_DOCUMENT_FRAGMENT: 0x400
  };
  w.NodeFilter = NodeFilter;
  function filtrar(r, n) {
    if (!(r.whatToShow & (1 << (n.nodeType - 1)))) return 3;
    var f = r.filter;
    if (!f) return 1;
    return typeof f === 'function' ? f(n) : f.acceptNode(n);
  }
  function TreeWalker(raiz, mostrar, filtro) {
    this.root = raiz; this.currentNode = raiz;
    this.whatToShow = mostrar === undefined ? NodeFilter.SHOW_ALL : mostrar >>> 0;
    this.filter = filtro || null;
  }
  // Recorre los hijos (primero o último) según la especificación.
  function hijos(tw, primero) {
    var n = primero ? tw.currentNode.firstChild : tw.currentNode.lastChild;
    while (n) {
      var r = filtrar(tw, n);
      if (r === 1) { tw.currentNode = n; return n; }
      if (r === 3) {
        var h = primero ? n.firstChild : n.lastChild;
        if (h) { n = h; continue; }
      }
      while (n) {
        var hermano = primero ? n.nextSibling : n.previousSibling;
        if (hermano) { n = hermano; break; }
        var padre = n.parentNode;
        if (!padre || padre === tw.root || padre === tw.currentNode) return null;
        n = padre;
      }
    }
    return null;
  }
  function hermanos(tw, siguiente) {
    var n = tw.currentNode;
    if (n === tw.root) return null;
    while (true) {
      var s = siguiente ? n.nextSibling : n.previousSibling;
      while (s) {
        n = s;
        var r = filtrar(tw, n);
        if (r === 1) { tw.currentNode = n; return n; }
        s = siguiente ? n.firstChild : n.lastChild;
        if (r === 2 || !s) s = siguiente ? n.nextSibling : n.previousSibling;
      }
      n = n.parentNode;
      if (!n || n === tw.root) return null;
      if (filtrar(tw, n) === 1) return null;
    }
  }
  TreeWalker.prototype = {
    parentNode: function () {
      var n = this.currentNode;
      while (n && n !== this.root) {
        n = n.parentNode;
        if (n && filtrar(this, n) === 1) { this.currentNode = n; return n; }
      }
      return null;
    },
    firstChild: function () { return hijos(this, true); },
    lastChild: function () { return hijos(this, false); },
    nextSibling: function () { return hermanos(this, true); },
    previousSibling: function () { return hermanos(this, false); },
    nextNode: function () {
      var n = this.currentNode, r = 1;
      while (true) {
        while (r !== 2 && n.firstChild) {
          n = n.firstChild;
          r = filtrar(this, n);
          if (r === 1) { this.currentNode = n; return n; }
        }
        var s = null, t = n;
        while (t) {
          if (t === this.root) return null;
          s = t.nextSibling;
          if (s) break;
          t = t.parentNode;
        }
        if (!s) return null;
        n = s;
        r = filtrar(this, n);
        if (r === 1) { this.currentNode = n; return n; }
      }
    },
    previousNode: function () {
      var n = this.currentNode;
      while (n !== this.root) {
        var s = n.previousSibling;
        while (s) {
          n = s;
          var r = filtrar(this, n);
          while (r !== 2 && n.lastChild) { n = n.lastChild; r = filtrar(this, n); }
          if (r === 1) { this.currentNode = n; return n; }
          s = n.previousSibling;
        }
        if (n === this.root || !n.parentNode) return null;
        n = n.parentNode;
        if (filtrar(this, n) === 1) { this.currentNode = n; return n; }
      }
      return null;
    }
  };
  w.TreeWalker = TreeWalker;
  P.p_documento.createTreeWalker = function (raiz, mostrar, filtro) { return new TreeWalker(raiz, mostrar, filtro); };
  // NodeIterator: en orden de documento; los rechazados se saltan solos (no su subárbol).
  function NodeIterator(raiz, mostrar, filtro) {
    this.root = raiz; this.referenceNode = raiz; this.pointerBeforeReferenceNode = true;
    this.whatToShow = mostrar === undefined ? NodeFilter.SHOW_ALL : mostrar >>> 0;
    this.filter = filtro || null;
  }
  function siguienteEnOrden(n, raiz) {
    if (n.firstChild) return n.firstChild;
    while (n && n !== raiz) {
      if (n.nextSibling) return n.nextSibling;
      n = n.parentNode;
    }
    return null;
  }
  function anteriorEnOrden(n, raiz) {
    if (n === raiz) return null;
    var p = n.previousSibling;
    if (!p) return n.parentNode;
    while (p.lastChild) p = p.lastChild;
    return p;
  }
  NodeIterator.prototype = {
    nextNode: function () {
      var n = this.referenceNode, antes = this.pointerBeforeReferenceNode;
      while (true) {
        if (antes) antes = false; else { n = siguienteEnOrden(n, this.root); if (!n) return null; }
        if (filtrar(this, n) === 1) { this.referenceNode = n; this.pointerBeforeReferenceNode = false; return n; }
      }
    },
    previousNode: function () {
      var n = this.referenceNode, antes = this.pointerBeforeReferenceNode;
      while (true) {
        if (!antes) antes = true; else { n = anteriorEnOrden(n, this.root); if (!n) return null; }
        if (filtrar(this, n) === 1) { this.referenceNode = n; this.pointerBeforeReferenceNode = true; return n; }
      }
    },
    detach: function () {}
  };
  w.NodeIterator = NodeIterator;
  P.p_documento.createNodeIterator = function (raiz, mostrar, filtro) { return new NodeIterator(raiz, mostrar, filtro); };

  // ---------------- Image ----------------
  w.Image = function Image(ancho, alto) {
    var img = d.createElement('img');
    if (ancho !== undefined) img.setAttribute('width', String(ancho));
    if (alto !== undefined) img.setAttribute('height', String(alto));
    return img;
  };
  w.Image.prototype = w.HTMLImageElement.prototype;
  // `new Audio(src)`: un `<audio>` con `preload="auto"` (aún no suena: falta la fase de medios).
  w.Audio = function Audio(src) {
    var a = d.createElement('audio');
    a.setAttribute('preload', 'auto');
    if (src !== undefined) a.setAttribute('src', String(src));
    return a;
  };
  if (w.HTMLAudioElement) w.Audio.prototype = w.HTMLAudioElement.prototype;
  // `<audio>` y `<video>` sin reproductor todavía: el estado de uno que no ha cargado nada.
  // `canPlayType` da '' (no puede), y `play()` se queda en pausa sin error.
  var M = w.HTMLMediaElement.prototype;
  M.load = function () {};
  M.pause = function () {};
  M.play = function () { return Promise.resolve(); };
  M.canPlayType = function () { return ''; };
  M.addTextTrack = function () { return { cues: [], mode: 'disabled', addCue: function () {} }; };
  [['paused', true], ['ended', false], ['seeking', false], ['currentTime', 0], ['duration', NaN],
    ['volume', 1], ['muted', false], ['playbackRate', 1], ['readyState', 0], ['networkState', 0],
    ['error', null], ['buffered', { length: 0 }], ['textTracks', []]].forEach(function (par) {
    M[par[0]] = par[1];
  });
  ['HAVE_NOTHING', 'HAVE_METADATA', 'HAVE_CURRENT_DATA', 'HAVE_FUTURE_DATA', 'HAVE_ENOUGH_DATA'].forEach(function (k, i) {
    w.HTMLMediaElement[k] = M[k] = i;
  });
  // `img.width` y `img.height`: números (el atributo, o 0 mientras no se sepa).
  ['width', 'height'].forEach(function (a) {
    Object.defineProperty(w.HTMLImageElement.prototype, a, {
      get: function () { var v = parseInt(this.getAttribute(a), 10); return v >= 0 ? v : 0; },
      set: function (v) { this.setAttribute(a, String(v >>> 0)); }
    });
  });

  // ---------------- canvas ----------------
  function Contexto2D(canvas) {
    this.canvas = canvas;
    this.font = '10px sans-serif';
    this.globalAlpha = 1; this.lineWidth = 1; this.textAlign = 'start'; this.textBaseline = 'alphabetic';
    this.globalCompositeOperation = 'source-over'; this.lineCap = 'butt'; this.lineJoin = 'miter';
    this.shadowBlur = 0; this.shadowColor = 'rgba(0, 0, 0, 0)'; this.imageSmoothingEnabled = true;
  }
  // Los colores se normalizan como en un canvas ('red' → '#ff0000'); uno no válido no cambia nada.
  ['fillStyle', 'strokeStyle', 'shadowColor'].forEach(function (k) {
    var defecto = k === 'shadowColor' ? 'rgba(0, 0, 0, 0)' : '#000000';
    Object.defineProperty(Contexto2D.prototype, k, {
      get: function () { var v = this['\u0001' + k]; return v === undefined ? defecto : v; },
      set: function (v) {
        if (typeof v !== 'string') { this['\u0001' + k] = v; return; }
        var c = __quimera_color(v);
        if (c !== null) this['\u0001' + k] = c;
      }
    });
  });
  ['fillRect', 'strokeRect', 'clearRect', 'fillText', 'strokeText', 'beginPath', 'closePath', 'moveTo', 'lineTo',
    'arc', 'arcTo', 'rect', 'ellipse', 'quadraticCurveTo', 'bezierCurveTo', 'fill', 'stroke', 'clip', 'save', 'restore',
    'translate', 'rotate', 'scale', 'transform', 'setTransform', 'resetTransform', 'drawImage', 'putImageData',
    'setLineDash'].forEach(function (m) { Contexto2D.prototype[m] = function () {}; });
  Contexto2D.prototype.getLineDash = function () { return []; };
  // Texto: medida aproximada (0,55 em por carácter) con el tamaño de `font`.
  Contexto2D.prototype.measureText = function (t) {
    var m = /(\d+(?:\.\d+)?)px/.exec(this.font), px = m ? parseFloat(m[1]) : 10;
    var ancho = String(t).length * px * 0.55;
    return { width: ancho, actualBoundingBoxAscent: px * 0.8, actualBoundingBoxDescent: px * 0.2,
      actualBoundingBoxLeft: 0, actualBoundingBoxRight: ancho, fontBoundingBoxAscent: px * 0.8, fontBoundingBoxDescent: px * 0.2 };
  };
  Contexto2D.prototype.getImageData = function (x, y, a, b) {
    var n = Math.max(0, a | 0) * Math.max(0, b | 0) * 4;
    return { width: a | 0, height: b | 0, data: new Uint8ClampedArray(n) };
  };
  Contexto2D.prototype.createImageData = function (a, b) { return this.getImageData(0, 0, a, b); };
  Contexto2D.prototype.createLinearGradient = Contexto2D.prototype.createRadialGradient = function () { return { addColorStop: function () {} }; };
  Contexto2D.prototype.createPattern = function () { return null; };
  Contexto2D.prototype.isPointInPath = function () { return false; };
  w.CanvasRenderingContext2D = Contexto2D;
  var C = w.HTMLCanvasElement.prototype;
  C.getContext = function (tipo) {
    if (String(tipo) !== '2d') return null; // WebGL y compañía: como un navegador sin GPU para la página
    return this['\u0001contexto'] || (this['\u0001contexto'] = new Contexto2D(this));
  };
  C.toDataURL = function () { return 'data:,'; };
  C.toBlob = function (f) { setTimeout(function () { f(null); }, 0); };
  ['width', 'height'].forEach(function (a, i) {
    Object.defineProperty(C, a, {
      get: function () { var v = parseInt(this.getAttribute(a), 10); return v >= 0 ? v : (i ? 150 : 300); },
      set: function (v) { this.setAttribute(a, String(v >>> 0)); }
    });
  });

  // ---------------- Window ----------------
  // `window` hereda de `Window.prototype`, que hereda de `EventTarget` (como en los navegadores).
  var Window = function Window() { throw new TypeError('Illegal constructor: Window'); };
  Window.prototype = Object.create(P.p_destino);
  Window.prototype.constructor = Window;
  Window.prototype[Symbol.toStringTag] = 'Window';
  if (Object.getPrototypeOf(w) !== Window.prototype) Object.setPrototypeOf(w, Window.prototype);
  w.Window = Window;

  // ---------------- Range y Selection ----------------
  // Un rango entre dos puntos del árbol. Lo que más se usa: `createContextualFragment` (HTML a
  // fragmento, Polymer y Lit) y `selectNodeContents` + `deleteContents`. No se pinta ni mide.
  function indice(n) { var i = 0; while ((n = n.previousSibling)) i++; return i; }
  function largo(n) { return n.nodeType === 1 || n.nodeType === 9 || n.nodeType === 11 ? n.childNodes.length : (n.data || '').length; }
  function Range() {
    this.startContainer = this.endContainer = d; this.startOffset = this.endOffset = 0;
  }
  Range.START_TO_START = 0; Range.START_TO_END = 1; Range.END_TO_END = 2; Range.END_TO_START = 3;
  Range.prototype = {
    get collapsed() { return this.startContainer === this.endContainer && this.startOffset === this.endOffset; },
    get commonAncestorContainer() {
      var a = this.startContainer;
      while (a && !(a === this.endContainer || a.contains(this.endContainer))) a = a.parentNode;
      return a || d;
    },
    setStart: function (n, o) { this.startContainer = n; this.startOffset = o | 0; if (this.collapsed === false && !this.endContainer) this.collapse(true); },
    setEnd: function (n, o) { this.endContainer = n; this.endOffset = o | 0; },
    setStartBefore: function (n) { this.setStart(n.parentNode, indice(n)); },
    setStartAfter: function (n) { this.setStart(n.parentNode, indice(n) + 1); },
    setEndBefore: function (n) { this.setEnd(n.parentNode, indice(n)); },
    setEndAfter: function (n) { this.setEnd(n.parentNode, indice(n) + 1); },
    selectNode: function (n) { this.setStartBefore(n); this.setEndAfter(n); },
    selectNodeContents: function (n) { this.setStart(n, 0); this.setEnd(n, largo(n)); },
    collapse: function (alInicio) {
      if (alInicio) { this.endContainer = this.startContainer; this.endOffset = this.startOffset; }
      else { this.startContainer = this.endContainer; this.startOffset = this.endOffset; }
    },
    cloneRange: function () {
      var r = new Range();
      r.setStart(this.startContainer, this.startOffset); r.setEnd(this.endContainer, this.endOffset);
      return r;
    },
    // Los hijos enteros que quedan dentro (el caso de un rango sobre los hijos de un nodo).
    _hijos: function () {
      var c = this.startContainer;
      if (c !== this.endContainer || (c.nodeType !== 1 && c.nodeType !== 11 && c.nodeType !== 9)) return null;
      return Array.prototype.slice.call(c.childNodes, this.startOffset, this.endOffset);
    },
    deleteContents: function () {
      var h = this._hijos();
      if (h) { h.forEach(function (n) { n.parentNode.removeChild(n); }); this.collapse(true); return; }
      var c = this.startContainer;
      if (c === this.endContainer && typeof c.data === 'string') {
        c.data = c.data.slice(0, this.startOffset) + c.data.slice(this.endOffset);
        this.collapse(true);
      }
    },
    extractContents: function () {
      var f = d.createDocumentFragment(), h = this._hijos();
      if (h) { h.forEach(function (n) { f.appendChild(n); }); this.collapse(true); }
      return f;
    },
    cloneContents: function () {
      var f = d.createDocumentFragment(), h = this._hijos();
      if (h) h.forEach(function (n) { f.appendChild(n.cloneNode(true)); });
      return f;
    },
    insertNode: function (n) {
      var c = this.startContainer;
      if (c.nodeType === 3) {
        var resto = c.splitText ? c.splitText(this.startOffset) : null;
        c.parentNode.insertBefore(n, resto || c.nextSibling);
      } else {
        c.insertBefore(n, c.childNodes[this.startOffset] || null);
      }
    },
    surroundContents: function (n) { n.appendChild(this.extractContents()); this.insertNode(n); this.selectNode(n); },
    createContextualFragment: function (html) {
      var t = d.createElement('template');
      t.innerHTML = String(html);
      var f = d.createDocumentFragment();
      while (t.content.firstChild) f.appendChild(t.content.firstChild);
      return f;
    },
    compareBoundaryPoints: function () { return 0; },
    comparePoint: function () { return 0; },
    isPointInRange: function () { return false; },
    intersectsNode: function () { return false; },
    getBoundingClientRect: function () { return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 }; },
    getClientRects: function () { return []; },
    detach: function () {},
    toString: function () {
      var c = this.startContainer;
      if (c === this.endContainer && typeof c.data === 'string') return c.data.slice(this.startOffset, this.endOffset);
      var h = this._hijos();
      return h ? h.map(function (n) { return n.textContent; }).join('') : '';
    }
  };
  w.Range = Range;
  P.p_documento.createRange = function () { return new Range(); };
  var seleccion = {
    rangeCount: 0, isCollapsed: true, type: 'None', anchorNode: null, focusNode: null, anchorOffset: 0, focusOffset: 0,
    getRangeAt: function () { throw new DOMException('no hay rangos', 'IndexSizeError'); },
    addRange: function () {}, removeAllRanges: function () {}, removeRange: function () {}, empty: function () {},
    collapse: function () {}, selectAllChildren: function () {}, containsNode: function () { return false; },
    toString: function () { return ''; }
  };
  w.getSelection = P.p_documento.getSelection = function () { return seleccion; };

  // ---------------- MessageChannel ----------------
  // Dos puertos enlazados: lo que se manda por uno llega al otro como `message`, en otra tarea.
  class MessagePort extends EventTarget {
    constructor() { super(); this.onmessage = null; this['\u0001otro'] = null; this['\u0001cerrado'] = false; }
    postMessage(dato) {
      var otro = this['\u0001otro'];
      if (!otro || this['\u0001cerrado']) return;
      setTimeout(function () {
        if (otro['\u0001cerrado']) return;
        otro.dispatchEvent(new MessageEvent('message', { data: dato, origin: '', source: null }));
      }, 0);
    }
    start() {}
    close() { this['\u0001cerrado'] = true; }
  }
  w.MessagePort = MessagePort;
  w.MessageChannel = function MessageChannel() {
    this.port1 = new MessagePort(); this.port2 = new MessagePort();
    this.port1['\u0001otro'] = this.port2; this.port2['\u0001otro'] = this.port1;
  };

  // ---------------- init...Event ----------------
  var E = P.p_evento;
  E.initCustomEvent = function (tipo, b, c, detalle) { this.initEvent(tipo, b, c); this.detail = detalle === undefined ? null : detalle; };
  E.initUIEvent = function (tipo, b, c, vista, detalle) { this.initEvent(tipo, b, c); this.view = vista || null; this.detail = detalle | 0; };
  E.initMouseEvent = function (tipo, b, c, vista, detalle, sx, sy, cx, cy, ctrl, alt, shift, meta, boton, relacionado) {
    this.initUIEvent(tipo, b, c, vista, detalle);
    this.screenX = sx | 0; this.screenY = sy | 0; this.clientX = cx | 0; this.clientY = cy | 0;
    this.ctrlKey = !!ctrl; this.altKey = !!alt; this.shiftKey = !!shift; this.metaKey = !!meta;
    this.button = boton | 0; this.relatedTarget = relacionado || null;
  };
  E.initKeyboardEvent = function (tipo, b, c, vista, tecla) { this.initUIEvent(tipo, b, c, vista, 0); this.key = tecla || ''; };
  __quimera_no_enumerar(E);
  __quimera_no_enumerar(TreeWalker.prototype);
  __quimera_no_enumerar(NodeIterator.prototype);
  __quimera_no_enumerar(Contexto2D.prototype);
  __quimera_no_enumerar(Range.prototype);
})(this, this.document, this.__quimera_protos);"#;
