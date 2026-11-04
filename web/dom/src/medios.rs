//! `MediaSource`, `SourceBuffer` y `<video>` (y `<audio>`).
//!
//! - Cada `SourceBuffer` es una **fuente** en Rust: lo que se le añade va al demultiplexor MP4 de
//!   `quimera-video`, que dice qué tiempos tienen datos (`buffered`). Las muestras de vídeo
//!   esperan a que el anfitrión las tome (`Dom::tomar_videos`), las decodifique y pinte.
//! - El **reloj** de cada `<video>` también vive aquí, sobre la hora de la página: corre mientras
//!   se reproduce y se para al final de los datos. Así va igual en la ventana y en una captura.
//! - La API de la web (`MEDIOS_JS`) va en JavaScript encima, con sus eventos: `sourceopen`,
//!   `updateend`, `loadedmetadata`, `canplay`, `playing`, `timeupdate`, `waiting`, `ended`...
//!
//! Solo H.264 y AAC en MP4 (lo que se sabe decodificar): `isTypeSupported` dice que no a lo
//! demás, y el reproductor de YouTube elige esos.

use crate::Estado;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use quimera_video::mp4::{Codec, Demuxer, Muestra};

/// Lo que se ha metido en un `SourceBuffer`.
pub(crate) struct Fuente {
    demux: Demuxer,
    es_video: bool,
    /// Tiempos con datos, en segundos: ordenados y sin solaparse.
    pub(crate) rangos: Vec<(f64, f64)>,
    /// Muestras de vídeo que el anfitrión aún no ha tomado.
    pendientes: Vec<Muestra>,
    /// `avcC` y unidades por segundo de la pista de vídeo.
    pub(crate) config: Option<(Vec<u8>, u32)>,
    /// Ancho y alto de la imagen.
    pub(crate) tam: (u16, u16),
}

/// El reloj de un `<video>`.
#[derive(Default, Clone, Copy)]
pub(crate) struct Reloj {
    /// El tiempo del vídeo (s) en `desde` (ms de la página).
    tiempo: f64,
    desde: f64,
    corre: bool,
    velocidad: f64,
    /// La fuente de vídeo que lo alimenta.
    fuente: Option<usize>,
}

/// Todas las fuentes y relojes de la página.
#[derive(Default)]
pub(crate) struct Medios {
    pub(crate) fuentes: Vec<Fuente>,
    relojes: BTreeMap<u32, Reloj>,
}

/// Una fuente nueva para un `SourceBuffer` de `tipo` (`video/mp4; codecs="avc1..."`).
pub(crate) fn nueva_fuente(est: &Estado, tipo: &str) -> usize {
    let mut m = est.medios.borrow_mut();
    m.fuentes.push(Fuente {
        demux: Demuxer::nuevo(),
        es_video: tipo.trim_start().to_ascii_lowercase().starts_with("video"),
        rangos: Vec::new(),
        pendientes: Vec::new(),
        config: None,
        tam: (0, 0),
    });
    m.fuentes.len() - 1
}

/// Une `(a, b)` a los rangos (los que se tocan, con una décima de margen, quedan en uno).
fn unir(rangos: &mut Vec<(f64, f64)>, a: f64, b: f64) {
    rangos.push((a, b));
    rangos.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut v: Vec<(f64, f64)> = Vec::with_capacity(rangos.len());
    for &(a, b) in rangos.iter() {
        match v.last_mut() {
            Some(u) if a <= u.1 + 0.1 => u.1 = u.1.max(b),
            _ => v.push((a, b)),
        }
    }
    *rangos = v;
}

/// Añade bytes a la fuente `id` (con `desfase`, el `timestampOffset`, en segundos) y devuelve sus
/// rangos con datos como `[inicio0, fin0, inicio1, fin1...]`.
pub(crate) fn anadir(est: &Estado, id: usize, datos: &[u8], desfase: f64) -> Vec<f64> {
    let mut m = est.medios.borrow_mut();
    let Some(f) = m.fuentes.get_mut(id) else { return Vec::new() };
    let muestras = f.demux.empujar(datos);
    if let Some(p) = f.demux.pistas.first() {
        if let Codec::Avc { avcc, ancho, alto } = &p.codec {
            f.config = Some((avcc.clone(), p.escala));
            f.tam = (*ancho, *alto);
        }
        let escala = f64::from(p.escala.max(1));
        for s in &muestras {
            let a = s.pts as f64 / escala + desfase;
            unir(&mut f.rangos, a, a + f64::from(s.duracion) / escala);
        }
    }
    if f.es_video {
        f.pendientes.extend(muestras);
    }
    f.rangos.iter().flat_map(|&(a, b)| [a, b]).collect()
}

/// `SourceBuffer.remove(a, b)`: quita esos tiempos de los rangos.
pub(crate) fn quitar(est: &Estado, id: usize, a: f64, b: f64) -> Vec<f64> {
    let mut m = est.medios.borrow_mut();
    let Some(f) = m.fuentes.get_mut(id) else { return Vec::new() };
    let mut v = Vec::new();
    for &(x, y) in &f.rangos {
        if x < a {
            v.push((x, y.min(a)));
        }
        if y > b {
            v.push((x.max(b), y));
        }
    }
    f.rangos = v.into_iter().filter(|(x, y)| y > x).collect();
    f.rangos.iter().flat_map(|&(a, b)| [a, b]).collect()
}

/// Qué fuente de vídeo alimenta al `<video>` `nodo`.
pub(crate) fn enlazar(est: &Estado, nodo: u32, fuente: usize) {
    est.medios.borrow_mut().relojes.entry(nodo).or_default().fuente = Some(fuente);
}

/// El tiempo del `<video>` `nodo` ahora: si corre, avanza con la página, sin pasar del final de
/// los datos que tiene su fuente de vídeo.
pub(crate) fn tiempo(est: &Estado, nodo: u32) -> f64 {
    let m = est.medios.borrow();
    let Some(r) = m.relojes.get(&nodo) else { return 0.0 };
    if !r.corre {
        return r.tiempo;
    }
    let t = r.tiempo + (est.ahora.get() - r.desde).max(0.0) / 1000.0 * r.velocidad;
    let tope = r
        .fuente
        .and_then(|f| m.fuentes.get(f))
        .and_then(|f| f.rangos.iter().find(|(a, b)| r.tiempo >= *a - 0.1 && r.tiempo <= *b + 0.1).map(|x| x.1));
    tope.map_or(t, |fin| t.min(fin))
}

/// Pone el reloj del `<video>` `nodo` en `tiempo`, corriendo o parado, a `velocidad`.
pub(crate) fn poner_reloj(est: &Estado, nodo: u32, tiempo: f64, corre: bool, velocidad: f64) {
    let ahora = est.ahora.get();
    let mut m = est.medios.borrow_mut();
    let r = m.relojes.entry(nodo).or_default();
    (r.tiempo, r.desde, r.corre, r.velocidad) = (tiempo, ahora, corre, velocidad);
}

/// Ancho y alto del vídeo de la fuente `id` (0 × 0 si es de audio); `None` hasta que llega su
/// segmento de inicio.
pub(crate) fn tam(est: &Estado, id: usize) -> Option<(u16, u16)> {
    est.medios.borrow().fuentes.get(id).filter(|f| !f.demux.pistas.is_empty()).map(|f| f.tam)
}

/// Lo que el anfitrión necesita para pintar un `<video>`.
#[derive(Debug, Clone)]
pub struct VideoPendiente {
    /// El elemento `<video>`.
    pub nodo: quimera_html::NodoId,
    /// Su tiempo ahora, en segundos.
    pub tiempo: f64,
    /// `avcC` y unidades por segundo de su pista de vídeo.
    pub config: Option<(Vec<u8>, u32)>,
    /// Las muestras nuevas desde la última vez.
    pub muestras: Vec<Muestra>,
    /// Ancho y alto de la imagen (0 × 0 hasta su segmento de inicio).
    pub tam: (u16, u16),
    /// Si su reloj corre, y a qué velocidad: quien pinta puede llevar el tiempo por su cuenta.
    pub corre: bool,
    /// `playbackRate`.
    pub velocidad: f64,
}

/// Los `<video>` con su tiempo y sus muestras nuevas (que dejan de estar pendientes).
pub(crate) fn tomar(est: &Estado) -> Vec<VideoPendiente> {
    let enlazados: Vec<(u32, Reloj)> = est.medios.borrow().relojes.iter().map(|(n, r)| (*n, *r)).collect();
    let mut v = Vec::new();
    for (nodo, r) in enlazados {
        let t = tiempo(est, nodo);
        let mut m = est.medios.borrow_mut();
        let (config, muestras, tam) = match r.fuente.and_then(|f| m.fuentes.get_mut(f)) {
            Some(f) => (f.config.clone(), core::mem::take(&mut f.pendientes), f.tam),
            None => (None, Vec::new(), (0, 0)),
        };
        let nodo = quimera_html::NodoId(nodo);
        v.push(VideoPendiente { nodo, tiempo: t, config, muestras, tam, corre: r.corre, velocidad: r.velocidad });
    }
    v
}

/// La API de la web (tras `binario.rs`).
pub(crate) const MEDIOS_JS: &str = r#"(function (w) {
  var K = '\u0001m', O = '\u0001o';
  // ---------------- blob: ----------------
  var registro = {}, cuenta = 0;
  w.URL.createObjectURL = function (o) {
    var u = 'blob:' + w.location.origin + '/' + (++cuenta).toString(16) + '-' + Math.random().toString(16).slice(2, 10);
    registro[u] = o;
    return u;
  };
  w.URL.revokeObjectURL = function (u) { delete registro[u]; };
  w.__quimera_blob = function (u) { return registro[u]; };

  // ---------------- eventos de objetos que no son nodos ----------------
  var Emisor = {
    addEventListener: function (t, f) { if (f) (this[O] = this[O] || {}, this[O][t] = this[O][t] || []).push(f); },
    removeEventListener: function (t, f) {
      var l = this[O] && this[O][t];
      if (l) { var i = l.indexOf(f); if (i >= 0) l.splice(i, 1); }
    },
    dispatchEvent: function (ev) {
      var yo = this, l = ((this[O] && this[O][ev.type]) || []).slice();
      try { ev.target = ev.currentTarget = yo; } catch (e) {}
      var h = this['on' + ev.type];
      if (typeof h === 'function') l.unshift(h);
      l.forEach(function (f) { try { typeof f === 'function' ? f.call(yo, ev) : f.handleEvent(ev); } catch (e) { setTimeout(function () { throw e; }, 0); } });
      return true;
    }
  };
  function emitir(o, tipo) { o.dispatchEvent(new Event(tipo)); }

  // ---------------- TimeRanges ----------------
  function TimeRanges(r) { this['\u0001r'] = r || []; this.length = this['\u0001r'].length / 2; }
  TimeRanges.prototype.start = function (i) {
    if (!(i < this.length)) throw new DOMException('índice fuera de los rangos', 'IndexSizeError');
    return this['\u0001r'][2 * i];
  };
  TimeRanges.prototype.end = function (i) {
    if (!(i < this.length)) throw new DOMException('índice fuera de los rangos', 'IndexSizeError');
    return this['\u0001r'][2 * i + 1];
  };
  w.TimeRanges = TimeRanges;
  // Lo común de varios rangos (`buffered` de un vídeo con su fuente de audio y la de vídeo).
  function interseccion(listas) {
    if (!listas.length) return [];
    var r = listas[0];
    for (var k = 1; k < listas.length; k++) {
      var s = listas[k], v = [];
      for (var i = 0; i < r.length; i += 2) for (var j = 0; j < s.length; j += 2) {
        var a = Math.max(r[i], s[j]), b = Math.min(r[i + 1], s[j + 1]);
        if (b > a) v.push(a, b);
      }
      r = v;
    }
    return r;
  }

  // ---------------- MediaSource ----------------
  // Solo H.264 y AAC en MP4, con `codecs` (como Chrome, sin ellos dice que no).
  function soportado(t) {
    t = String(t).toLowerCase();
    var c = /codecs\s*=\s*"?([^";]+)"?/.exec(t);
    if (!c || !/^\s*(video|audio)\/mp4\s*(;|$)/.test(t)) return false;
    return c[1].split(',').every(function (x) { x = x.trim(); return /^avc[13]\./.test(x) || /^mp4a\.40\./.test(x); });
  }
  function SourceBufferList() { this.length = 0; }
  Object.assign(SourceBufferList.prototype, Emisor);
  function MediaSource() {
    this.readyState = 'closed';
    this['\u0001d'] = NaN;
    this.sourceBuffers = new SourceBufferList();
    this.activeSourceBuffers = this.sourceBuffers;
    this['\u0001video'] = null;
  }
  Object.assign(MediaSource.prototype, Emisor);
  MediaSource.isTypeSupported = soportado;
  MediaSource.canConstructInDedicatedWorker = false;
  Object.defineProperty(MediaSource.prototype, 'duration', {
    get: function () { return this['\u0001d']; },
    set: function (d) {
      this['\u0001d'] = +d;
      if (this['\u0001video']) emitir(this['\u0001video'], 'durationchange');
    }
  });
  MediaSource.prototype.addSourceBuffer = function (tipo) {
    if (!soportado(tipo)) throw new DOMException('tipo no soportado: ' + tipo, 'NotSupportedError');
    if (this.readyState !== 'open') throw new DOMException('el MediaSource no está abierto', 'InvalidStateError');
    var sb = new SourceBuffer(this, tipo), l = this.sourceBuffers;
    l[l.length++] = sb;
    if (/^\s*video/i.test(tipo) && this['\u0001video']) __quimera_medio_video(this['\u0001video'], sb['\u0001id']);
    emitir(l, 'addsourcebuffer');
    return sb;
  };
  MediaSource.prototype.removeSourceBuffer = function (sb) {
    var l = this.sourceBuffers, v = [];
    for (var i = 0; i < l.length; i++) if (l[i] !== sb) v.push(l[i]);
    for (i = 0; i < l.length; i++) delete l[i];
    for (i = 0; i < v.length; i++) l[i] = v[i];
    l.length = v.length;
    emitir(l, 'removesourcebuffer');
  };
  MediaSource.prototype.endOfStream = function () {
    this.readyState = 'ended';
    if (isNaN(this['\u0001d'])) {
      var fin = 0, l = this.sourceBuffers;
      for (var i = 0; i < l.length; i++) { var r = l[i]['\u0001r']; if (r.length) fin = Math.max(fin, r[r.length - 1]); }
      this.duration = fin;
    }
    emitir(this, 'sourceended');
    if (this['\u0001video']) actualizar(this['\u0001video']);
  };
  MediaSource.prototype.setLiveSeekableRange = function () {};
  MediaSource.prototype.clearLiveSeekableRange = function () {};
  w.MediaSource = MediaSource;

  function SourceBuffer(ms, tipo) {
    this['\u0001ms'] = ms;
    this['\u0001id'] = __quimera_medio_fuente(String(tipo));
    this['\u0001r'] = [];
    this.mode = 'segments';
    this.updating = false;
    this.timestampOffset = 0;
    this.appendWindowStart = 0;
    this.appendWindowEnd = Infinity;
  }
  Object.assign(SourceBuffer.prototype, Emisor);
  Object.defineProperty(SourceBuffer.prototype, 'buffered', { get: function () { return new TimeRanges(this['\u0001r'].slice()); } });
  function terminar(sb, r) {
    setTimeout(function () {
      sb['\u0001r'] = r;
      sb.updating = false;
      // Como en los navegadores, el vídeo ya sabe lo que hay antes de `updateend`.
      var v = sb['\u0001ms']['\u0001video'];
      if (v) actualizar(v);
      emitir(sb, 'update');
      emitir(sb, 'updateend');
    }, 0);
  }
  SourceBuffer.prototype.appendBuffer = function (datos) {
    if (this.updating) throw new DOMException('el SourceBuffer está ocupado', 'InvalidStateError');
    this.updating = true;
    emitir(this, 'updatestart');
    terminar(this, __quimera_medio_anadir(this['\u0001id'], datos, +this.timestampOffset || 0));
  };
  SourceBuffer.prototype.remove = function (a, b) {
    if (this.updating) throw new DOMException('el SourceBuffer está ocupado', 'InvalidStateError');
    this.updating = true;
    emitir(this, 'updatestart');
    terminar(this, __quimera_medio_quitar(this['\u0001id'], +a, +b));
  };
  SourceBuffer.prototype.abort = function () { this.updating = false; };
  SourceBuffer.prototype.changeType = function (t) {
    if (!soportado(t)) throw new DOMException('tipo no soportado: ' + t, 'NotSupportedError');
  };
  w.SourceBuffer = SourceBuffer;
  w.SourceBufferList = SourceBufferList;

  // ---------------- <video> y <audio> ----------------
  var M = w.HTMLMediaElement.prototype;
  function st(el) {
    return el[K] || (el[K] = { src: '', ms: null, paused: true, ended: false, listo: 0, volumen: 1, mudo: false,
      velocidad: 1, buscando: false, error: null, intervalo: 0, corre: false, cargado: false });
  }
  function rangosDe(el) {
    var s = st(el);
    if (!s.ms) return [];
    var l = s.ms.sourceBuffers, listas = [];
    for (var i = 0; i < l.length; i++) listas.push(l[i]['\u0001r']);
    return interseccion(listas);
  }
  function dentro(r, t) {
    for (var i = 0; i < r.length; i += 2) if (t >= r[i] - 0.1 && t < r[i + 1] - 0.05) return true;
    return false;
  }
  // Pone al día el estado del elemento con lo que hay en sus fuentes y su reloj.
  function actualizar(el) {
    var s = st(el);
    if (!s.ms) return;
    var t = __quimera_medio_tiempo(el), r = rangosDe(el), l = s.ms.sourceBuffers;
    if (s.listo < 1 && l.length && __quimera_medio_tam(l[0]['\u0001id']) !== null) {
      s.listo = 1;
      emitir(el, 'durationchange');
      emitir(el, 'loadedmetadata');
    }
    var fin = s.ms.duration;
    if (s.ms.readyState === 'ended' && !isNaN(fin) && t >= fin - 0.05) {
      if (!s.ended) {
        s.ended = true; s.corre = false;
        __quimera_medio_reloj(el, fin, false, s.velocidad);
        emitir(el, 'timeupdate');
        if (!s.paused) { s.paused = true; emitir(el, 'pause'); }
        emitir(el, 'ended');
      }
      return;
    }
    if (dentro(r, t)) {
      if (s.listo < 4) {
        if (!s.cargado) { s.cargado = true; emitir(el, 'loadeddata'); }
        s.listo = 4;
        emitir(el, 'canplay');
        emitir(el, 'canplaythrough');
      }
      if (!s.paused && !s.corre) {
        s.corre = true;
        __quimera_medio_reloj(el, t, true, s.velocidad);
        emitir(el, 'playing');
      }
    } else if (s.listo >= 3 || s.corre) {
      s.listo = Math.min(s.listo, 2);
      if (s.corre) { s.corre = false; __quimera_medio_reloj(el, t, false, s.velocidad); }
      if (!s.paused) emitir(el, 'waiting');
    }
  }
  function soltar(el) {
    var s = st(el);
    if (s.intervalo) { clearInterval(s.intervalo); s.intervalo = 0; }
    if (s.ms) s.ms['\u0001video'] = null;
    s.ms = null; s.listo = 0; s.corre = false; s.ended = false; s.cargado = false;
  }
  function asignar(el, u) {
    var s = st(el);
    soltar(el);
    s.src = u;
    __quimera_medio_reloj(el, 0, false, 1);
    if (!u) return;
    var o = registro[u];
    setTimeout(function () {
      emitir(el, 'loadstart');
      if (o instanceof MediaSource) {
        s.ms = o;
        o['\u0001video'] = el;
        o.readyState = 'open';
        emitir(o, 'sourceopen');
      } else {
        // Archivos sueltos (no MediaSource): todavía no.
        s.error = { code: 4, message: 'QUIMERA solo reproduce vídeo por MediaSource' };
        emitir(el, 'error');
      }
    }, 0);
  }
  Object.defineProperty(M, 'src', { configurable: true,
    get: function () { return st(this).src || this.getAttribute('src') || ''; },
    set: function (v) { asignar(this, String(v)); } });
  Object.defineProperty(M, 'currentSrc', { configurable: true, get: function () { return st(this).src; } });
  Object.defineProperty(M, 'srcObject', { configurable: true, get: function () { return null; }, set: function () {} });
  Object.defineProperty(M, 'currentTime', { configurable: true,
    get: function () { return __quimera_medio_tiempo(this); },
    set: function (t) {
      var el = this, s = st(el);
      t = Math.max(0, +t || 0);
      s.buscando = true; s.ended = false; s.corre = false;
      __quimera_medio_reloj(el, t, false, s.velocidad);
      emitir(el, 'seeking');
      setTimeout(function () { s.buscando = false; emitir(el, 'seeked'); emitir(el, 'timeupdate'); actualizar(el); }, 0);
    } });
  [['paused', 'paused'], ['ended', 'ended'], ['readyState', 'listo'], ['seeking', 'buscando'], ['error', 'error']].forEach(function (p) {
    Object.defineProperty(M, p[0], { configurable: true, get: function () { return st(this)[p[1]]; } });
  });
  Object.defineProperty(M, 'networkState', { configurable: true, get: function () { return st(this).src ? 2 : 0; } });
  Object.defineProperty(M, 'duration', { configurable: true, get: function () { var s = st(this); return s.ms ? s.ms.duration : NaN; } });
  Object.defineProperty(M, 'buffered', { configurable: true, get: function () { return new TimeRanges(rangosDe(this)); } });
  Object.defineProperty(M, 'seekable', { configurable: true, get: function () {
    var d = this.duration;
    return new TimeRanges(isFinite(d) ? [0, d] : []);
  } });
  Object.defineProperty(M, 'played', { configurable: true, get: function () { return new TimeRanges([]); } });
  [['volume', 'volumen', 'volumechange'], ['muted', 'mudo', 'volumechange'], ['playbackRate', 'velocidad', 'ratechange']].forEach(function (p) {
    Object.defineProperty(M, p[0], { configurable: true,
      get: function () { return st(this)[p[1]]; },
      set: function (v) {
        var s = st(this), t = __quimera_medio_tiempo(this);
        s[p[1]] = p[0] === 'muted' ? !!v : +v;
        if (p[0] === 'playbackRate') __quimera_medio_reloj(this, t, s.corre, s.velocidad);
        emitir(this, p[2]);
      } });
  });
  M.defaultPlaybackRate = 1;
  M.preservesPitch = true;
  M.play = function () {
    var el = this, s = st(el);
    if (s.ended) { s.ended = false; __quimera_medio_reloj(el, 0, false, s.velocidad); }
    if (s.paused) {
      s.paused = false;
      emitir(el, 'play');
      if (!s.intervalo) s.intervalo = setInterval(function () { actualizar(el); if (s.corre) emitir(el, 'timeupdate'); }, 250);
      setTimeout(function () { actualizar(el); }, 0);
    }
    return Promise.resolve();
  };
  M.pause = function () {
    var s = st(this);
    if (s.paused) return;
    s.paused = true;
    __quimera_medio_reloj(this, __quimera_medio_tiempo(this), false, s.velocidad);
    s.corre = false;
    if (s.intervalo) { clearInterval(s.intervalo); s.intervalo = 0; }
    emitir(this, 'timeupdate');
    emitir(this, 'pause');
  };
  M.load = function () { asignar(this, this.getAttribute('src') || ''); };
  M.canPlayType = function (t) { return soportado(t) ? 'probably' : /^\s*(video|audio)\/mp4\s*$/i.test(t) ? 'maybe' : ''; };
  M.setSinkId = function () { return Promise.resolve(); };
  M.setMediaKeys = function () { return Promise.resolve(); };
  M.mediaKeys = null;
  function fuenteVideo(el) {
    var s = st(el);
    if (!s.ms) return null;
    var l = s.ms.sourceBuffers;
    for (var i = 0; i < l.length; i++) { var t = __quimera_medio_tam(l[i]['\u0001id']); if (t && t[0]) return t; }
    return null;
  }
  if (w.HTMLVideoElement) {
    var V = w.HTMLVideoElement.prototype;
    Object.defineProperty(V, 'videoWidth', { configurable: true, get: function () { var t = fuenteVideo(this); return t ? t[0] : 0; } });
    Object.defineProperty(V, 'videoHeight', { configurable: true, get: function () { var t = fuenteVideo(this); return t ? t[1] : 0; } });
    V.getVideoPlaybackQuality = function () {
      return { totalVideoFrames: Math.floor(__quimera_medio_tiempo(this) * 30), droppedVideoFrames: 0, corruptedVideoFrames: 0, creationTime: performance.now() };
    };
  }
})(this);"#;
