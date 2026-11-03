// `Intl`: formatos que dependen del idioma, en JavaScript y sin tablas de CLDR. Cubre español e
// inglés (y separadores de algunos idiomas más); lo demás sale como en inglés. Se ejecuta tras
// preludio.js y binarios.js.
//
// - NumberFormat: decimales, agrupación, porcentajes y monedas comunes.
// - DateTimeFormat: numérico y con nombres de meses y días. La zona horaria es UTC, como el
//   resto de `Date` en QUIMERA.
// - PluralRules, ListFormat, RelativeTimeFormat, Collator, Locale y getCanonicalLocales.
(function (global) {
  var ocultar = global.__quimera_ocultar;

  function porDefecto() { return (global.navigator && global.navigator.language) || 'en-US'; }
  function idioma(loc) {
    if (Array.isArray(loc)) loc = loc[0];
    loc = loc === undefined ? porDefecto() : String(loc);
    return { locale: loc, lengua: loc.split(/[-_]/)[0].toLowerCase() };
  }
  // Separador de miles y de decimales.
  function separadores(lengua) {
    if (lengua === 'en' || lengua === 'ja' || lengua === 'zh' || lengua === 'ko' || lengua === 'he') return [',', '.'];
    if (lengua === 'fr') return [' ', ','];
    if (lengua === 'ru' || lengua === 'pl' || lengua === 'cs' || lengua === 'sv' || lengua === 'fi' || lengua === 'uk') return [' ', ','];
    if (lengua === 'de-ch') return ['’', '.'];
    return ['.', ','];
  }
  function clase(nombre, C) {
    C.prototype[Symbol.toStringTag] = 'Intl.' + nombre;
    ocultar.push(C.prototype);
    return C;
  }
  // Las clases de Intl se pueden llamar con o sin `new`.
  function crear(C, args) { var o = Object.create(C.prototype); C.apply(o, args); return o; }

  // ---------------- NumberFormat ----------------
  var SIMBOLOS = { USD: '$', EUR: '€', GBP: '£', JPY: '¥', CNY: 'CN¥', INR: '₹', KRW: '₩', BRL: 'R$', MXN: 'MX$' };
  function NumberFormat(loc, op) {
    if (!(this instanceof NumberFormat)) return crear(NumberFormat, arguments);
    var l = idioma(loc);
    op = op || {};
    this.locale = l.locale;
    this.lengua = l.lengua;
    this.estilo = op.style || 'decimal';
    this.moneda = op.currency ? String(op.currency).toUpperCase() : undefined;
    var decimalesMoneda = this.moneda === 'JPY' || this.moneda === 'KRW' ? 0 : 2;
    var min = op.minimumFractionDigits, max = op.maximumFractionDigits;
    var defMin = this.estilo === 'currency' ? decimalesMoneda : 0;
    var defMax = this.estilo === 'currency' ? decimalesMoneda : this.estilo === 'percent' ? 0 : 3;
    this.min = min === undefined ? Math.min(defMin, max === undefined ? defMin : max) : min;
    this.max = max === undefined ? Math.max(this.min, defMax) : max;
    this.agrupar = op.useGrouping !== false;
  }
  NumberFormat.prototype.format = function (n) {
    n = Number(n);
    if (n !== n) return 'NaN';
    var neg = n < 0 || (n === 0 && 1 / n < 0);
    n = Math.abs(n);
    if (this.estilo === 'percent') n *= 100;
    var cuerpo;
    if (n === Infinity) cuerpo = '∞';
    else {
      var t = n.toFixed(this.max), partes = t.split('.'), ent = partes[0], frac = partes[1] || '';
      while (frac.length > this.min && frac.charAt(frac.length - 1) === '0') frac = frac.slice(0, -1);
      var sep = separadores(this.lengua);
      // En español no se agrupan los números de cuatro cifras (1234, pero 12.345).
      var agrupar = this.agrupar && !(this.lengua === 'es' && ent.length <= 4);
      if (agrupar) ent = ent.replace(/\B(?=(\d{3})+(?!\d))/g, sep[0]);
      cuerpo = frac ? ent + sep[1] + frac : ent;
    }
    var signo = neg ? '-' : '';
    if (this.estilo === 'percent') return signo + cuerpo + (this.lengua === 'en' ? '%' : ' %');
    if (this.estilo === 'currency') {
      var s = SIMBOLOS[this.moneda] || this.moneda;
      if (this.lengua === 'es' && this.moneda === 'USD') s = 'US$';
      return this.lengua === 'en' ? signo + s + cuerpo : signo + cuerpo + ' ' + s;
    }
    return signo + cuerpo;
  };
  NumberFormat.prototype.formatToParts = function (n) { return [{ type: 'literal', value: this.format(n) }]; };
  NumberFormat.prototype.resolvedOptions = function () {
    return { locale: this.locale, numberingSystem: 'latn', style: this.estilo, currency: this.moneda,
      minimumFractionDigits: this.min, maximumFractionDigits: this.max, useGrouping: this.agrupar };
  };
  clase('NumberFormat', NumberFormat);

  // ---------------- DateTimeFormat ----------------
  var NOMBRES = {
    en: { meses: ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'],
      dias: ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'] },
    es: { meses: ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre'],
      dias: ['domingo', 'lunes', 'martes', 'miércoles', 'jueves', 'viernes', 'sábado'] }
  };
  function DateTimeFormat(loc, op) {
    if (!(this instanceof DateTimeFormat)) return crear(DateTimeFormat, arguments);
    var l = idioma(loc);
    op = op || {};
    this.locale = l.locale;
    this.lengua = NOMBRES[l.lengua] ? l.lengua : 'en';
    var campos = ['weekday', 'year', 'month', 'day', 'hour', 'minute', 'second'];
    var alguno = campos.some(function (k) { return op[k] !== undefined; });
    this.op = {};
    for (var k in op) this.op[k] = op[k];
    if (op.dateStyle || op.timeStyle) {
      var ds = op.dateStyle, ts = op.timeStyle;
      if (ds === 'full') { this.op.weekday = 'long'; this.op.month = 'long'; }
      if (ds === 'long') this.op.month = 'long';
      if (ds === 'medium') this.op.month = 'short';
      if (ds) { this.op.year = 'numeric'; this.op.day = 'numeric'; this.op.month = this.op.month || 'numeric'; }
      if (ts) { this.op.hour = 'numeric'; this.op.minute = '2-digit'; if (ts !== 'short') this.op.second = '2-digit'; }
    } else if (!alguno) {
      this.op.year = 'numeric'; this.op.month = 'numeric'; this.op.day = 'numeric';
    }
  }
  function dos(n) { return (n < 10 ? '0' : '') + n; }
  DateTimeFormat.prototype.format = function (fecha) {
    var d = fecha === undefined ? new Date() : new Date(+fecha);
    if (d.getTime() !== d.getTime()) throw new RangeError('Invalid time value');
    var o = this.op, es = this.lengua === 'es', N = NOMBRES[this.lengua];
    var a = d.getUTCFullYear(), m = d.getUTCMonth(), dia = d.getUTCDate();
    var fechaTxt = '';
    if (o.month === 'long' || o.month === 'short') {
      var mes = o.month === 'long' ? N.meses[m] : N.meses[m].slice(0, 3);
      if (es) fechaTxt = [o.day ? dia : '', mes, o.year ? a : ''].filter(String).join(' de ');
      else fechaTxt = mes + (o.day ? ' ' + dia : '') + (o.year ? (o.day ? ', ' : ' ') + a : '');
    } else if (o.year || o.month || o.day) {
      var p = [];
      var md = o.month ? (o.month === '2-digit' ? dos(m + 1) : m + 1) : null;
      var dd = o.day ? (o.day === '2-digit' ? dos(dia) : dia) : null;
      if (es) { if (dd !== null) p.push(dd); if (md !== null) p.push(md); } else { if (md !== null) p.push(md); if (dd !== null) p.push(dd); }
      if (o.year) p.push(o.year === '2-digit' ? dos(a % 100) : a);
      fechaTxt = p.join('/');
    }
    if (o.weekday) {
      var nd = o.weekday === 'long' ? N.dias[d.getUTCDay()] : N.dias[d.getUTCDay()].slice(0, 3);
      fechaTxt = fechaTxt ? nd + ', ' + fechaTxt : nd;
    }
    var horaTxt = '';
    if (o.hour || o.minute) {
      var h = d.getUTCHours(), mi = dos(d.getUTCMinutes()), s = o.second ? ':' + dos(d.getUTCSeconds()) : '';
      var doce = o.hour12 !== undefined ? o.hour12 : !es;
      if (doce) horaTxt = ((h % 12) || 12) + ':' + mi + s + (es ? ' ' : ' ') + (h < 12 ? (es ? 'a. m.' : 'AM') : (es ? 'p. m.' : 'PM'));
      else horaTxt = (o.hour === '2-digit' || es ? dos(h) : h) + ':' + mi + s;
    }
    return fechaTxt && horaTxt ? fechaTxt + ', ' + horaTxt : fechaTxt || horaTxt;
  };
  DateTimeFormat.prototype.formatToParts = function (f) { return [{ type: 'literal', value: this.format(f) }]; };
  DateTimeFormat.prototype.formatRange = function (a, b) { return this.format(a) + ' – ' + this.format(b); };
  DateTimeFormat.prototype.resolvedOptions = function () {
    var r = { locale: this.locale, calendar: 'gregory', numberingSystem: 'latn', timeZone: 'UTC' };
    for (var k in this.op) r[k] = this.op[k];
    return r;
  };
  clase('DateTimeFormat', DateTimeFormat);

  // ---------------- PluralRules, ListFormat, RelativeTimeFormat ----------------
  function PluralRules(loc, op) {
    if (!(this instanceof PluralRules)) throw new TypeError("Intl.PluralRules necesita 'new'");
    this.locale = idioma(loc).locale;
    this.lengua = idioma(loc).lengua;
    this.tipo = (op && op.type) || 'cardinal';
  }
  PluralRules.prototype.select = function (n) {
    n = Number(n);
    if (this.tipo === 'ordinal') {
      if (this.lengua !== 'en') return 'other';
      var d = n % 10, c = n % 100;
      return d === 1 && c !== 11 ? 'one' : d === 2 && c !== 12 ? 'two' : d === 3 && c !== 13 ? 'few' : 'other';
    }
    return n === 1 ? 'one' : 'other';
  };
  PluralRules.prototype.resolvedOptions = function () { return { locale: this.locale, type: this.tipo }; };
  clase('PluralRules', PluralRules);

  function ListFormat(loc, op) {
    if (!(this instanceof ListFormat)) throw new TypeError("Intl.ListFormat necesita 'new'");
    this.lengua = idioma(loc).lengua;
    this.tipo = (op && op.type) || 'conjunction';
  }
  ListFormat.prototype.format = function (lista) {
    var l = Array.from(lista, String), es = this.lengua === 'es';
    if (l.length < 2) return l.join('');
    var y = this.tipo === 'disjunction' ? (es ? ' o ' : ' or ') : this.tipo === 'unit' ? ', ' : (es ? ' y ' : ' and ');
    if (l.length === 2) return l[0] + y + l[1];
    var ultima = es || this.tipo === 'unit' ? y : ',' + y;
    return l.slice(0, -1).join(', ') + ultima + l[l.length - 1];
  };
  clase('ListFormat', ListFormat);

  var UNIDADES = {
    en: { second: ['second', 'seconds'], minute: ['minute', 'minutes'], hour: ['hour', 'hours'], day: ['day', 'days'],
      week: ['week', 'weeks'], month: ['month', 'months'], quarter: ['quarter', 'quarters'], year: ['year', 'years'] },
    es: { second: ['segundo', 'segundos'], minute: ['minuto', 'minutos'], hour: ['hora', 'horas'], day: ['día', 'días'],
      week: ['semana', 'semanas'], month: ['mes', 'meses'], quarter: ['trimestre', 'trimestres'], year: ['año', 'años'] }
  };
  function RelativeTimeFormat(loc, op) {
    if (!(this instanceof RelativeTimeFormat)) throw new TypeError("Intl.RelativeTimeFormat necesita 'new'");
    this.lengua = UNIDADES[idioma(loc).lengua] ? idioma(loc).lengua : 'en';
    this.numerico = (op && op.numeric) || 'always';
  }
  RelativeTimeFormat.prototype.format = function (v, unidad) {
    v = Number(v);
    unidad = String(unidad).replace(/s$/, '');
    var es = this.lengua === 'es', U = UNIDADES[this.lengua][unidad];
    if (!U) throw new RangeError('unidad no válida: ' + unidad);
    if (this.numerico === 'auto' && unidad === 'day' && Math.abs(v) <= 1) {
      return v === 0 ? (es ? 'hoy' : 'today') : v > 0 ? (es ? 'mañana' : 'tomorrow') : (es ? 'ayer' : 'yesterday');
    }
    var n = Math.abs(v), txt = new NumberFormat(this.lengua).format(n) + ' ' + U[n === 1 ? 0 : 1];
    if (v < 0 || (v === 0 && 1 / v < 0)) return es ? 'hace ' + txt : txt + ' ago';
    return es ? 'dentro de ' + txt : 'in ' + txt;
  };
  clase('RelativeTimeFormat', RelativeTimeFormat);

  // ---------------- Collator, Locale ----------------
  function Collator(loc, op) {
    if (!(this instanceof Collator)) return crear(Collator, arguments);
    this.locale = idioma(loc).locale;
    this.base = !!op && (op.sensitivity === 'base' || op.sensitivity === 'accent');
    this.numerico = !!op && !!op.numeric;
    var self = this;
    this.compare = function (a, b) {
      a = String(a); b = String(b);
      if (self.base) { a = a.toLowerCase(); b = b.toLowerCase(); }
      if (self.numerico && /^\d+$/.test(a) && /^\d+$/.test(b)) return Math.sign(Number(a) - Number(b));
      return a.localeCompare(b);
    };
  }
  Collator.prototype.resolvedOptions = function () { return { locale: this.locale, usage: 'sort', sensitivity: this.base ? 'base' : 'variant' }; };
  clase('Collator', Collator);

  function Locale(etiqueta) {
    if (!(this instanceof Locale)) throw new TypeError("Intl.Locale necesita 'new'");
    var p = String(etiqueta).split(/[-_]/);
    this.language = p[0].toLowerCase();
    this.region = p.length > 1 && /^[a-z]{2}$|^\d{3}$/i.test(p[p.length - 1]) ? p[p.length - 1].toUpperCase() : undefined;
    this.baseName = this.language + (this.region ? '-' + this.region : '');
  }
  Locale.prototype.toString = function () { return this.baseName; };
  Locale.prototype.maximize = function () { return this; };
  Locale.prototype.minimize = function () { return this; };
  clase('Locale', Locale);


  // Intl.Segmenter: letras (por punto de código; las marcas combinantes, los selectores de
  // variante, los tonos de piel y lo unido con ZWJ van con la letra anterior), palabras (con
  // `isWordLike`) y frases (hasta `.`, `!` o `?` y los blancos que siguen). Sin los diccionarios
  // de las lenguas sin espacios (chino, japonés, tailandés).
  function puntos(s) {
    var v = [];
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i), largo = 1;
      if (c >= 0xD800 && c < 0xDC00 && i + 1 < s.length) {
        var d = s.charCodeAt(i + 1);
        if (d >= 0xDC00 && d < 0xE000) { c = 0x10000 + ((c - 0xD800) << 10) + (d - 0xDC00); largo = 2; }
      }
      v.push({ c: c, i: i, t: s.substr(i, largo) });
      i += largo - 1;
    }
    return v;
  }
  function seUne(c) {
    return (c >= 0x300 && c <= 0x36F) || (c >= 0x1AB0 && c <= 0x1AFF) || (c >= 0x20D0 && c <= 0x20FF) ||
      (c >= 0xFE00 && c <= 0xFE0F) || (c >= 0xFE20 && c <= 0xFE2F) || (c >= 0x1F3FB && c <= 0x1F3FF) ||
      (c >= 0xE0100 && c <= 0xE01EF) || c === 0x200D;
  }
  function deLetra(t) { return /[0-9_]/.test(t) || t.toLowerCase() !== t.toUpperCase() || t.charCodeAt(0) > 0x2E7F; }
  function letras(s) {
    var v = [], p = puntos(s);
    for (var k = 0; k < p.length; k++) {
      var previo = v[v.length - 1];
      var unido = previo && (seUne(p[k].c) || (k > 0 && p[k - 1].c === 0x200D));
      if (unido) previo.segment += p[k].t; else v.push({ segment: p[k].t, index: p[k].i });
    }
    return v;
  }
  function palabras(s) {
    var v = [];
    letras(s).forEach(function (g) {
      var tipo = /^\s+$/.test(g.segment) ? 'b' : deLetra(g.segment) ? 'p' : 's';
      var previo = v[v.length - 1];
      if (previo && tipo !== 's' && previo.tipo === tipo) previo.segment += g.segment;
      else v.push({ segment: g.segment, index: g.index, tipo: tipo });
    });
    return v.map(function (x) { return { segment: x.segment, index: x.index, isWordLike: x.tipo === 'p' }; });
  }
  function frases(s) {
    var v = [], re = /[^.!?]*(?:[.!?]+|$)\s*/g, m;
    while ((m = re.exec(s)) && m[0] !== '') { v.push({ segment: m[0], index: m.index }); }
    return v;
  }
  function Segmenter(loc, op) {
    if (!(this instanceof Segmenter)) return crear(Segmenter, arguments);
    this.locale = idioma(loc).locale;
    this.granularity = (op && op.granularity) || 'grapheme';
  }
  Segmenter.prototype.segment = function (s) {
    s = String(s);
    var g = this.granularity, partes = g === 'word' ? palabras(s) : g === 'sentence' ? frases(s) : letras(s);
    partes.forEach(function (p) { p.input = s; if (g !== 'word') delete p.isWordLike; });
    var segmentos = {
      containing: function (i) {
        i = i === undefined ? 0 : Number(i);
        for (var k = 0; k < partes.length; k++) if (i >= partes[k].index && i < partes[k].index + partes[k].segment.length) return partes[k];
        return undefined;
      }
    };
    segmentos[Symbol.iterator] = function () { return partes.slice()[Symbol.iterator](); };
    return segmentos;
  };
  Segmenter.prototype.resolvedOptions = function () { return { locale: this.locale, granularity: this.granularity }; };
  clase('Segmenter', Segmenter);

  var Intl = {
    Segmenter: Segmenter, NumberFormat: NumberFormat, DateTimeFormat: DateTimeFormat, PluralRules: PluralRules, ListFormat: ListFormat,
    RelativeTimeFormat: RelativeTimeFormat, Collator: Collator, Locale: Locale,
    getCanonicalLocales: function (l) { return (l === undefined ? [] : Array.isArray(l) ? l : [l]).map(String); },
    supportedValuesOf: function () { return []; }
  };
  [NumberFormat, DateTimeFormat, PluralRules, ListFormat, RelativeTimeFormat, Collator, Segmenter].forEach(function (C) {
    C.supportedLocalesOf = function (l) { return Intl.getCanonicalLocales(l); };
  });
  Intl[Symbol.toStringTag] = 'Intl';
  global.Intl = Intl;

  // Los `toLocale...` de números y fechas, sobre Intl.
  Number.prototype.toLocaleString = function (loc, op) { return new NumberFormat(loc, op).format(this); };
  Date.prototype.toLocaleDateString = function (loc, op) { return new DateTimeFormat(loc, op).format(this); };
  Date.prototype.toLocaleTimeString = function (loc, op) {
    return new DateTimeFormat(loc, op || { hour: 'numeric', minute: '2-digit', second: '2-digit' }).format(this);
  };
  Date.prototype.toLocaleString = function (loc, op) {
    return new DateTimeFormat(loc, op || { year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: '2-digit', second: '2-digit' }).format(this);
  };
})(this);
