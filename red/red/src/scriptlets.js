// Los scriptlets de QUIMERA: los de uBlock Origin y Brave que más usan EasyList y los filtros de
// YouTube, escritos de nuevo y en pequeño. Corren antes que los scripts de la página.
var w = window;
function nada() {}

// Si `texto` casa con `aguja`: vacía, siempre; `/…/`, expresión regular; si no, la contiene.
function casa(texto, aguja) {
  if (!aguja) return true;
  texto = String(texto);
  if (aguja.length > 2 && aguja.charAt(0) === '/' && aguja.charAt(aguja.length - 1) === '/') {
    try { return new RegExp(aguja.slice(1, -1)).test(texto); } catch (e) { return false; }
  }
  return texto.indexOf(aguja) >= 0;
}

// Los valores que se dejan poner (como uBlock Origin: nada que la lista pueda usar para atacar).
var NO_VALE = {};
function valorDe(t) {
  switch (t) {
    case 'undefined': return undefined;
    case 'null': return null;
    case 'true': return true;
    case 'false': return false;
    case "''": case '': return '';
    case '[]': case 'emptyArr': return [];
    case '{}': case 'emptyObj': return {};
    case 'noopFunc': return function () {};
    case 'trueFunc': return function () { return true; };
    case 'falseFunc': return function () { return false; };
  }
  if (/^-?\d{1,5}$/.test(t)) return Number(t);
  return NO_VALE;
}

function esObjeto(v) { return v !== null && (typeof v === 'object' || typeof v === 'function'); }

// Pone `desc` en la propiedad final de `ruta` (`a.b.c`) desde `raiz`. Si un trozo del camino
// todavía no existe, espera a que la página lo ponga.
function atrapar(raiz, ruta, desc) {
  var i = ruta.indexOf('.');
  if (i < 0) {
    try { desc.configurable = true; Object.defineProperty(raiz, ruta, desc); } catch (e) {}
    return;
  }
  var cabeza = ruta.slice(0, i), resto = ruta.slice(i + 1), actual = raiz[cabeza];
  if (esObjeto(actual)) { atrapar(actual, resto, desc); return; }
  try {
    Object.defineProperty(raiz, cabeza, {
      configurable: true,
      get: function () { return actual; },
      set: function (v) { actual = v; if (esObjeto(v)) atrapar(v, resto, desc); }
    });
  } catch (e) {}
}

function abortar() { throw new ReferenceError('QUIMERA: bloqueado por un filtro'); }

var S = {};
S['set-constant'] = function (ruta, valor) {
  var v = valorDe(valor === undefined ? 'undefined' : valor);
  if (!ruta || v === NO_VALE) return;
  atrapar(w, ruta, { get: function () { return v; }, set: nada });
};
S['abort-on-property-read'] = function (ruta) {
  if (ruta) atrapar(w, ruta, { get: abortar, set: nada });
};
S['abort-on-property-write'] = function (ruta) {
  if (!ruta) return;
  var v;
  atrapar(w, ruta, { get: function () { return v; }, set: abortar });
};
S['abort-current-script'] = function (ruta, aguja) {
  if (!ruta) return;
  var v = ruta.indexOf('.') < 0 ? w[ruta] : undefined;
  atrapar(w, ruta, {
    get: function () {
      var s = document.currentScript;
      if (!aguja || (s && casa(s.src || s.textContent || '', aguja))) abortar();
      return v;
    },
    set: function (x) { v = x; }
  });
};

// json-prune: quita esas rutas (`a.b`, `[]` para cada elemento de una lista) de lo que lee la
// página con JSON.parse o Response.json. Con `requeridas`, solo si están todas.
function borrar(o, partes) {
  if (!esObjeto(o)) return;
  var k = partes[0], resto = partes.slice(1);
  if (k === '[]') {
    if (o.length !== undefined) for (var i = 0; i < o.length; i++) if (resto.length) borrar(o[i], resto);
    return;
  }
  if (!resto.length) { delete o[k]; return; }
  borrar(o[k], resto);
}
function existe(o, partes) {
  for (var i = 0; i < partes.length; i++) { if (!esObjeto(o) || !(partes[i] in o)) return false; o = o[partes[i]]; }
  return true;
}
var podas = [];
function podar(r) {
  for (var i = 0; i < podas.length; i++) {
    var p = podas[i], ok = true;
    for (var j = 0; j < p.requeridas.length; j++) if (!existe(r, p.requeridas[j])) ok = false;
    if (ok) for (var k = 0; k < p.rutas.length; k++) borrar(r, p.rutas[k]);
  }
  return r;
}
function partir(t) {
  var v = [], l = String(t || '').split(/\s+/);
  for (var i = 0; i < l.length; i++) if (l[i]) v.push(l[i].split('.'));
  return v;
}
S['json-prune'] = function (rutas, requeridas) {
  if (!podas.length) {
    var parse = JSON.parse;
    JSON.parse = function () { return podar(parse.apply(this, arguments)); };
    if (w.Response && w.Response.prototype.json) {
      var json = w.Response.prototype.json;
      w.Response.prototype.json = function () { return json.apply(this, arguments).then(podar); };
    }
  }
  podas.push({ rutas: partir(rutas), requeridas: partir(requeridas) });
};

S['addEventListener-defuser'] = function (tipo, aguja) {
  var dueno = w.EventTarget ? w.EventTarget.prototype : (w.Node ? w.Node.prototype : null);
  var lista = [dueno, w, w.document];
  for (var i = 0; i < lista.length; i++) {
    var d = lista[i];
    if (!d || !d.addEventListener || d.addEventListener['\u0001quimera']) continue;
    (function (d, original) {
      d.addEventListener = function (t, f) {
        if (casa(t, tipo) && casa(f ? f.toString() : '', aguja)) return;
        return original.apply(this, arguments);
      };
      d.addEventListener['\u0001quimera'] = true;
    })(d, d.addEventListener);
  }
};

S['no-fetch-if'] = function (aguja) {
  if (!w.fetch) return;
  var f = w.fetch;
  w.fetch = function (recurso) {
    var url = typeof recurso === 'string' ? recurso : (recurso && recurso.url) || String(recurso);
    if (casa(url, aguja)) return Promise.resolve(new w.Response('', { status: 200, url: url }));
    return f.apply(this, arguments);
  };
};

S['no-xhr-if'] = function (aguja) {
  if (!w.XMLHttpRequest) return;
  var X = w.XMLHttpRequest.prototype, abrir = X.open, enviar = X.send;
  X.open = function (metodo, url) { this['\u0001nada'] = casa(url, aguja); return abrir.apply(this, arguments); };
  X.send = function () {
    if (!this['\u0001nada']) return enviar.apply(this, arguments);
    var x = this;
    setTimeout(function () {
      try { Object.defineProperty(x, 'readyState', { value: 4, configurable: true }); } catch (e) {}
      try { Object.defineProperty(x, 'status', { value: 200, configurable: true }); } catch (e) {}
      try { Object.defineProperty(x, 'responseText', { value: '', configurable: true }); } catch (e) {}
      if (typeof x.onreadystatechange === 'function') x.onreadystatechange();
      if (typeof x.onload === 'function') x.onload();
      if (x.dispatchEvent && w.Event) { x.dispatchEvent(new w.Event('readystatechange')); x.dispatchEvent(new w.Event('load')); }
    }, 0);
  };
};

function sinTemporizador(nombre) {
  return function (aguja, espera) {
    var original = w[nombre];
    if (!original) return;
    w[nombre] = function (f, ms) {
      var texto = typeof f === 'function' ? f.toString() : String(f);
      var no = aguja && aguja.charAt(0) === '!';
      var coincide = casa(texto, no ? aguja.slice(1) : aguja) !== no;
      if (coincide && (!espera || String(ms) === String(espera))) return 0;
      return original.apply(this, arguments);
    };
  };
}
S['no-setTimeout-if'] = sinTemporizador('setTimeout');
S['no-setInterval-if'] = sinTemporizador('setInterval');

S['set-local-storage-item'] = function (clave, valor) {
  try {
    if (valor === '$remove$') { w.localStorage.removeItem(clave); return; }
    var v = valorDe(valor);
    if (v !== NO_VALE && !esObjeto(v)) w.localStorage.setItem(clave, String(v));
  } catch (e) {}
};

S['remove-cookie'] = function (aguja) {
  try {
    var l = String(document.cookie || '').split(';');
    for (var i = 0; i < l.length; i++) {
      var n = l[i].split('=')[0].trim();
      if (n && casa(n, aguja)) document.cookie = n + '=; max-age=0; path=/';
    }
  } catch (e) {}
};

// Los nombres cortos de uBlock Origin.
var otros = {
  set: 'set-constant', aopr: 'abort-on-property-read', aopw: 'abort-on-property-write',
  acs: 'abort-current-script', acis: 'abort-current-script', 'abort-current-inline-script': 'abort-current-script',
  aeld: 'addEventListener-defuser', 'prevent-addEventListener': 'addEventListener-defuser',
  'prevent-fetch': 'no-fetch-if', 'prevent-xhr': 'no-xhr-if',
  nostif: 'no-setTimeout-if', 'prevent-setTimeout': 'no-setTimeout-if',
  nosiif: 'no-setInterval-if', 'prevent-setInterval': 'no-setInterval-if',
  'cookie-remover': 'remove-cookie'
};

// La llamada de una regla: `q('set', ['a.b', 'undefined'])`.
function q(nombre, args) {
  nombre = nombre.replace(/\.js$/, '');
  var f = S[otros[nombre] || nombre];
  if (f) { try { f.apply(null, args); } catch (e) {} }
}
