//! La red desde JavaScript: `fetch`, `XMLHttpRequest` y los `<script src>` que añaden los
//! scripts.
//!
//! El DOM no tiene red (no sabe de hilos ni de sockets): apunta lo que pide la página en una
//! cola ([`Dom::tomar_peticiones`]) y quien muestra la página la atiende y devuelve la respuesta
//! ([`Dom::responder`]). Las reglas de origen (qué lleva cookies, CORS) las aplica ese anfitrión,
//! que es quien sabe de URLs.
//!
//! `fetch`, `Response`, `Headers` y `XMLHttpRequest` están escritos en JavaScript (abajo) sobre
//! una sola función del anfitrión, `__quimera_pedir`.

use crate::{Dom, Estado, microtareas};
use alloc::string::String;
use alloc::vec::Vec;
use quimera_html::{Documento, NodoId};
use quimera_vm::{Motor, Valor};

/// Algo que la página pide a la red.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeticionJs {
    /// Número para devolver la respuesta.
    pub id: u32,
    /// La dirección tal como la escribió la página (sin resolver).
    pub url: String,
    /// `GET`, `POST`...
    pub metodo: String,
    /// El cuerpo, si lo hay.
    pub cuerpo: Option<String>,
    /// Cabeceras que pide la página.
    pub cabeceras: Vec<(String, String)>,
    /// `same-origin` (lo normal), `include` u `omit`: cuándo van las cookies.
    pub credenciales: String,
    /// Es un `<script src>` (se ejecuta al llegar; no pasa por CORS).
    pub es_script: bool,
}

/// Lo que llegó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespuestaJs {
    /// Código HTTP.
    pub estado: u16,
    /// La URL final (tras las redirecciones).
    pub url: String,
    /// Cabeceras, con el nombre en minúsculas.
    pub cabeceras: Vec<(String, String)>,
    /// El cuerpo como texto.
    pub cuerpo: String,
}

/// El cargador de módulos: pide cada uno con `fetch`, lo compila (`__quimera_modulo`) y lo
/// ejecuta una sola vez con sus exportaciones, su `import` (relativo a él) e `import.meta`. En
/// un ciclo, quien importa recibe el espacio de nombres a medio llenar (como en los navegadores).
/// Solo resuelve rutas (`./`, `../`, `/`, URLs): sin mapas de importación.
pub(crate) const MODULOS_JS: &str = r#"(function (w) {
  var modulos = {};
  function normalizar(ruta) {
    var resto = '', i = ruta.search(/[?#]/);
    if (i >= 0) { resto = ruta.slice(i); ruta = ruta.slice(0, i); }
    var partes = ruta.split('/'), r = [];
    for (var j = 0; j < partes.length; j++) {
      var p = partes[j];
      if (p === '..') { if (r.length > 1) r.pop(); if (j === partes.length - 1) r.push(''); }
      else if (p === '.') { if (j === partes.length - 1) r.push(''); }
      else r.push(p);
    }
    return r.join('/') + resto;
  }
  // Una dirección relativa contra `base` (como `new URL(relativa, base)`).
  function unir(espec, base) {
    espec = String(espec);
    if (/^[a-z][a-z0-9+.-]*:/i.test(espec)) return espec;
    var m = /^([a-z][a-z0-9+.-]*:)(\/\/[^\/?#]*)?([^?#]*)(\?[^#]*)?/i.exec(String(base)) || ['', '', '', '/', ''];
    var esquema = m[1], autoridad = m[2] || '', ruta = m[3] || '/', consulta = m[4] || '';
    if (espec === '') return esquema + autoridad + ruta + consulta;
    if (espec.slice(0, 2) === '//') return esquema + espec;
    if (espec.charAt(0) === '/') return esquema + autoridad + normalizar(espec);
    if (espec.charAt(0) === '?') return esquema + autoridad + ruta + espec;
    if (espec.charAt(0) === '#') return esquema + autoridad + ruta + consulta + espec;
    return esquema + autoridad + normalizar(ruta.replace(/[^\/]*$/, '') + espec);
  }
  // El mapa de importación de la página (`<script type=importmap>`).
  var mapa = { imports: {} };
  w.__quimera_mapa_importaciones = function (texto) {
    try {
      var m = JSON.parse(texto);
      Object.keys(m.imports || {}).forEach(function (k) { mapa.imports[k] = unir(m.imports[k], location.href); });
    } catch (e) { console.error('mapa de importación no válido: ' + e.message); }
  };
  // Un especificador de módulo: una ruta, una URL, o un nombre del mapa de importación
  // (exacto, o por prefijo si acaba en `/`).
  function resolver(espec, base) {
    espec = String(espec);
    if (Object.prototype.hasOwnProperty.call(mapa.imports, espec)) return mapa.imports[espec];
    var mejor = '';
    Object.keys(mapa.imports).forEach(function (k) {
      if (k.charAt(k.length - 1) === '/' && espec.indexOf(k) === 0 && k.length > mejor.length) mejor = k;
    });
    if (mejor) return mapa.imports[mejor] + espec.slice(mejor.length);
    var relativo = /^[a-z][a-z0-9+.-]*:/i.test(espec) || espec.charAt(0) === '/' || espec.slice(0, 2) === './' || espec.slice(0, 3) === '../';
    if (!relativo) throw new TypeError('QUIMERA no resuelve el módulo "' + espec + '" (no está en el mapa de importación)');
    return unir(espec, base);
  }

  // URLSearchParams y URL.
  function decodificar(s) { return decodeURIComponent(String(s).replace(/\+/g, ' ')); }
  function codificar(s) { return encodeURIComponent(String(s)).replace(/%20/g, '+'); }
  class URLSearchParams {
    constructor(inicial) {
      this['\u0001p'] = [];
      this['\u0001url'] = null;
      if (typeof inicial === 'string') {
        inicial.replace(/^\?/, '').split('&').forEach((par) => {
          if (!par) return;
          const i = par.indexOf('=');
          this['\u0001p'].push(i < 0 ? [decodificar(par), ''] : [decodificar(par.slice(0, i)), decodificar(par.slice(i + 1))]);
        });
      } else if (inicial instanceof URLSearchParams) {
        this['\u0001p'] = inicial['\u0001p'].map((p) => p.slice());
      } else if (inicial && typeof inicial === 'object') {
        (Array.isArray(inicial) ? inicial : Object.entries(inicial)).forEach((p) => this['\u0001p'].push([String(p[0]), String(p[1])]));
      }
    }
    ['\u0001cambio']() { if (this['\u0001url']) this['\u0001url']['\u0001search'] = this['\u0001p'].length ? '?' + this.toString() : ''; }
    get(k) { const p = this['\u0001p'].find((x) => x[0] === String(k)); return p ? p[1] : null; }
    getAll(k) { return this['\u0001p'].filter((x) => x[0] === String(k)).map((x) => x[1]); }
    has(k) { return this['\u0001p'].some((x) => x[0] === String(k)); }
    append(k, v) { this['\u0001p'].push([String(k), String(v)]); this['\u0001cambio'](); }
    set(k, v) {
      const i = this['\u0001p'].findIndex((x) => x[0] === String(k));
      this['\u0001p'] = this['\u0001p'].filter((x, j) => x[0] !== String(k) || j === i);
      if (i < 0) this['\u0001p'].push([String(k), String(v)]); else this['\u0001p'][this['\u0001p'].findIndex((x) => x[0] === String(k))][1] = String(v);
      this['\u0001cambio']();
    }
    delete(k) { this['\u0001p'] = this['\u0001p'].filter((x) => x[0] !== String(k)); this['\u0001cambio'](); }
    sort() { this['\u0001p'].sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0)); this['\u0001cambio'](); }
    forEach(f, este) { this['\u0001p'].forEach((p) => f.call(este, p[1], p[0], this)); }
    keys() { return this['\u0001p'].map((p) => p[0])[Symbol.iterator](); }
    values() { return this['\u0001p'].map((p) => p[1])[Symbol.iterator](); }
    entries() { return this['\u0001p'].map((p) => p.slice())[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
    get size() { return this['\u0001p'].length; }
    toString() { return this['\u0001p'].map((p) => codificar(p[0]) + '=' + codificar(p[1])).join('&'); }
  }
  class URL {
    constructor(url, base) {
      const absoluta = base !== undefined ? unir(String(url), String(base)) : String(url);
      const m = /^([a-z][a-z0-9+.-]*:)(?:\/\/(?:([^:@\/]*)(?::([^@\/]*))?@)?(\[[^\]]*\]|[^\/?#:]*)(?::(\d*))?)?([^?#]*)(\?[^#]*)?(#.*)?$/i.exec(absoluta.trim());
      if (!m || (base === undefined && !/^[a-z][a-z0-9+.-]*:/i.test(absoluta))) throw new TypeError('Invalid URL: ' + url);
      this.protocol = m[1].toLowerCase();
      this.username = m[2] || '';
      this.password = m[3] || '';
      this.hostname = (m[4] || '').toLowerCase();
      const defecto = { 'http:': '80', 'https:': '443' }[this.protocol];
      this.port = m[5] && m[5] !== defecto ? m[5] : '';
      this.pathname = m[6] || (this.hostname ? '/' : '');
      if (this.hostname) this.pathname = normalizar(this.pathname);
      this['\u0001search'] = m[7] && m[7] !== '?' ? m[7] : '';
      this.hash = m[8] && m[8] !== '#' ? m[8] : '';
      this['\u0001params'] = null;
    }
    get host() { return this.hostname + (this.port ? ':' + this.port : ''); }
    get origin() { return this.hostname ? this.protocol + '//' + this.host : 'null'; }
    get search() { return this['\u0001search']; }
    set search(s) { s = String(s); this['\u0001search'] = s && s.charAt(0) !== '?' ? '?' + s : s === '?' ? '' : s; this['\u0001params'] = null; }
    get searchParams() {
      if (!this['\u0001params']) { this['\u0001params'] = new URLSearchParams(this['\u0001search']); this['\u0001params']['\u0001url'] = this; }
      return this['\u0001params'];
    }
    get href() {
      const cred = this.username ? this.username + (this.password ? ':' + this.password : '') + '@' : '';
      return this.protocol + (this.hostname || this.protocol === 'file:' ? '//' + cred + this.host : '') + this.pathname + this['\u0001search'] + this.hash;
    }
    set href(h) { const u = new URL(h); Object.keys(u).forEach((k) => { this[k] = u[k]; }); this['\u0001search'] = u['\u0001search']; this['\u0001params'] = null; }
    toString() { return this.href; }
    toJSON() { return this.href; }
    static canParse(u, b) { try { new URL(u, b); return true; } catch (e) { return false; } }
  }
  w.URL = URL;
  w.URLSearchParams = URLSearchParams;
  function ejecutar(texto, url, m) {
    var f = __quimera_modulo(texto);
    m.ejecutando = true;
    var importar = function (espec) {
      try { return cargar(resolver(espec, url)); } catch (e) { return Promise.reject(e); }
    };
    return f(m.ns, importar, { url: url, resolve: function (e) { return resolver(e, url); } }).then(
      function () { m.ejecutando = false; return m.ns; },
      function (e) { m.ejecutando = false; throw e; }
    );
  }
  function cargar(url) {
    var m = modulos[url];
    if (m) return m.ejecutando ? Promise.resolve(m.ns) : m.promesa;
    m = modulos[url] = { ns: Object.create(null), ejecutando: false };
    m.promesa = fetch(url).then(function (r) {
      if (!r.ok) throw new TypeError('no se pudo cargar el módulo ' + url + ' (' + r.status + ')');
      return r.text();
    }).then(function (texto) { return ejecutar(texto, url, m); });
    return m.promesa;
  }
  w.__quimera_resolver = resolver;
  w.__quimera_reexportar = function (destino, origen) {
    Object.keys(origen).forEach(function (k) {
      if (k !== 'default' && !(k in destino)) Object.defineProperty(destino, k, { get: function () { return origen[k]; } });
    });
  };
  w.__quimera_importar_dinamico = function (espec) {
    try { return cargar(resolver(espec, location.href)); } catch (e) { return Promise.reject(e); }
  };
  function avisar(donde) { return function (e) { console.error('Error en ' + donde + ': ' + String(e && e.message || e)); }; }
  w.__quimera_modulo_en_linea = function (texto) {
    var m = { ns: Object.create(null), ejecutando: false };
    var p;
    try { p = ejecutar(texto, location.href, m); } catch (e) { p = Promise.reject(e); }
    return p['catch'](avisar('un módulo de la página'));
  };
  w.__quimera_modulo_externo = function (src) { return w.__quimera_importar_dinamico(src)['catch'](avisar('el módulo ' + src)); };
})(this);"#;

/// `fetch`, `Response`, `Headers` y `XMLHttpRequest`, sobre `__quimera_pedir`.
pub(crate) const RED_JS: &str = r"(function (w) {
  var pendientes = {};
  function Headers(inicial) {
    var c = [];
    if (inicial && inicial['\u0001c']) c = inicial['\u0001c'].slice();
    else if (Array.isArray(inicial)) inicial.forEach(function (p) { c.push(String(p[0]).toLowerCase(), String(p[1])); });
    else if (inicial) Object.keys(inicial).forEach(function (k) { c.push(k.toLowerCase(), String(inicial[k])); });
    this['\u0001c'] = c;
  }
  Headers.prototype.get = function (k) {
    k = String(k).toLowerCase();
    var c = this['\u0001c'], r = null;
    for (var i = 0; i < c.length; i += 2) if (c[i] === k) r = r === null ? c[i + 1] : r + ', ' + c[i + 1];
    return r;
  };
  Headers.prototype.has = function (k) { return this.get(k) !== null; };
  Headers.prototype.set = function (k, v) { this['delete'](k); this['\u0001c'].push(String(k).toLowerCase(), String(v)); };
  Headers.prototype.append = function (k, v) { this['\u0001c'].push(String(k).toLowerCase(), String(v)); };
  Headers.prototype['delete'] = function (k) {
    k = String(k).toLowerCase();
    var c = this['\u0001c'], n = [];
    for (var i = 0; i < c.length; i += 2) if (c[i] !== k) n.push(c[i], c[i + 1]);
    this['\u0001c'] = n;
  };
  Headers.prototype.forEach = function (f, este) {
    var c = this['\u0001c'];
    for (var i = 0; i < c.length; i += 2) f.call(este, c[i + 1], c[i], this);
  };
  function Response(cuerpo, opciones) {
    opciones = opciones || {};
    this.status = opciones.status === undefined ? 200 : opciones.status;
    this.ok = this.status >= 200 && this.status < 300;
    this.statusText = opciones.statusText || '';
    this.headers = new Headers(opciones.headers);
    this.url = opciones.url || '';
    this.redirected = false;
    this.type = 'basic';
    this.bodyUsed = false;
    this['\u0001cuerpo'] = cuerpo === undefined || cuerpo === null ? '' : String(cuerpo);
  }
  Response.prototype.text = function () { this.bodyUsed = true; return Promise.resolve(this['\u0001cuerpo']); };
  Response.prototype.json = function () {
    var t = this['\u0001cuerpo'];
    this.bodyUsed = true;
    return new Promise(function (r) { r(JSON.parse(t)); });
  };
  Response.prototype.clone = function () {
    return new Response(this['\u0001cuerpo'], { status: this.status, statusText: this.statusText, headers: this.headers, url: this.url });
  };
  w.Headers = Headers;
  w.Response = Response;
  w.fetch = function (recurso, op) {
    op = op || {};
    var url = typeof recurso === 'string' ? recurso : (recurso && recurso.url) || String(recurso);
    var cab = new Headers(op.headers)['\u0001c'];
    var cuerpo = op.body === undefined || op.body === null ? null : String(op.body);
    var metodo = String(op.method || 'GET').toUpperCase();
    return new Promise(function (res, rej) {
      var id = __quimera_pedir(url, metodo, cuerpo, cab, String(op.credentials || 'same-origin'));
      pendientes[id] = { res: res, rej: rej };
    });
  };
  w.__quimera_respuesta = function (id, estado, url, cabeceras, cuerpo) {
    var p = pendientes[id];
    delete pendientes[id];
    if (!p) return;
    var r = new Response(cuerpo, { status: estado, url: url });
    r.headers['\u0001c'] = cabeceras;
    p.res(r);
  };
  w.__quimera_fallo = function (id, mensaje) {
    var p = pendientes[id];
    delete pendientes[id];
    if (p) p.rej(new TypeError('Failed to fetch (' + mensaje + ')'));
  };

  function XMLHttpRequest() {
    this.readyState = 0;
    this.status = 0;
    this.statusText = '';
    this.responseText = '';
    this.response = '';
    this.responseType = '';
    this.responseURL = '';
    this.withCredentials = false;
    this['\u0001cab'] = [];
    this['\u0001oyentes'] = {};
  }
  XMLHttpRequest.UNSENT = 0; XMLHttpRequest.OPENED = 1; XMLHttpRequest.HEADERS_RECEIVED = 2;
  XMLHttpRequest.LOADING = 3; XMLHttpRequest.DONE = 4;
  var X = XMLHttpRequest.prototype;
  X['\u0001aviso'] = function (tipo) {
    var ev = { type: tipo, target: this, currentTarget: this };
    var f = this['on' + tipo];
    if (typeof f === 'function') f.call(this, ev);
    var lista = this['\u0001oyentes'][tipo] || [];
    for (var i = 0; i < lista.length; i++) lista[i].call(this, ev);
  };
  X.addEventListener = function (tipo, f) { (this['\u0001oyentes'][tipo] = this['\u0001oyentes'][tipo] || []).push(f); };
  X.removeEventListener = function (tipo, f) {
    var l = this['\u0001oyentes'][tipo] || [];
    var i = l.indexOf(f);
    if (i >= 0) l.splice(i, 1);
  };
  X.open = function (metodo, url) {
    this['\u0001metodo'] = String(metodo).toUpperCase();
    this['\u0001url'] = String(url);
    this.readyState = 1;
    this['\u0001aviso']('readystatechange');
  };
  X.setRequestHeader = function (k, v) { this['\u0001cab'].push([k, v]); };
  X.getResponseHeader = function (k) { return this['\u0001respuesta'] ? this['\u0001respuesta'].get(k) : null; };
  X.getAllResponseHeaders = function () {
    var s = '';
    if (this['\u0001respuesta']) this['\u0001respuesta'].forEach(function (v, k) { s += k + ': ' + v + '\r\n'; });
    return s;
  };
  X.overrideMimeType = function () {};
  X.abort = function () { this['\u0001abortado'] = true; };
  X.send = function (cuerpo) {
    var x = this;
    var op = { method: this['\u0001metodo'], headers: this['\u0001cab'], body: cuerpo, credentials: this.withCredentials ? 'include' : 'same-origin' };
    w.fetch(this['\u0001url'], op).then(function (r) {
      return r.text().then(function (t) {
        if (x['\u0001abortado']) return;
        x.status = r.status;
        x.responseURL = r.url;
        x['\u0001respuesta'] = r.headers;
        x.responseText = t;
        if (x.responseType === 'json') { try { x.response = JSON.parse(t); } catch (e) { x.response = null; } }
        else x.response = t;
        x.readyState = 4;
        x['\u0001aviso']('readystatechange');
        x['\u0001aviso']('load');
        x['\u0001aviso']('loadend');
      });
    }, function () {
      if (x['\u0001abortado']) return;
      x.readyState = 4;
      x.status = 0;
      x['\u0001aviso']('readystatechange');
      x['\u0001aviso']('error');
      x['\u0001aviso']('loadend');
    });
  };
  w.XMLHttpRequest = XMLHttpRequest;
})(this);";

/// Apunta una petición en la cola y devuelve su número.
pub(crate) fn pedir(est: &Estado, mut p: PeticionJs) -> u32 {
    let id = est.siguiente_peticion.get();
    est.siguiente_peticion.set(id.wrapping_add(1));
    p.id = id;
    est.peticiones.borrow_mut().push(p);
    id
}

/// Los textos de un arreglo de JavaScript (`[k, v, k, v...]` → pares).
pub(crate) fn pares_de(m: &Motor, arr: Valor) -> Vec<(String, String)> {
    let n = m.mundo.longitud(arr).unwrap_or(0);
    let textos: Vec<String> =
        (0..n).map(|i| m.texto(m.mundo.leer_elemento(arr, i).unwrap_or(Valor::INDEFINIDO))).collect();
    textos.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect()
}

/// Tras meter `nodo` en el árbol: sus `<script>` nuevos se ejecutan (en línea) o se piden
/// (`src`), como en los navegadores. Cada script se ejecuta una sola vez.
pub(crate) fn scripts_insertados(m: &mut Motor, est: &Estado, nodo: NodoId) {
    let nuevos: Vec<(NodoId, Option<String>)> = {
        let doc = est.doc.borrow();
        if !doc.contiene(Documento::RAIZ, nodo) {
            return;
        }
        let mut ejecutados = est.ejecutados.borrow_mut();
        core::iter::once(nodo)
            .chain(doc.descendientes(nodo))
            .filter_map(|id| doc.elemento(id).filter(|e| e.nombre == "script").map(|e| (id, e)))
            .filter(|(_, e)| crate::es_javascript(e.atributo("type")))
            .filter(|(id, _)| ejecutados.insert(id.0))
            .map(|(id, e)| (id, e.atributo("src").map(String::from)))
            .collect()
    };
    for (id, src) in nuevos {
        match src {
            Some(url) => {
                let p = PeticionJs {
                    id: 0,
                    url,
                    metodo: String::from("GET"),
                    cuerpo: None,
                    cabeceras: Vec::new(),
                    credenciales: String::from("same-origin"),
                    es_script: true,
                };
                let pid = pedir(est, p);
                est.scripts_pendientes.borrow_mut().insert(pid, id.0);
            }
            None => {
                let codigo = est.doc.borrow().texto(id);
                if let Err(e) = m.ejecutar(&codigo) {
                    est.errores.borrow_mut().push(e.mensaje);
                }
            }
        }
    }
}

impl Dom {
    /// Lo que la página ha pedido a la red desde la última vez.
    pub fn tomar_peticiones(&self) -> Vec<PeticionJs> {
        core::mem::take(&mut *self.estado.peticiones.borrow_mut())
    }

    /// La respuesta (o por qué falló) a la petición `id`. Un script se ejecuta y lanza `load`
    /// (o `error`); un `fetch` resuelve (o rechaza) su promesa.
    pub fn responder(&mut self, id: u32, r: Result<RespuestaJs, String>) {
        let script = self.estado.scripts_pendientes.borrow_mut().remove(&id);
        if let Some(nodo) = script {
            match r {
                Ok(resp) if (200..300).contains(&resp.estado) => {
                    self.ejecutar(&resp.cuerpo);
                    self.despachar(NodoId(nodo), "load");
                }
                _ => {
                    self.despachar(NodoId(nodo), "error");
                }
            }
            return;
        }
        let m = &mut self.motor;
        let resultado = match r {
            Ok(resp) => {
                let mut v: Vec<Valor> = Vec::new();
                for (k, valor) in &resp.cabeceras {
                    v.push(m.cadena(k));
                    v.push(m.cadena(valor));
                }
                let cabeceras = m.crear_arreglo(&v);
                let marca = m.apilar(cabeceras);
                let (url, cuerpo) = (m.cadena(&resp.url), m.cadena(&resp.cuerpo));
                let args =
                    [Valor::entero(id as i32), Valor::entero(i32::from(resp.estado)), url, m.de_pila(marca), cuerpo];
                let r = m.llamar_global("__quimera_respuesta", &args);
                m.soltar_pila(marca);
                r
            }
            Err(motivo) => {
                let t = m.cadena(&motivo);
                m.llamar_global("__quimera_fallo", &[Valor::entero(id as i32), t])
            }
        };
        if let Err(e) = resultado {
            self.estado.errores.borrow_mut().push(e.mensaje);
        }
        microtareas(&mut self.motor, &self.estado);
    }
}
