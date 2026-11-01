function a(e, n) { for (const n in e) {} return n; }
function b(e) { var n = 'fuera'; for (const n in e) {} return n; }
function c(e) { let n = 'fuera'; { const n = 'dentro'; } return n; }
function d(e) { let n = 'fuera'; for (let n = 0; n < 2; n++) {} return n; }
function f(e) { let n = 'fuera'; for (const n of [1]) {} return n; }
function g() { let x = 1; { let x = 2; var h = () => x; } return [x, h()].join(); }
function k() { let e = 'fuera'; try { throw 'dentro'; } catch (e) { e = 'cambiado'; } return e; }
function r(e, ...n) { var t = {}; for (const n in e) t[n] = e[n]; return n.forEach(function (x) { t.extra = x; }), JSON.stringify(t); }
console.log(a({x: 1}, 'param'), b({x: 1}), c(), d(), f(), g(), k(), r({a: 1}, 'b'));
