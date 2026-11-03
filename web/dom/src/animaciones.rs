//! `element.animate()` (Web Animations), sin fotogramas intermedios: la animación espera su
//! retraso y su duración con un temporizador y entonces termina. Si su `fill` es `forwards` o
//! `both`, el último fotograma queda en el estilo del elemento (QUIMERA no guarda una capa de
//! efectos aparte). Avisan `onfinish`, la promesa `finished` y el evento `finish`.
//!
//! Así las páginas que muestran sus bloques con una animación de entrada (de `opacity: 0` a 1,
//! como las de Framer Motion en GitHub) acaban visibles, como en un navegador.

/// El código (tras `interfaces.rs`).
pub(crate) const ANIMACIONES_JS: &str = r#"(function (w, d, P) {
  var E = P.p_elemento;
  var NO_SON_ESTILO = { offset: 1, easing: 1, composite: 1, computedOffset: 1 };
  // `[{opacity: 0}, {opacity: 1}]` o `{opacity: [0, 1]}`: la lista de fotogramas.
  function fotogramas(k) {
    if (!k) return [];
    if (Array.isArray(k)) return k;
    var v = [], n = 0;
    Object.keys(k).forEach(function (p) { n = Math.max(n, [].concat(k[p]).length); });
    for (var i = 0; i < n; i++) {
      var f = {};
      Object.keys(k).forEach(function (p) { var l = [].concat(k[p]); f[p] = l[Math.min(i, l.length - 1)]; });
      v.push(f);
    }
    return v;
  }
  function aplicar(el, f) {
    if (!f) return;
    Object.keys(f).forEach(function (p) { if (!NO_SON_ESTILO[p]) { try { el.style[p] = String(f[p]); } catch (e) {} } });
  }
  function Animation(el, k, op) {
    var a = this;
    op = typeof op === 'number' ? { duration: op } : (op || {});
    a.effect = { target: el, getKeyframes: function () { return k.slice(); },
      getTiming: function () { return op; }, getComputedTiming: function () { return { progress: a.playState === 'finished' ? 1 : 0 }; } };
    a.id = op.id || ''; a.playbackRate = 1; a.startTime = performance.now(); a.currentTime = 0;
    a.playState = 'running'; a.pending = false; a.onfinish = null; a.oncancel = null; a.timeline = d.timeline || null;
    a['\u0001k'] = k; a['\u0001op'] = op; a['\u0001el'] = el; a['\u0001esc'] = {};
    a.finished = new Promise(function (ok, mal) { a['\u0001ok'] = ok; a['\u0001mal'] = mal; });
    a.finished.catch(function () {});
    a.ready = Promise.resolve(a);
    var veces = op.iterations === Infinity ? 0 : (op.iterations === undefined ? 1 : op.iterations);
    var ms = (op.delay || 0) + (Number(op.duration) || 0) * veces + (op.endDelay || 0);
    // Infinita: no termina nunca (como en un navegador).
    if (op.iterations !== Infinity) a['\u0001t'] = setTimeout(function () { a.finish(); }, Math.max(0, ms));
    (el['\u0001anims'] = el['\u0001anims'] || []).push(a);
  }
  Animation.prototype.finish = function () {
    if (this.playState === 'finished' || this.playState === 'idle') return;
    clearTimeout(this['\u0001t']);
    var fill = this['\u0001op'].fill;
    if (fill === 'forwards' || fill === 'both') this.commitStyles();
    this.playState = 'finished';
    this.currentTime = (this['\u0001op'].duration || 0);
    quitar(this);
    // Como en los navegadores: primero la promesa (microtarea) y luego el evento, en otra tarea.
    this['\u0001ok'](this);
    var a = this, ev = { type: 'finish', target: this, currentTime: this.currentTime, timelineTime: performance.now() };
    setTimeout(function () {
      if (typeof a.onfinish === 'function') { try { a.onfinish(ev); } catch (e) { console.error(String(e && e.stack || e)); } }
      (a['\u0001esc'].finish || []).forEach(function (f) { try { f.call(a, ev); } catch (e) {} });
    }, 0);
  };
  Animation.prototype.cancel = function () {
    if (this.playState === 'idle') return;
    clearTimeout(this['\u0001t']);
    this.playState = 'idle';
    quitar(this);
    if (typeof this.oncancel === 'function') { try { this.oncancel({ type: 'cancel', target: this }); } catch (e) {} }
    this['\u0001mal'](new DOMException('La animación se canceló', 'AbortError'));
  };
  Animation.prototype.commitStyles = function () { var k = this['\u0001k']; aplicar(this['\u0001el'], k[k.length - 1]); };
  Animation.prototype.play = function () { if (this.playState === 'paused') this.playState = 'running'; };
  Animation.prototype.pause = function () { if (this.playState === 'running') this.playState = 'paused'; };
  Animation.prototype.reverse = function () { this.playbackRate = -this.playbackRate; };
  Animation.prototype.persist = function () {};
  Animation.prototype.updatePlaybackRate = function (r) { this.playbackRate = r; };
  Animation.prototype.addEventListener = function (t, f) { (this['\u0001esc'][t] = this['\u0001esc'][t] || []).push(f); };
  Animation.prototype.removeEventListener = function (t, f) {
    this['\u0001esc'][t] = (this['\u0001esc'][t] || []).filter(function (g) { return g !== f; });
  };
  function quitar(a) {
    var l = a['\u0001el']['\u0001anims'] || [], i = l.indexOf(a);
    if (i >= 0) l.splice(i, 1);
  }
  E.animate = function (k, op) { return new Animation(this, fotogramas(k), op); };
  E.getAnimations = function () { return (this['\u0001anims'] || []).slice(); };
  d.getAnimations = function () { return []; };
  w.Animation = Animation;
  w.KeyframeEffect = function KeyframeEffect(el, k, op) { this.target = el; this['\u0001k'] = fotogramas(k); this['\u0001op'] = op; };
  __quimera_no_enumerar(Animation.prototype);
})(this, this.document, this.__quimera_protos);"#;
