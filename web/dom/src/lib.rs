//! El DOM de QUIMERA: el árbol del documento (`quimera-html`) visto desde JavaScript
//! (`quimera-vm`).
//!
//! - **Un solo árbol.** Los scripts cambian el mismo [`Documento`] que leen el estilo y la
//!   maquetación: no hay una copia para JavaScript. Tras un cambio, quien muestra la página
//!   vuelve a maquetar ([`Dom::tomar_cambios`]).
//! - **Un solo GC.** Cada nodo que JavaScript toca tiene un objeto del anfitrión en el montón de
//!   la VM (con el número del nodo), creado la primera vez y guardado en la raíz del anfitrión:
//!   el mismo nodo da siempre el mismo objeto (`a === b`). Los listeners y temporizadores también
//!   viven en ese montón. No hay dos montones que se apunten entre sí (el problema de Chromium
//!   que resolvió Oilpan).
//! - **Las propiedades** (`textContent`, `id`...) se le preguntan al anfitrión en cada acceso
//!   (`puente.rs`); los métodos son funciones del anfitrión en tres prototipos: nodo, elemento y
//!   documento.
//!
//! Hecho a mano para lo que más usan las páginas. Cuando la superficie crezca, estas tablas se
//! generarán desde WebIDL (ver docs/PLAN.md).
#![no_std]

extern crate alloc;

mod animaciones;
mod complementos;
mod eventos;
mod interfaces;
mod metodos;
mod mutaciones;
mod observadores;
mod puente;
mod red;
mod selectores;

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::{Cell, Ref, RefCell};
use quimera_html::{Dato, Documento, NodoId};
use quimera_vm::{Motor, Valor};

pub use red::{PeticionJs, RespuestaJs};
pub use selectores::Selector;

/// El número de "nodo" de `window` en los listeners (no es un nodo del árbol).
pub(crate) const VENTANA: u32 = u32::MAX;
/// Bit de los objetos `style` de un elemento (el resto es el número del nodo).
pub(crate) const BIT_ESTILO: u32 = 1 << 30;
/// Bit de los objetos `classList`.
pub(crate) const BIT_CLASES: u32 = 1 << 29;
/// Bit de los objetos `dataset` (los atributos `data-*`).
pub(crate) const BIT_DATOS: u32 = 1 << 28;

/// Lo que comparten el [`Dom`] y su puente con la VM.
pub(crate) struct Estado {
    pub(crate) doc: RefCell<Documento>,
    /// El árbol cambió desde la última vez que se preguntó.
    pub(crate) cambiado: Cell<bool>,
    /// `preventDefault()` y `stopPropagation()` del evento en curso.
    pub(crate) prevenido: Cell<bool>,
    pub(crate) detenido: Cell<bool>,
    /// La hora del anfitrión en milisegundos (para los temporizadores).
    pub(crate) ahora: Cell<f64>,
    pub(crate) siguiente_temporizador: Cell<i32>,
    /// La dirección de la página (`location.href`, `document.URL`).
    pub(crate) url: String,
    /// Errores de JavaScript que nadie capturó (de eventos y temporizadores).
    pub(crate) errores: RefCell<Vec<String>>,
    /// Lo pedido a la red que el anfitrión aún no ha recogido (`red.rs`).
    pub(crate) peticiones: RefCell<Vec<red::PeticionJs>>,
    pub(crate) siguiente_peticion: Cell<u32>,
    /// Peticiones de `<script src>` añadidos por scripts: número de petición → nodo.
    pub(crate) scripts_pendientes: RefCell<alloc::collections::BTreeMap<u32, u32>>,
    /// Las imágenes que ya llegaron, por `src`: su tamaño propio (`None` si fallaron).
    pub(crate) naturales: RefCell<alloc::collections::BTreeMap<String, Option<(u32, u32)>>>,
    /// Los `<script>` ya ejecutados (o pedidos): cada uno corre una sola vez.
    pub(crate) ejecutados: RefCell<alloc::collections::BTreeSet<u32>>,
    /// Quien maqueta el documento para las medidas (`clientWidth`...); lo pone el anfitrión.
    pub(crate) medir: RefCell<Option<Medir>>,
    /// Las últimas medidas, hasta que el árbol cambie.
    pub(crate) medidas: RefCell<Option<Rc<Medidas>>>,
    /// El árbol cambió y los observadores aún no lo saben.
    pub(crate) sin_observar: Cell<bool>,
    /// Los nodos que observa algún `MutationObserver` (número, con `subtree`).
    pub(crate) observados: RefCell<Vec<(u32, bool)>>,
    /// Cambió el árbol con observadores: hay que encolar la microtarea que los avisa.
    pub(crate) mutacion_pendiente: Cell<bool>,
}

/// Maqueta un documento y da sus medidas (el DOM no maqueta: está por debajo del estilo).
pub type Medir = alloc::boxed::Box<dyn Fn(&Documento) -> Medidas>;

/// Las medidas de la maquetación que lee JavaScript.
#[derive(Debug, Default, Clone)]
pub struct Medidas {
    /// La caja de borde (x, y, ancho, alto) de cada elemento maquetado, por número de nodo.
    pub cajas: alloc::collections::BTreeMap<u32, [f32; 4]>,
    /// El tamaño de la ventana.
    pub ventana: (f32, f32),
}

impl Estado {
    /// El árbol cambió: hay que repintar y las medidas ya no valen.
    pub(crate) fn tocar(&self) {
        self.cambiado.set(true);
        self.sin_observar.set(true);
        if !self.observados.borrow().is_empty() {
            self.mutacion_pendiente.set(true);
        }
        self.medidas.borrow_mut().take();
    }

    /// Las medidas del documento como está ahora (maqueta si cambió desde la última vez).
    pub(crate) fn medidas(&self) -> Option<Rc<Medidas>> {
        if let Some(m) = self.medidas.borrow().as_ref() {
            return Some(m.clone());
        }
        let medir = self.medir.borrow();
        let m = Rc::new((medir.as_ref()?)(&self.doc.borrow()));
        *self.medidas.borrow_mut() = Some(m.clone());
        Some(m)
    }
}

/// Una página con JavaScript: su árbol y su motor.
pub struct Dom {
    /// El motor de JavaScript de la página.
    pub motor: Motor,
    estado: Rc<Estado>,
    /// Cuántas líneas de la consola ya se entregaron.
    consola_leida: usize,
}

/// El objeto de JavaScript de un nodo (el mismo siempre; se crea la primera vez).
pub(crate) fn envolver(m: &mut Motor, est: &Estado, id: NodoId) -> Valor {
    if id.0 == VENTANA {
        return m.global();
    }
    let raiz = m.raiz_anfitrion();
    let nodos = m.leer_nombre(raiz, "nodos");
    if let Some(w) = m.mundo.leer_elemento(nodos, id.0).filter(|v| m.mundo.es_objeto(*v)) {
        return w;
    }
    // El prototipo de su interfaz: el de su etiqueta (HTMLInputElement...), o el genérico.
    let (proto, etiqueta) = match &est.doc.borrow().nodo(id).dato {
        Dato::Elemento(e) if e.nombre == quimera_html::FRAGMENTO => ("p_fragmento", None),
        Dato::Elemento(e) if puente::SVG.contains(&e.nombre.as_str()) => ("p_svg", None),
        Dato::Elemento(e) => ("p_html", Some(e.nombre.clone())),
        Dato::Texto(_) | Dato::Comentario(_) => ("p_caracteres", None),
        Dato::Documento => ("p_documento", None),
        Dato::Doctype(_) => ("p_nodo", None),
    };
    let propio = etiqueta.and_then(|t| {
        let etiquetas = m.leer_nombre(raiz, "p_etiquetas");
        let clave = m.mundo.cadenas.internar(&t);
        m.mundo.leer_propia(etiquetas, clave).filter(|p| m.mundo.es_objeto(*p))
    });
    let proto = match propio {
        Some(p) => p,
        None => m.leer_nombre(raiz, proto),
    };
    let w = m.crear_objeto_anfitrion(id.0, proto);
    let marca = m.apilar(w);
    let nodos = m.leer_nombre(m.raiz_anfitrion(), "nodos");
    let w = m.de_pila(marca);
    m.mundo.escribir_elemento(nodos, id.0, w);
    let w = m.de_pila(marca);
    m.soltar_pila(marca);
    w
}

/// Las microtareas (promesas, `queueMicrotask`): como en los navegadores, se vacían al terminar
/// cada script, cada manejador de eventos y cada temporizador.
pub(crate) fn microtareas(m: &mut Motor, est: &Estado) {
    microtareas_sin_mutaciones(m, est);
    // Los avisos de `MutationObserver` (`mutaciones.rs`).
    if !est.observados.borrow().is_empty() {
        mutaciones::entregar(m, est);
    }
}

/// Las microtareas, sin entregar los cambios a los `MutationObserver`.
pub(crate) fn microtareas_sin_mutaciones(m: &mut Motor, est: &Estado) {
    let errores = m.correr_microtareas();
    est.errores.borrow_mut().extend(errores);
    // Si la tarea cambió el árbol, los observadores de tamaño y de cruce miran en el siguiente
    // fotograma (`observadores.rs`).
    if est.sin_observar.replace(false)
        && let Err(e) = m.llamar_global("__quimera_tras_cambio", &[])
    {
        est.errores.borrow_mut().push(e.completo());
    }
}

/// `localStorage` y `sessionStorage`, en memoria (se pierden al cerrar la página).
const ALMACENES: &str = r"(function (w) {
  function Almacen() { this['\u0001d'] = Object.create(null); this.length = 0; }
  Almacen.prototype.getItem = function (k) { var d = this['\u0001d']; k = String(k); return k in d ? d[k] : null; };
  Almacen.prototype.setItem = function (k, v) { var d = this['\u0001d']; k = String(k); if (!(k in d)) this.length++; d[k] = String(v); };
  Almacen.prototype.removeItem = function (k) { var d = this['\u0001d']; k = String(k); if (k in d) { delete d[k]; this.length--; } };
  Almacen.prototype.clear = function () { this['\u0001d'] = Object.create(null); this.length = 0; };
  Almacen.prototype.key = function (i) { var k = Object.keys(this['\u0001d'])[i]; return k === undefined ? null : k; };
  w.localStorage = new Almacen();
  w.sessionStorage = new Almacen();
})(this);";

/// `document.implementation` (`createHTMLDocument` hace un `html`/`head`/`body` sueltos con los
/// métodos de creación de este documento: jQuery lo usa para analizar HTML sin ejecutarlo) y
/// `document.location`.
const DOCUMENTO_JS: &str = r"(function (w, d) {
  // location completo, a partir de href.
  var partes = /^([a-z][a-z0-9+.-]*:)(?:\/\/([^/?#:]*)(?::(\d+))?)?([^?#]*)(\?[^#]*)?(#.*)?$/i.exec(w.location.href) || [];
  var loc = w.location;
  loc.protocol = partes[1] || '';
  loc.hostname = partes[2] || '';
  loc.port = partes[3] || '';
  loc.host = loc.hostname + (loc.port ? ':' + loc.port : '');
  loc.pathname = partes[4] || '/';
  loc.search = partes[5] || '';
  loc.hash = partes[6] || '';
  loc.origin = loc.host ? loc.protocol + '//' + loc.host : 'null';
  loc.toString = function () { return this.href; };
  loc.assign = loc.replace = loc.reload = function () {};
  d.location = loc;
  d.cookie = '';
  d.referrer = '';
  d.characterSet = 'UTF-8';
  d.visibilityState = 'visible';
  d.hidden = false;
  d.hasFocus = function () { return true; };
  // Aproximaciones: lo justo para que las páginas no se caigan (ver docs/DOM.md).
  w.history = { length: 1, state: null, scrollRestoration: 'auto',
    pushState: function (s) { this.state = s; }, replaceState: function (s) { this.state = s; },
    back: function () {}, forward: function () {}, go: function () {} };
  var nav = w.navigator;
  nav.languages = ['es', 'en'];
  nav.platform = 'Win32';
  nav.vendor = '';
  nav.cookieEnabled = true;
  nav.onLine = true;
  nav.hardwareConcurrency = 4;
  nav.maxTouchPoints = 0;
  w.screen = { width: 1920, height: 1080, availWidth: 1920, availHeight: 1040, colorDepth: 24, pixelDepth: 24 };
  w.innerWidth = 1024; w.innerHeight = 768; w.outerWidth = 1024; w.outerHeight = 768;
  w.devicePixelRatio = 1;
  w.scrollX = w.scrollY = w.pageXOffset = w.pageYOffset = 0;
  w.scrollTo = w.scrollBy = w.scroll = w.focus = w.blur = function () {};
  w.getComputedStyle = function (el) { return el.style; };
  // El origen se toma en la primera lectura: el reloj real llega después de este código.
  var inicio = null;
  function origen() { if (inicio === null) inicio = Date.now(); return inicio; }
  // La entrada de navegación (`responseStart`...): todo ocurre en el instante de origen, que es
  // cuando llegó la página (QUIMERA no mide aún las fases de la red).
  function navegacion() {
    var e = { name: w.location.href, entryType: 'navigation', startTime: 0, duration: 0, type: 'navigate',
      redirectCount: 0, transferSize: 0, encodedBodySize: 0, decodedBodySize: 0, nextHopProtocol: 'http/1.1' };
    ['fetchStart', 'domainLookupStart', 'domainLookupEnd', 'connectStart', 'connectEnd', 'secureConnectionStart',
      'requestStart', 'responseStart', 'responseEnd', 'domInteractive', 'domContentLoadedEventStart',
      'domContentLoadedEventEnd', 'domComplete', 'loadEventStart', 'loadEventEnd', 'redirectStart', 'redirectEnd',
      'unloadEventStart', 'unloadEventEnd', 'workerStart'].forEach(function (k) { e[k] = 0; });
    e.toJSON = function () { return this; };
    return e;
  }
  w.performance = { now: function () { return Date.now() - origen(); },
    mark: function () {}, measure: function () {}, clearMarks: function () {}, clearMeasures: function () {},
    getEntries: function () { return [navegacion()]; },
    getEntriesByName: function () { return []; },
    getEntriesByType: function (t) { return t === 'navigation' ? [navegacion()] : []; },
    setResourceTimingBufferSize: function () {}, toJSON: function () { return {}; } };
  Object.defineProperty(w.performance, 'timeOrigin', { get: origen, enumerable: true });
  // `performance.timing` (el antiguo): los instantes en milisegundos desde 1970.
  Object.defineProperty(w.performance, 'timing', { get: function () {
    var t = {}, o = Math.round(origen());
    ['navigationStart', 'fetchStart', 'domainLookupStart', 'domainLookupEnd', 'connectStart', 'connectEnd',
      'requestStart', 'responseStart', 'responseEnd', 'domLoading', 'domInteractive', 'domContentLoadedEventStart',
      'domContentLoadedEventEnd', 'domComplete', 'loadEventStart', 'loadEventEnd'].forEach(function (k) { t[k] = o; });
    t.unloadEventStart = t.unloadEventEnd = t.redirectStart = t.redirectEnd = t.secureConnectionStart = 0;
    return t;
  }, enumerable: true });
  w.requestIdleCallback = function (f) { return setTimeout(function () { f({ didTimeout: false, timeRemaining: function () { return 50; } }); }, 1); };
  w.cancelIdleCallback = clearTimeout;
  function Observador() { this.observe = this.unobserve = this.disconnect = function () {}; this.takeRecords = function () { return []; }; }
  // Sin entradas de rendimiento todavía: no anuncia ningún tipo.
  w.PerformanceObserver = function PerformanceObserver() { Observador.call(this); };
  w.PerformanceObserver.supportedEntryTypes = [];
  d.implementation = {
    hasFeature: function () { return true; },
    createHTMLDocument: function (titulo) {
      var html = d.createElement('html'), head = d.createElement('head'), body = d.createElement('body');
      html.appendChild(head);
      html.appendChild(body);
      var nuevo = {
        nodeType: 9, nodeName: '#document', documentElement: html, head: head, body: body, title: titulo || '',
        implementation: d.implementation, location: null, defaultView: null,
        createElement: function (t) { return d.createElement(t); },
        createTextNode: function (t) { return d.createTextNode(t); },
        createComment: function (t) { return d.createComment(t); },
        createDocumentFragment: function () { return d.createDocumentFragment(); },
        getElementById: function (i) { return html.querySelector('#' + i); },
        getElementsByTagName: function (t) { return html.getElementsByTagName(t); },
        querySelector: function (s) { return html.querySelector(s); },
        querySelectorAll: function (s) { return html.querySelectorAll(s); }
      };
      return nuevo;
    }
  };
})(this, this.document);";

/// El objeto de un nodo, o `null`.
pub(crate) fn envolver_o_nulo(m: &mut Motor, est: &Estado, id: Option<NodoId>) -> Valor {
    id.map_or(Valor::NULO, |id| envolver(m, est, id))
}

/// Un arreglo con los objetos de estos nodos (una foto, no una lista viva).
pub(crate) fn lista(m: &mut Motor, est: &Estado, ids: &[NodoId]) -> Valor {
    let arr = m.crear_arreglo(&[]);
    let marca = m.apilar(arr);
    for &id in ids {
        let w = envolver(m, est, id);
        let arr = m.de_pila(marca);
        m.mundo.empujar(arr, w);
    }
    let arr = m.de_pila(marca);
    m.soltar_pila(marca);
    arr
}

/// Si `tipo` es el de un módulo (`<script type=module>`).
pub(crate) fn es_modulo(tipo: Option<&str>) -> bool {
    tipo.is_some_and(|t| t.trim().eq_ignore_ascii_case("module"))
}

/// Si `tipo` es de un `<script>` clásico que QUIMERA ejecuta.
pub(crate) fn es_javascript(tipo: Option<&str>) -> bool {
    match tipo.map(|t| t.trim().to_ascii_lowercase()) {
        None => true,
        Some(t) => matches!(
            t.as_str(),
            "" | "text/javascript" | "application/javascript" | "text/ecmascript" | "application/ecmascript"
        ),
    }
}

impl Dom {
    /// El DOM de un documento ya analizado, en la dirección `url`, con su `window` y su `document`.
    pub fn nuevo(doc: Documento, url: &str) -> Dom {
        let estado = Rc::new(Estado {
            doc: RefCell::new(doc),
            cambiado: Cell::new(false),
            prevenido: Cell::new(false),
            detenido: Cell::new(false),
            ahora: Cell::new(0.0),
            siguiente_temporizador: Cell::new(1),
            url: String::from(url),
            errores: RefCell::new(Vec::new()),
            peticiones: RefCell::new(Vec::new()),
            siguiente_peticion: Cell::new(1),
            scripts_pendientes: RefCell::new(alloc::collections::BTreeMap::new()),
            naturales: RefCell::new(alloc::collections::BTreeMap::new()),
            ejecutados: RefCell::new(alloc::collections::BTreeSet::new()),
            medir: RefCell::new(None),
            medidas: RefCell::new(None),
            sin_observar: Cell::new(false),
            observados: RefCell::new(Vec::new()),
            mutacion_pendiente: Cell::new(false),
        });
        let mut motor = Motor::default();
        motor.poner_anfitrion(Rc::new(puente::Puente(estado.clone())));
        puente::preparar(&mut motor, &estado);
        motor.fijar_origen("quimera:dom");
        for codigo in [
            ALMACENES,
            red::RED_JS,
            red::MODULOS_JS,
            interfaces::INTERFACES_JS,
            complementos::COMPLEMENTOS_JS,
            observadores::OBSERVADORES_JS,
            animaciones::ANIMACIONES_JS,
            DOCUMENTO_JS,
        ] {
            if let Err(e) = motor.ejecutar(codigo) {
                estado.errores.borrow_mut().push(e.completo());
            }
        }
        motor.fijar_biblioteca();
        motor.fijar_origen(url);
        Dom { motor, estado, consola_leida: 0 }
    }

    /// El árbol (para el estilo y la maquetación).
    pub fn documento(&self) -> Ref<'_, Documento> {
        self.estado.doc.borrow()
    }

    /// Si el árbol cambió desde la última vez (y lo olvida).
    pub fn tomar_cambios(&self) -> bool {
        self.estado.cambiado.replace(false)
    }

    /// Las líneas nuevas de `console.log` desde la última vez.
    pub fn consola(&mut self) -> Vec<String> {
        let nuevas = self.motor.salida().get(self.consola_leida..).unwrap_or_default().to_vec();
        self.consola_leida = self.motor.salida().len();
        nuevas
    }

    /// Los errores de JavaScript sin capturar desde la última vez.
    pub fn errores(&self) -> Vec<String> {
        core::mem::take(&mut *self.estado.errores.borrow_mut())
    }

    /// La hora del anfitrión en milisegundos.
    pub fn poner_hora(&self, ms: f64) {
        self.estado.ahora.set(ms);
    }

    /// Ejecuta los `<script>` de la página en orden (los que había al cargarla), y luego lanza
    /// `DOMContentLoaded` y `load`. `externo` da el código de un `src` (o `None` si no llegó).
    /// Los de tipo `module` todavía no se ejecutan.
    pub fn ejecutar_scripts(&mut self, externo: &dyn Fn(&str) -> Option<String>) {
        let scripts: Vec<(Option<String>, Option<String>, NodoId)> = {
            let doc = self.estado.doc.borrow();
            doc.descendientes(Documento::RAIZ)
                .filter_map(|id| doc.elemento(id).filter(|e| e.nombre == "script").map(|e| (e, id)))
                // Con módulos, los `nomodule` (la versión para navegadores viejos) no corren.
                .filter(|(e, _)| e.atributo("nomodule").is_none())
                .filter(|(e, _)| {
                    let t = e.atributo("type");
                    es_javascript(t) || es_modulo(t) || t.is_some_and(|t| t.trim().eq_ignore_ascii_case("importmap"))
                })
                .map(|(e, id)| (e.atributo("src").map(String::from), e.atributo("type").map(String::from), id))
                .collect()
        };
        self.estado.ejecutados.borrow_mut().extend(scripts.iter().map(|(_, _, id)| id.0));
        // Los módulos van después de los scripts clásicos (como `defer`).
        // El mapa de importación, antes que nada.
        let (mapas, scripts): (Vec<_>, Vec<_>) = scripts
            .into_iter()
            .partition(|(_, t, _)| t.as_deref().is_some_and(|t| t.trim().eq_ignore_ascii_case("importmap")));
        for (_, _, id) in mapas {
            let texto = self.estado.doc.borrow().texto(id);
            let t = self.motor.cadena(&texto);
            if let Err(e) = self.motor.llamar_global("__quimera_mapa_importaciones", &[t]) {
                self.estado.errores.borrow_mut().push(e.completo());
            }
        }
        let (modulos, clasicos): (Vec<_>, Vec<_>) = scripts.into_iter().partition(|(_, t, _)| es_modulo(t.as_deref()));
        for (src, _, id) in clasicos {
            let codigo = match &src {
                Some(s) => externo(s),
                None => Some(self.estado.doc.borrow().texto(id)),
            };
            match codigo {
                Some(c) => {
                    let origen = src.clone().unwrap_or_else(|| self.estado.url.clone());
                    self.ejecutar_desde(&c, &origen);
                }
                None => {
                    self.estado.errores.borrow_mut().push(format!("no llegó el script {}", src.unwrap_or_default()))
                }
            }
        }
        for (src, _, id) in modulos {
            let r = match src {
                Some(s) => {
                    let s = self.motor.cadena(&s);
                    self.motor.llamar_global("__quimera_modulo_externo", &[s])
                }
                None => {
                    let texto = self.estado.doc.borrow().texto(id);
                    let t = self.motor.cadena(&texto);
                    self.motor.llamar_global("__quimera_modulo_en_linea", &[t])
                }
            };
            if let Err(e) = r {
                self.estado.errores.borrow_mut().push(e.completo());
            }
        }
        microtareas(&mut self.motor, &self.estado);
        self.despachar(Documento::RAIZ, "DOMContentLoaded");
        self.despachar(NodoId(VENTANA), "load");
    }

    /// Da al DOM quien maqueta, para `clientWidth`, `getBoundingClientRect`... y el tamaño de la
    /// ventana (`innerWidth`, `innerHeight`).
    pub fn con_medidas(&mut self, (ancho, alto): (f32, f32), medir: Medir) {
        *self.estado.medir.borrow_mut() = Some(medir);
        self.estado.medidas.borrow_mut().take();
        let codigo = alloc::format!(
            "window.innerWidth = window.outerWidth = {ancho}; window.innerHeight = window.outerHeight = {alto};"
        );
        self.ejecutar(&codigo);
    }

    /// El reloj de `Date` (milisegundos desde 1970) y el azar del sistema (`crypto`,
    /// `Math.random`). El DOM es `no_std`: los da quien lo usa (la ventana).
    pub fn con_sistema(&mut self, reloj: fn() -> f64, entropia: fn(&mut [u8])) {
        self.motor.con_reloj(reloj);
        self.motor.con_entropia(entropia);
    }

    /// Como [`Dom::ejecutar`], con el código de `origen` (la URL del script, para las trazas).
    pub fn ejecutar_desde(&mut self, codigo: &str, origen: &str) {
        self.motor.fijar_origen(origen);
        self.ejecutar(codigo);
        let pagina = self.estado.url.clone();
        self.motor.fijar_origen(&pagina);
    }

    /// Ejecuta código en la página (sus errores van a [`Dom::errores`]).
    pub fn ejecutar(&mut self, codigo: &str) {
        if let Err(e) = self.motor.ejecutar(codigo) {
            self.estado.errores.borrow_mut().push(e.completo());
        }
        microtareas(&mut self.motor, &self.estado);
    }

    /// Un clic en `nodo`. Devuelve `true` si un script llamó a `preventDefault()` (el navegador
    /// no debe seguir el enlace ni enviar el formulario).
    pub fn clic(&mut self, nodo: NodoId) -> bool {
        self.despachar(nodo, "click")
    }

    /// Lanza el evento `tipo` en `nodo`, que sube por sus antepasados hasta `window`. Devuelve
    /// si se llamó a `preventDefault()`.
    pub fn despachar(&mut self, nodo: NodoId, tipo: &str) -> bool {
        puente::despachar(&mut self.motor, &self.estado, nodo, tipo)
    }

    /// Llegó la imagen `src` (con su tamaño propio) o falló (`None`): sus `<img>` lanzan `load`
    /// (o `error`) y ya dicen `naturalWidth`, `naturalHeight` y `complete`.
    pub fn imagen_cargada(&mut self, src: &str, tam: Option<(u32, u32)>) {
        self.estado.naturales.borrow_mut().insert(String::from(src), tam);
        let imgs: Vec<NodoId> = {
            let doc = self.estado.doc.borrow();
            doc.descendientes(Documento::RAIZ)
                .filter(|&id| doc.elemento(id).is_some_and(|e| e.nombre == "img" && e.atributo("src") == Some(src)))
                .collect()
        };
        let tipo = if tam.is_some() { "load" } else { "error" };
        for id in imgs {
            puente::despachar_quieto(&mut self.motor, &self.estado, id, tipo);
        }
    }

    /// Ejecuta los temporizadores que ya tocan a la hora `ahora` (ms). Los que se crean mientras
    /// tanto esperan a la siguiente vez.
    pub fn correr_temporizadores(&mut self, ahora: f64) {
        self.estado.ahora.set(ahora);
        puente::correr_temporizadores(&mut self.motor, &self.estado, ahora);
    }

    /// Cuándo toca el próximo temporizador (ms), si hay alguno.
    pub fn proximo_temporizador(&mut self) -> Option<f64> {
        puente::proximo_temporizador(&mut self.motor)
    }
}

#[cfg(test)]
mod pruebas;
