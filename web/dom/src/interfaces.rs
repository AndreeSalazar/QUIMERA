//! Las clases del DOM (`Node`, `Element`, `HTMLElement`, `HTMLInputElement`...) y los elementos
//! personalizados (`customElements`), en JavaScript sobre los prototipos que crea `puente.rs`.
//!
//! - **Constructores**: `el instanceof HTMLElement`, `class X extends HTMLElement`. Llamarlos
//!   directamente da «Illegal constructor», salvo los que se pueden crear (`Event`, `Text`,
//!   `DocumentFragment`) y `HTMLElement` desde una clase registrada.
//! - **Por etiqueta**: cada etiqueta conocida tiene su interfaz (`input` → `HTMLInputElement`), y
//!   `envolver` le pone su prototipo al crear el objeto del nodo.
//! - **`customElements.define`**: los elementos que ya existen se mejoran (se les pone el
//!   prototipo de la clase y se llama a su constructor con `this` = el elemento); los que se
//!   crean después, al crearlos. `connectedCallback`/`disconnectedCallback` al meterlos y
//!   sacarlos del documento, `attributeChangedCallback` con `setAttribute` y compañía.
//! - **Shadow DOM, a medias**: `attachShadow` da una raíz (un fragmento) donde se puede escribir,
//!   pero todavía no se dibuja: se ven los hijos normales del elemento.

/// El código (se ejecuta al crear cada página, tras los prototipos).
pub(crate) const INTERFACES_JS: &str = r#"(function (w, d, P) {
  function ilegal(nombre) { return function () { throw new TypeError('Illegal constructor: ' + nombre); }; }
  function interfaz(nombre, proto, ctor) {
    var C = ctor || ilegal(nombre);
    C.prototype = proto;
    proto.constructor = C;
    proto[Symbol.toStringTag] = nombre;
    w[nombre] = C;
    return C;
  }
  function esNodo(x) { return x != null && typeof x.nodeType === 'number'; }

  var protos = [];
  function hijo(padre) { var p = Object.create(padre); protos.push(p); return p; }

  // `new EventTarget()` y `class X extends EventTarget` valen: para lo que no es un nodo, los
  // listeners se guardan aquí en JavaScript (los nodos usan los del DOM).
  interfaz('EventTarget', P.p_destino, function EventTarget() {});
  ['addEventListener', 'removeEventListener', 'dispatchEvent'].forEach(function (m) {
    var nativo = P.p_destino[m];
    P.p_destino[m] = function (tipo, f) {
      if (esNodo(this) || this === w) return nativo.apply(this, arguments);
      var oyentes = this['\u0001oyentes'] || (this['\u0001oyentes'] = {});
      if (m === 'addEventListener') {
        var l = oyentes[tipo] || (oyentes[tipo] = []);
        if (typeof f === 'function' && l.indexOf(f) < 0) l.push(f);
        return undefined;
      }
      if (m === 'removeEventListener') {
        var lista = oyentes[tipo] || [], i = lista.indexOf(f);
        if (i >= 0) lista.splice(i, 1);
        return undefined;
      }
      var ev = tipo;
      ev.target = ev.currentTarget = this;
      var h = this['on' + ev.type];
      if (typeof h === 'function') h.call(this, ev);
      (oyentes[ev.type] || []).slice().forEach(function (g) { g.call(this, ev); }, this);
      return !ev.defaultPrevented;
    };
  });
  var Node = interfaz('Node', P.p_nodo);
  interfaz('CharacterData', P.p_caracteres);
  w.Text = function Text(t) { return d.createTextNode(t === undefined ? '' : String(t)); };
  w.Text.prototype = P.p_caracteres;
  w.Comment = function Comment(t) { return d.createComment(t === undefined ? '' : String(t)); };
  w.Comment.prototype = P.p_caracteres;
  // Las que en HTML casi no aparecen, pero existen (el polyfill de Web Components las recorre).
  w.CDATASection = function CDATASection() { throw new TypeError('Illegal constructor: CDATASection'); };
  w.CDATASection.prototype = Object.create(P.p_caracteres);
  w.ProcessingInstruction = function ProcessingInstruction() { throw new TypeError('Illegal constructor: ProcessingInstruction'); };
  w.ProcessingInstruction.prototype = Object.create(P.p_caracteres);
  interfaz('Element', P.p_elemento);
  interfaz('SVGElement', P.p_svg);
  interfaz('DocumentFragment', P.p_fragmento, function DocumentFragment() { return d.createDocumentFragment(); });
  interfaz('Document', P.p_documento);
  w.HTMLDocument = w.Document;
  var constantes = { ELEMENT_NODE: 1, ATTRIBUTE_NODE: 2, TEXT_NODE: 3, CDATA_SECTION_NODE: 4, PROCESSING_INSTRUCTION_NODE: 7,
    COMMENT_NODE: 8, DOCUMENT_NODE: 9, DOCUMENT_TYPE_NODE: 10, DOCUMENT_FRAGMENT_NODE: 11,
    DOCUMENT_POSITION_DISCONNECTED: 1, DOCUMENT_POSITION_PRECEDING: 2, DOCUMENT_POSITION_FOLLOWING: 4,
    DOCUMENT_POSITION_CONTAINS: 8, DOCUMENT_POSITION_CONTAINED_BY: 16, DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC: 32 };
  Object.keys(constantes).forEach(function (k) { Node[k] = constantes[k]; P.p_nodo[k] = constantes[k]; });

  // ---------------- elementos personalizados ----------------
  var registro = new Map(), porClase = new Map(), esperas = {};
  function observados(C) { return Array.isArray(C.observedAttributes) ? C.observedAttributes : []; }
  function avisar(e) {
    var pila = e && typeof e.stack === 'string' ? e.stack : '', i = pila.indexOf('\n    at ');
    console.error('Error en un elemento personalizado: ' + String(e && e.message || e) + (i < 0 ? '' : pila.slice(i)));
  }
  // Los elementos que se están mejorando: si su constructor crea el objeto con `new` o
  // `Reflect.construct` (clases compiladas), `HTMLElement()` devuelve el elemento, como en los
  // navegadores.
  var enMejora = [];
  function mejorar(el, C) {
    if (el['\u0001mejorado']) return;
    el['\u0001mejorado'] = true;
    Object.setPrototypeOf(el, C.prototype);
    enMejora.push(el);
    try { C.call(el); } catch (e) { avisar(e); }
    enMejora.pop();
    var cambio = el.attributeChangedCallback;
    if (typeof cambio === 'function') {
      observados(C).forEach(function (a) { if (el.hasAttribute(a)) { try { cambio.call(el, a, null, el.getAttribute(a)); } catch (e) { avisar(e); } } });
    }
    if (el.isConnected) conectarUno(el);
  }
  function conectarUno(el) {
    if (el['\u0001conectado']) return;
    el['\u0001conectado'] = true;
    if (typeof el.connectedCallback === 'function') { try { el.connectedCallback(); } catch (e) { avisar(e); } }
  }
  // Lo que está dentro de un `<template>` (o en su `content`) es inerte: ni se mejora ni se
  // conecta, como en los navegadores (su documento no tiene registro de elementos).
  function enPlantilla(n) {
    for (var p = n.parentNode; p; p = p.parentNode) {
      if (p['\u0001inerte'] || p.localName === 'template') return true;
    }
    return false;
  }
  function cadaElemento(n, f) {
    if (!esNodo(n) || n['\u0001inerte'] || enPlantilla(n)) return;
    if (n.nodeType === 1) f(n);
    if (n.nodeType !== 1 && n.nodeType !== 11 && n.nodeType !== 9) return;
    if (n.localName === 'template') return;
    var todos = n.querySelectorAll('*'), plantillas = n.querySelectorAll('template');
    todos.forEach(plantillas.length ? function (el) {
      for (var i = 0; i < plantillas.length; i++) if (plantillas[i] !== el && plantillas[i].contains(el)) return;
      f(el);
    } : f);
  }
  // Tras meter `n` en el árbol: mejorar lo registrado y avisar a lo que entra en el documento.
  function conectar(n) {
    if (!registro.size) return;
    cadaElemento(n, function (el) {
      var C = registro.get(el.localName);
      if (C && !el['\u0001mejorado']) mejorar(el, C);
      else if (el['\u0001mejorado'] && el.isConnected) conectarUno(el);
    });
  }
  function desconectar(n) {
    if (!registro.size || !esNodo(n) || !n.isConnected) return;
    cadaElemento(n, function (el) {
      if (el['\u0001conectado']) {
        el['\u0001conectado'] = false;
        if (typeof el.disconnectedCallback === 'function') { try { el.disconnectedCallback(); } catch (e) { avisar(e); } }
      }
    });
  }
  w.__quimera_tras_html = function (el) { if (registro.size) for (var i = 0; i < el.childNodes.length; i++) conectar(el.childNodes[i]); };

  var crearNativo = P.p_documento.createElement;
  P.p_documento.createElement = function (t) {
    var el = crearNativo.call(this, t);
    var C = registro.get(String(t).toLowerCase());
    if (C) mejorar(el, C);
    return el;
  };
  P.p_documento.createElementNS = function (ns, t) { return this.createElement(String(t).replace(/^.*:/, '')); };
  P.p_documento.createEvent = function () { return new Event(''); };
  // La copia nace en el documento: lo registrado se mejora ya (sin conectar, aún no está dentro).
  P.p_documento.importNode = function (n, profundo) {
    var c = n.cloneNode(!!profundo);
    if (registro.size) cadaElemento(c, function (el) { var C = registro.get(el.localName); if (C) mejorar(el, C); });
    return c;
  };
  P.p_documento.adoptNode = function (n) { if (n.parentNode) n.parentNode.removeChild(n); return n; };

  w.customElements = {
    define: function (nombre, C) {
      nombre = String(nombre);
      if (!/^[a-z][a-z0-9._]*-[a-z0-9._-]*$/.test(nombre)) throw new SyntaxError('"' + nombre + '" no es un nombre válido de elemento personalizado');
      if (registro.has(nombre)) throw new Error('el elemento "' + nombre + '" ya está definido');
      registro.set(nombre, C);
      porClase.set(C, nombre);
      d.querySelectorAll(nombre).forEach(function (el) { if (!enPlantilla(el)) mejorar(el, C); });
      if (esperas[nombre]) { esperas[nombre].resolver(C); delete esperas[nombre]; }
    },
    get: function (nombre) { return registro.get(String(nombre)); },
    getName: function (C) { return porClase.get(C) || null; },
    whenDefined: function (nombre) {
      nombre = String(nombre);
      if (registro.has(nombre)) return Promise.resolve(registro.get(nombre));
      if (!esperas[nombre]) {
        var e = {};
        e.promesa = new Promise(function (r) { e.resolver = r; });
        esperas[nombre] = e;
      }
      return esperas[nombre].promesa;
    },
    upgrade: function (raiz) { conectar(raiz); }
  };

  // `HTMLElement`: desde una clase registrada (`new MiElemento()`) crea el elemento; al mejorar
  // uno que ya existe (`this` es ese elemento), no hace nada.
  interfaz('HTMLElement', P.p_html, function HTMLElement() {
    if (esNodo(this)) return undefined;
    var arriba = enMejora[enMejora.length - 1];
    if (arriba && Object.getPrototypeOf(this) === Object.getPrototypeOf(arriba)) return arriba;
    var C = this.constructor, nombre = porClase.get(C);
    if (!nombre) throw new TypeError('Illegal constructor: HTMLElement (la clase no está registrada con customElements.define)');
    var el = crearNativo.call(d, nombre);
    el['\u0001mejorado'] = true;
    return el;
  });

  // ---------------- interfaces por etiqueta ----------------
  var porEtiqueta = {
    HTMLDivElement: ['div'], HTMLSpanElement: ['span'], HTMLParagraphElement: ['p'], HTMLAnchorElement: ['a'],
    HTMLImageElement: ['img'], HTMLInputElement: ['input'], HTMLButtonElement: ['button'], HTMLFormElement: ['form'],
    HTMLSelectElement: ['select'], HTMLOptionElement: ['option'], HTMLTextAreaElement: ['textarea'],
    HTMLLabelElement: ['label'], HTMLUListElement: ['ul'], HTMLOListElement: ['ol'], HTMLLIElement: ['li'],
    HTMLTableElement: ['table'], HTMLTableRowElement: ['tr'], HTMLTableCellElement: ['td', 'th'],
    HTMLTableSectionElement: ['thead', 'tbody', 'tfoot'], HTMLHeadingElement: ['h1', 'h2', 'h3', 'h4', 'h5', 'h6'],
    HTMLScriptElement: ['script'], HTMLStyleElement: ['style'], HTMLLinkElement: ['link'], HTMLMetaElement: ['meta'],
    HTMLTemplateElement: ['template'], HTMLIFrameElement: ['iframe'], HTMLCanvasElement: ['canvas'],
    HTMLSlotElement: ['slot'], HTMLBodyElement: ['body'], HTMLHeadElement: ['head'], HTMLHtmlElement: ['html'],
    HTMLBRElement: ['br'], HTMLHRElement: ['hr'], HTMLPreElement: ['pre'], HTMLDialogElement: ['dialog'],
    HTMLDetailsElement: ['details'], HTMLTitleElement: ['title'], HTMLQuoteElement: ['blockquote', 'q'],
    HTMLFieldSetElement: ['fieldset'], HTMLLegendElement: ['legend'], HTMLProgressElement: ['progress'],
    HTMLSourceElement: ['source'], HTMLPictureElement: ['picture'], HTMLDataListElement: ['datalist']
  };
  Object.keys(porEtiqueta).forEach(function (nombre) {
    var p = hijo(P.p_html);
    interfaz(nombre, p);
    porEtiqueta[nombre].forEach(function (t) { P.p_etiquetas[t] = p; });
  });
  var media = hijo(P.p_html);
  interfaz('HTMLMediaElement', media);
  ['HTMLVideoElement:video', 'HTMLAudioElement:audio'].forEach(function (par) {
    var a = par.split(':'), p = hijo(media);
    interfaz(a[0], p);
    P.p_etiquetas[a[1]] = p;
  });
  interfaz('HTMLUnknownElement', hijo(P.p_html));
  var ShadowRoot = interfaz('ShadowRoot', hijo(P.p_fragmento));

  // `template.content`: un fragmento con lo que había dentro (se mueve la primera vez).
  Object.defineProperty(w.HTMLTemplateElement.prototype, 'content', {
    get: function () {
      if (!this['\u0001contenido']) {
        var f = d.createDocumentFragment();
        f['\u0001inerte'] = true;
        while (this.firstChild) f.appendChild(this.firstChild);
        this['\u0001contenido'] = f;
      }
      return this['\u0001contenido'];
    }
  });

  // ---------------- eventos ----------------
  function Event(tipo, op) {
    if (!(this instanceof Event)) throw new TypeError("Failed to construct 'Event': use new");
    op = op || {};
    this.type = String(tipo);
    this.bubbles = !!op.bubbles;
    this.cancelable = !!op.cancelable;
    this.composed = !!op.composed;
    this.defaultPrevented = false;
    this.isTrusted = false;
    this.timeStamp = performance.now();
    this.target = null;
    this.currentTarget = null;
  }
  interfaz('Event', P.p_evento, Event);
  P.p_evento.stopImmediatePropagation = P.p_evento.stopPropagation;
  P.p_evento.composedPath = function () { var r = [], n = this.target; while (n) { r.push(n); n = n.parentNode; } r.push(w); return r; };
  P.p_evento.initEvent = function (tipo, b, c) { this.type = String(tipo); this.bubbles = !!b; this.cancelable = !!c; };
  function subclase(nombre, campos) {
    var C = function (tipo, op) {
      Event.call(this, tipo, op);
      op = op || {};
      for (var k in campos) this[k] = op[k] !== undefined ? op[k] : campos[k];
    };
    C.prototype = hijo(P.p_evento);
    C.prototype.constructor = C;
    w[nombre] = C;
  }
  subclase('CustomEvent', { detail: null });
  subclase('UIEvent', { detail: 0, view: null });
  subclase('MouseEvent', { clientX: 0, clientY: 0, button: 0, buttons: 0, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false });
  subclase('PointerEvent', { clientX: 0, clientY: 0, pointerId: 1, pointerType: 'mouse', button: 0 });
  subclase('KeyboardEvent', { key: '', code: '', ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, repeat: false });
  subclase('FocusEvent', { relatedTarget: null });
  subclase('InputEvent', { data: null, inputType: '' });
  subclase('MessageEvent', { data: null, origin: '', lastEventId: '', source: null, ports: [] });
  // El resto de clases de eventos que nombran las aplicaciones al arrancar (YouTube...).
  subclase('WheelEvent', { deltaX: 0, deltaY: 0, deltaZ: 0, deltaMode: 0, clientX: 0, clientY: 0, ctrlKey: false, shiftKey: false });
  subclase('DragEvent', { dataTransfer: null, clientX: 0, clientY: 0 });
  subclase('TouchEvent', { touches: [], targetTouches: [], changedTouches: [] });
  subclase('AnimationEvent', { animationName: '', elapsedTime: 0, pseudoElement: '' });
  subclase('TransitionEvent', { propertyName: '', elapsedTime: 0, pseudoElement: '' });
  subclase('ProgressEvent', { lengthComputable: false, loaded: 0, total: 0 });
  subclase('PopStateEvent', { state: null });
  subclase('HashChangeEvent', { oldURL: '', newURL: '' });
  subclase('PageTransitionEvent', { persisted: false });
  subclase('StorageEvent', { key: null, oldValue: null, newValue: null, url: '', storageArea: null });
  subclase('ErrorEvent', { message: '', filename: '', lineno: 0, colno: 0, error: null });
  subclase('PromiseRejectionEvent', { promise: null, reason: undefined });
  subclase('SubmitEvent', { submitter: null });
  subclase('CompositionEvent', { data: '' });
  subclase('ClipboardEvent', { clipboardData: null });
  subclase('BeforeUnloadEvent', { returnValue: '' });
  subclase('ToggleEvent', { oldState: '', newState: '' });
  subclase('SecurityPolicyViolationEvent', { violatedDirective: '', blockedURI: '' });
  // `window.postMessage`: el evento `message` llega a la propia ventana en otra tarea (sin
  // marcos ni otras ventanas aún; el dato no se copia, se pasa tal cual).
  w.postMessage = function (dato) {
    setTimeout(function () {
      w.dispatchEvent(new MessageEvent('message', { data: dato, origin: location.origin, source: w }));
    }, 0);
  };

  // ---------------- más de Node y Element ----------------
  var N = P.p_nodo, E = P.p_elemento;
  // Las propiedades del DOM, como accesores en los prototipos (como los navegadores): hay
  // librerías que copian sus descriptores (ShadyDOM mira `Node.prototype.firstChild`).
  function accesores(proto, lectura, escritura) {
    lectura.concat(escritura).forEach(function (k) {
      var d = { get: __quimera_accesor_anfitrion(k, false), configurable: true, enumerable: true };
      if (escritura.indexOf(k) >= 0) d.set = __quimera_accesor_anfitrion(k, true);
      Object.defineProperty(proto, k, d);
    });
  }
  var hijos = ['children', 'childElementCount', 'firstElementChild', 'lastElementChild'];
  accesores(N, ['parentNode', 'parentElement', 'firstChild', 'lastChild', 'childNodes', 'nextSibling',
    'previousSibling', 'ownerDocument', 'isConnected', 'nodeType', 'nodeName'], ['textContent', 'nodeValue']);
  accesores(E, hijos.concat(['nextElementSibling', 'previousElementSibling', 'outerHTML', 'localName', 'tagName']),
    ['innerHTML', 'id', 'className']);
  accesores(P.p_fragmento, hijos, []);
  accesores(P.p_documento, hijos, []);
  // El `ownerDocument` de lo que está en una plantilla es un documento inerte: lo que crea no
  // se mejora (Polymer convierte `<template is="dom-repeat">` con `t.ownerDocument.createElement`).
  var docInerte = Object.create(d), propietario = Object.getOwnPropertyDescriptor(N, 'ownerDocument').get;
  docInerte.createElement = function (t) { return crearNativo.call(d, t); };
  docInerte.createElementNS = function (ns, t) { return crearNativo.call(d, String(t).replace(/^.*:/, '')); };
  ['createTextNode', 'createComment', 'createDocumentFragment', 'createTreeWalker', 'createNodeIterator',
    'getElementById', 'querySelector', 'querySelectorAll'].forEach(function (k) {
    docInerte[k] = function () { return d[k].apply(d, arguments); };
  });
  docInerte.importNode = function (n, profundo) { return n.cloneNode(!!profundo); };
  Object.defineProperty(N, 'ownerDocument', {
    get: function () { return this['inerte'] || enPlantilla(this) ? docInerte : propietario.call(this); },
    configurable: true, enumerable: true
  });
  // Meter y sacar nodos avisa a los elementos personalizados.
  ['appendChild', 'insertBefore', 'replaceChild', 'append'].forEach(function (m) {
    var nativo = N[m];
    N[m] = function () {
      if (m === 'replaceChild') desconectar(arguments[1]);
      // Un fragmento se vacía al meterlo: lo que hay que conectar son sus hijos.
      var entran = [], lista = m === 'append' ? arguments : [arguments[0]];
      for (var i = 0; i < lista.length; i++) {
        var x = lista[i];
        if (esNodo(x) && x.nodeType === 11) { for (var c = x.firstChild; c; c = c.nextSibling) entran.push(c); } else entran.push(x);
      }
      var r = nativo.apply(this, arguments);
      for (var j = 0; j < entran.length; j++) conectar(entran[j]);
      return r;
    };
  });
  ['removeChild', 'remove'].forEach(function (m) {
    var nativo = N[m];
    N[m] = function (x) { desconectar(m === 'remove' ? this : x); return nativo.apply(this, arguments); };
  });
  // Copiar en profundidad copia también el `content` de cada plantilla (ya separado del árbol).
  var clonarNativo = N.cloneNode;
  N.cloneNode = function (profundo) {
    var c = clonarNativo.call(this, profundo);
    if (!profundo || !this.querySelectorAll) return c;
    var de = this.localName === 'template' ? [this] : [], a = c.localName === 'template' ? [c] : [];
    de = de.concat(Array.from(this.querySelectorAll('template')));
    a = a.concat(Array.from(c.querySelectorAll('template')));
    for (var i = 0; i < de.length && i < a.length; i++) {
      var f = de[i]['\u0001contenido'];
      if (f) a[i].content.appendChild(f.cloneNode(true));
    }
    return c;
  };
  function nodoDe(x) { return esNodo(x) ? x : d.createTextNode(String(x)); }
  N.prepend = function () { var primero = this.firstChild; for (var i = 0; i < arguments.length; i++) this.insertBefore(nodoDe(arguments[i]), primero); };
  N.before = function () { var p = this.parentNode; if (p) for (var i = 0; i < arguments.length; i++) p.insertBefore(nodoDe(arguments[i]), this); };
  N.after = function () { var p = this.parentNode, sig = this.nextSibling; if (p) for (var i = 0; i < arguments.length; i++) p.insertBefore(nodoDe(arguments[i]), sig); };
  N.replaceWith = function () { var p = this.parentNode; if (!p) return; var sig = this.nextSibling; p.removeChild(this); for (var i = 0; i < arguments.length; i++) p.insertBefore(nodoDe(arguments[i]), sig); };
  N.replaceChildren = function () { while (this.firstChild) this.removeChild(this.firstChild); this.append.apply(this, arguments); };
  N.isEqualNode = function (o) { return esNodo(o) && this.nodeType === o.nodeType && (this.outerHTML || this.textContent) === (o.outerHTML || o.textContent); };
  N.isSameNode = function (o) { return this === o; };
  N.getRootNode = function () { var n = this; while (n.parentNode) n = n.parentNode; return n; };
  N.normalize = function () {};
  N.compareDocumentPosition = function (o) {
    if (this === o) return 0;
    if (this.contains(o)) return 20;
    if (o.contains(this)) return 10;
    function cadena(n) { var c = []; while (n) { c.unshift(n); n = n.parentNode; } return c; }
    var a = cadena(this), b = cadena(o);
    if (a[0] !== b[0]) return 1 | 32 | 2;
    var i = 0;
    while (a[i] === b[i]) i++;
    var hermanos = Array.prototype.slice.call(a[i - 1].childNodes);
    return hermanos.indexOf(a[i]) < hermanos.indexOf(b[i]) ? 4 : 2;
  };
  var poner = E.setAttribute, quitar = E.removeAttribute;
  function avisarAtributo(el, nombre, viejo) {
    var f = el.attributeChangedCallback, C = el.constructor;
    if (typeof f === 'function' && el['\u0001mejorado'] && observados(C).indexOf(nombre) >= 0) {
      try { f.call(el, nombre, viejo, el.getAttribute(nombre)); } catch (e) { avisar(e); }
    }
  }
  E.setAttribute = function (n, v) { n = String(n).toLowerCase(); var viejo = this.getAttribute(n); poner.call(this, n, v); avisarAtributo(this, n, viejo); };
  E.removeAttribute = function (n) { n = String(n).toLowerCase(); var viejo = this.getAttribute(n); quitar.call(this, n); avisarAtributo(this, n, viejo); };
  E.toggleAttribute = function (n, forzar) {
    var hay = this.hasAttribute(n), poner2 = forzar === undefined ? !hay : !!forzar;
    if (poner2 && !hay) this.setAttribute(n, ''); else if (!poner2 && hay) this.removeAttribute(n);
    return poner2;
  };
  E.getAttributeNames = function () {
    var r = [], m = /<[^\s>]+((?:\s+[^\s=>]+(?:="[^"]*")?)*)/.exec(this.outerHTML || '');
    (m && m[1] || '').replace(/\s+([^\s=>]+)(?:="[^"]*")?/g, function (_, n) { r.push(n); });
    return r;
  };
  E.hasAttributes = function () { return this.getAttributeNames().length > 0; };
  Object.defineProperty(E, 'attributes', {
    get: function () {
      var el = this;
      // Un `NamedNodeMap` (una copia, no vivo) con nodos `Attr` sencillos.
      var lista = this.getAttributeNames().map(function (n) {
        return { name: n, localName: n, nodeName: n, value: el.getAttribute(n), nodeValue: el.getAttribute(n),
          nodeType: 2, specified: true, ownerElement: el, namespaceURI: null, prefix: null };
      });
      lista.item = function (i) { return this[i] || null; };
      lista.getNamedItem = function (n) {
        n = String(n).toLowerCase();
        for (var i = 0; i < this.length; i++) if (this[i].name === n) return this[i];
        return null;
      };
      lista.getNamedItemNS = function (ns, n) { return this.getNamedItem(n); };
      lista.removeNamedItem = function (n) { var a = this.getNamedItem(n); if (a) el.removeAttribute(a.name); return a; };
      lista.setNamedItem = function (a) { var v = this.getNamedItem(a.name); el.setAttribute(a.name, a.value); return v; };
      return lista;
    }
  });
  E.insertAdjacentHTML = function (donde, html) {
    var f = d.createElement('template');
    f.innerHTML = html;
    var c = f.content;
    var lugar = String(donde).toLowerCase();
    if (lugar === 'beforebegin') this.before(c);
    else if (lugar === 'afterbegin') this.insertBefore(c, this.firstChild);
    else if (lugar === 'beforeend') this.appendChild(c);
    else if (lugar === 'afterend') this.after(c);
  };
  E.insertAdjacentElement = function (donde, el) {
    var lugar = String(donde).toLowerCase();
    if (lugar === 'beforebegin') this.before(el); else if (lugar === 'afterbegin') this.insertBefore(el, this.firstChild);
    else if (lugar === 'beforeend') this.appendChild(el); else if (lugar === 'afterend') this.after(el);
    return el;
  };
  E.insertAdjacentText = function (donde, t) { this.insertAdjacentElement(donde, d.createTextNode(String(t))); };
  E.scrollIntoView = E.scrollTo = E.scrollBy = function () {};
  // Sin medidas de la maquetación todavía: un rectángulo vacío.
  // La caja de la maquetación (sin desplazamiento: la página está arriba del todo).
  function rect(x, y, w, h) {
    return { x: x, y: y, left: x, top: y, width: w, height: h, right: x + w, bottom: y + h,
      toJSON: function () { return { x: x, y: y, width: w, height: h, top: y, left: x, right: x + w, bottom: y + h }; } };
  }
  w.DOMRect = function DOMRect(x, y, ancho, alto) { return rect(x || 0, y || 0, ancho || 0, alto || 0); };
  E.getBoundingClientRect = function () { return rect(this.__quimera_x, this.__quimera_y, this.__quimera_ancho, this.__quimera_alto); };
  E.getClientRects = function () { var r = this.getBoundingClientRect(); return r.width || r.height ? [r] : []; };
  E.animate = function () { return { finished: Promise.resolve(), cancel: function () {}, play: function () {}, pause: function () {}, onfinish: null }; };
  E.attachShadow = function (op) {
    if (this['\u0001sombra']) throw new Error('el elemento ya tiene una raíz de sombra');
    var r = d.createDocumentFragment();
    Object.setPrototypeOf(r, ShadowRoot.prototype);
    r.host = this;
    r.mode = op && op.mode || 'open';
    r.adoptedStyleSheets = [];
    this['\u0001sombra'] = r;
    return r;
  };
  Object.defineProperty(E, 'shadowRoot', {
    get: function () { var r = this['\u0001sombra']; return r && r.mode === 'open' ? r : null; }
  });
  w.CSSStyleSheet = function CSSStyleSheet() { this.cssRules = []; };
  w.CSSStyleSheet.prototype.replaceSync = function (t) { this['\u0001texto'] = String(t); };
  w.CSSStyleSheet.prototype.replace = function (t) { this.replaceSync(t); return Promise.resolve(this); };
  w.CSSStyleSheet.prototype.insertRule = function () { return 0; };
  Object.defineProperty(P.p_documento, 'domain', { get: function () { return location.hostname; } });
  // `href` y `src` dan la URL absoluta, como en los navegadores (el atributo guarda lo escrito).
  function absoluta(atributo) {
    return {
      get: function () {
        var v = this.getAttribute(atributo);
        if (v === null) return '';
        try { return new URL(v, location.href).href; } catch (e) { return v; }
      },
      set: function (v) { this.setAttribute(atributo, v); }
    };
  }
  [['HTMLAnchorElement', 'href'], ['HTMLAreaElement', 'href'], ['HTMLLinkElement', 'href'],
    ['HTMLImageElement', 'src'], ['HTMLScriptElement', 'src'], ['HTMLIFrameElement', 'src'],
    ['HTMLMediaElement', 'src'], ['HTMLSourceElement', 'src'], ['HTMLEmbedElement', 'src']].forEach(function (par) {
    var I = w[par[0]];
    if (I) Object.defineProperty(I.prototype, par[1], absoluta(par[1]));
  });
  // `<a>` y `<area>` se descomponen como una URL (`a.pathname`, `a.host`...): lo usan axios y
  // muchas librerías para resolver direcciones.
  ['protocol', 'username', 'password', 'host', 'hostname', 'port', 'pathname', 'search', 'hash', 'origin'].forEach(function (k) {
    var d = {
      get: function () {
        var h = this.href;
        if (!h) return '';
        try { return new URL(h)[k]; } catch (e) { return k === 'origin' ? 'null' : ''; }
      }
    };
    if (k !== 'origin') {
      d.set = function (v) {
        try { var u = new URL(this.href); u[k] = String(v); this.setAttribute('href', u.href); } catch (e) {}
      };
    }
    Object.defineProperty(w.HTMLAnchorElement.prototype, k, d);
    if (w.HTMLAreaElement) Object.defineProperty(w.HTMLAreaElement.prototype, k, d);
  });
  Object.defineProperty(w.HTMLImageElement.prototype, 'currentSrc', { get: function () { return this.src; } });
  ['srcset', 'sizes'].forEach(function (a) {
    Object.defineProperty(w.HTMLImageElement.prototype, a, {
      get: function () { return this.getAttribute(a) || ''; },
      set: function (v) { this.setAttribute(a, v); }
    });
  });
  // `adoptedStyleSheets` se guarda (para que la página lo lea y lo amplíe), pero no se aplica.
  [P.p_documento, ShadowRoot.prototype].forEach(function (p) {
    Object.defineProperty(p, 'adoptedStyleSheets', {
      get: function () { return this['\u0001adoptadas'] || (this['\u0001adoptadas'] = []); },
      set: function (v) { this['\u0001adoptadas'] = Array.prototype.slice.call(v); }
    });
  });

  // ---------------- DOMException, AbortController ----------------
  function DOMException(mensaje, nombre) {
    this.message = mensaje === undefined ? '' : String(mensaje);
    this.name = nombre === undefined ? 'Error' : String(nombre);
    this.code = 0;
  }
  DOMException.prototype = Object.create(Error.prototype);
  DOMException.prototype.constructor = DOMException;
  w.DOMException = DOMException;
  class AbortSignal extends w.EventTarget {
    constructor() { super(); this.aborted = false; this.reason = undefined; this.onabort = null; }
    throwIfAborted() { if (this.aborted) throw this.reason; }
    static abort(motivo) { const c = new AbortController(); c.abort(motivo); return c.signal; }
    static timeout(ms) { const c = new AbortController(); setTimeout(() => c.abort(new DOMException('signal timed out', 'TimeoutError')), ms); return c.signal; }
    static any(senales) {
      const c = new AbortController();
      for (const s of senales) {
        if (s.aborted) { c.abort(s.reason); break; }
        s.addEventListener('abort', () => c.abort(s.reason));
      }
      return c.signal;
    }
  }
  class AbortController {
    constructor() { this.signal = new AbortSignal(); }
    abort(motivo) {
      const s = this.signal;
      if (s.aborted) return;
      s.aborted = true;
      s.reason = motivo === undefined ? new DOMException('signal is aborted without reason', 'AbortError') : motivo;
      s.dispatchEvent(new Event('abort'));
    }
  }
  w.AbortSignal = AbortSignal;
  w.AbortController = AbortController;

  // Los métodos de los prototipos no salen en for-in (como en los navegadores).
  [P.p_destino, P.p_nodo, P.p_caracteres, P.p_elemento, P.p_html, P.p_svg, P.p_fragmento, P.p_documento, P.p_evento]
    .concat(protos).forEach(function (p) { __quimera_no_enumerar(p); });
})(this, this.document, this.__quimera_protos);"#;
