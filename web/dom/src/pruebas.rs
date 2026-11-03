//! Pruebas del DOM: scripts de verdad sobre documentos pequeños. Los resultados esperados son
//! los de Chromium con el mismo HTML.

use crate::Dom;
use alloc::string::String;
use alloc::vec::Vec;
use quimera_html::analizar;

fn dom(html: &str) -> Dom {
    Dom::nuevo(analizar(html), "https://ejemplo.com/a")
}

/// Ejecuta `js` y devuelve lo escrito con `console.log` (y los errores, si hubo).
fn correr(d: &mut Dom, js: &str) -> Vec<String> {
    d.ejecutar(js);
    let mut r = d.consola();
    r.extend(d.errores().into_iter().map(|e| alloc::format!("ERROR {e}")));
    r
}

#[test]
fn leer_el_arbol() {
    let mut d = dom("<title> Hola </title><div id=a class='x y'><p>uno</p><!--c--><p>dos <b>tres</b></p></div>");
    let r = correr(
        &mut d,
        "var a = document.getElementById('a');
         console.log(a.tagName, a.id, a.className, a.children.length, a.childNodes.length);
         console.log(a.firstChild.textContent, a.lastChild.innerHTML, a.childNodes[1].nodeType);
         console.log(a === document.querySelector('div.x'), a.parentNode === document.body);
         console.log(document.title, document.documentElement.nodeName, document.nodeType);
         console.log(a.firstElementChild.nextElementSibling.textContent, a.getAttribute('class'));
         console.log(document.querySelectorAll('#a p').length, document.getElementsByTagName('P').length);
         console.log(a.querySelector('b').closest('div') === a, a.matches('.y'), typeof a);",
    );
    assert_eq!(
        r,
        [
            "DIV a x y 2 3",
            "uno dos <b>tres</b> 8",
            "true true",
            "Hola HTML 9",
            "dos tres x y",
            "2 2",
            "true true object",
        ]
    );
}

#[test]
fn cambiar_el_arbol() {
    let mut d = dom("<ul id=l><li>a</li></ul>");
    let r = correr(
        &mut d,
        "var l = document.getElementById('l');
         var li = document.createElement('LI');
         li.textContent = 'b';
         l.appendChild(li);
         l.insertBefore(document.createElement('li'), l.firstChild).id = 'cero';
         li.setAttribute('data-x', '1');
         li.className = 'nuevo';
         li.style.backgroundColor = 'red';
         li.style.setProperty('color', 'blue');
         li.classList.add('otra');
         li.classList.toggle('nuevo');
         console.log(l.innerHTML);
         console.log(li.style.backgroundColor, li.classList.contains('otra'), l.children.length);
         l.removeChild(l.firstChild);
         l.lastChild.remove();
         l.innerHTML += '<li>c</li>';
         console.log(l.innerHTML, li.parentNode);",
    );
    assert_eq!(
        r,
        [
            "<li id=\"cero\"></li><li>a</li><li data-x=\"1\" class=\"otra\" style=\"background-color: red; color: blue;\">b</li>",
            "red true 3",
            "<li>a</li><li>c</li> null",
        ]
    );
    assert!(d.tomar_cambios());
    assert!(!d.tomar_cambios());
    let doc = d.documento();
    assert_eq!(doc.texto(doc.buscar("ul").expect("ul")), "ac");
}

#[test]
fn promesas_tras_cada_script_y_almacen() {
    let mut d = dom("<p id=p>a</p>");
    let r = correr(
        &mut d,
        "Promise.resolve().then(function () { document.getElementById('p').textContent = 'b'; console.log('microtarea'); });
         console.log('script', document.getElementById('p').textContent);
         localStorage.setItem('k', 1);
         console.log(localStorage.getItem('k'), typeof localStorage.getItem('k'), localStorage.getItem('no'), localStorage.length, 'localStorage' in window);",
    );
    assert_eq!(r, ["script a", "1 string null 1 true", "microtarea"]);
    assert_eq!(d.documento().texto(d.documento().buscar("p").expect("p")), "b");
}

#[test]
fn scripts_eventos_y_temporizadores() {
    let mut d = dom(
        "<a id=e href='/x' onclick='console.log(\"atributo\", this.id, event.type)'>enlace</a>
         <script>
           var e = document.getElementById('e');
           e.addEventListener('click', function (ev) { console.log('listener', ev.target === e); ev.preventDefault(); });
           document.body.addEventListener('click', function (ev) { console.log('sube', ev.currentTarget.tagName); });
           window.addEventListener('load', function () { console.log('load'); });
           document.addEventListener('DOMContentLoaded', function () { console.log('listo'); });
           var t = setTimeout(function () { console.log('nunca'); }, 50);
           setTimeout(function () { console.log('a los 100'); clearTimeout(t); }, 10);
           var n = 0, i = setInterval(function () { if (++n == 3) { clearInterval(i); console.log('tres veces'); } }, 20);
           noExiste();
         </script>
         <script src=externo.js></script>",
    );
    d.ejecutar_scripts(&|src: &str| (src == "externo.js").then(|| String::from("console.log('externo')")));
    assert_eq!(d.consola(), ["externo", "listo", "load"]);
    assert_eq!(d.errores().len(), 1, "noExiste() lanza, pero los demás scripts siguen");
    let enlace = d
        .documento()
        .descendientes(quimera_html::Documento::RAIZ)
        .find(|&n| d.documento().elemento(n).is_some_and(|e| e.nombre == "a"));
    assert!(d.clic(enlace.expect("a")), "preventDefault");
    assert_eq!(d.consola(), ["atributo e click", "listener true", "sube BODY"]);
    assert_eq!(d.proximo_temporizador(), Some(10.0));
    for ms in [10.0, 20.0, 40.0, 60.0, 100.0] {
        d.correr_temporizadores(ms);
    }
    assert_eq!(d.consola(), ["a los 100", "tres veces"]);
    assert_eq!(d.proximo_temporizador(), None);
}

#[test]
fn fragmentos() {
    let mut d = dom("<ul id=l><li>a</li></ul>");
    let r = correr(
        &mut d,
        "var f = document.createDocumentFragment();
         ['b', 'c'].forEach(function (t) { var li = document.createElement('li'); li.textContent = t; f.appendChild(li); });
         console.log(f.nodeType, f.nodeName, f.childNodes.length);
         var l = document.getElementById('l');
         l.appendChild(f);
         console.log(l.innerHTML, f.childNodes.length);",
    );
    assert_eq!(r, ["11 #document-fragment 2", "<li>a</li><li>b</li><li>c</li> 0"]);
}

#[test]
fn modulos() {
    use crate::RespuestaJs;
    let mut d = Dom::nuevo(
        analizar(
            "<p id=p>antes</p>
             <script type=module>
               import { suma, PI as pi } from './util.js';
               import contar, * as todo from './sub/contador.js';
               document.getElementById('p').textContent = `${suma(2, 3)} ${pi} ${contar()} ${contar()} ${todo.nombre}`;
               const dinamico = await import('./util.js');
               console.log('mismo módulo', dinamico.suma === suma, import.meta === undefined);
             </script>
             <script nomodule>console.log('no debe correr');</script>",
        ),
        "https://ejemplo.com/app/index.html",
    );
    d.ejecutar_scripts(&|_| None);
    let archivos = [
        ("https://ejemplo.com/app/util.js", "export function suma(a, b) { return a + b; }\nexport const PI = 3;"),
        (
            "https://ejemplo.com/app/sub/contador.js",
            "import { suma } from '../util.js';\nlet n = 0;\nexport const nombre = 'contador';\nexport default function () { n = suma(n, 1); return n; }",
        ),
    ];
    for _ in 0..10 {
        let pedidas = d.tomar_peticiones();
        if pedidas.is_empty() {
            break;
        }
        for p in pedidas {
            let url = p.url.clone();
            let cuerpo = archivos.iter().find(|(u, _)| *u == url).map(|(_, c)| String::from(*c));
            let r = cuerpo.map(|c| RespuestaJs { estado: 200, url: url.clone(), cabeceras: Vec::new(), cuerpo: c });
            d.responder(p.id, r.ok_or_else(|| alloc::format!("404 {url}")));
        }
    }
    assert_eq!(d.consola(), ["mismo módulo true false"]);
    assert!(d.errores().is_empty(), "{:?}", d.errores());
    let doc = d.documento();
    assert_eq!(doc.texto(doc.buscar("p").expect("p")), "5 3 1 2 contador");
}

#[test]
fn fetch_xhr_y_scripts_añadidos() {
    use crate::RespuestaJs;
    let mut d = dom("<p id=p></p>");
    d.ejecutar(
        "fetch('/datos.json', {method: 'post', body: 'x=1', headers: {'X-Prueba': 'si'}})
           .then(function (r) { console.log('fetch', r.ok, r.status, r.headers.get('Content-Type')); return r.json(); })
           .then(function (j) { console.log('json', j.a); });
         fetch('https://otro.example/').catch(function (e) { console.log('falló', e instanceof TypeError); });
         var x = new XMLHttpRequest();
         x.open('GET', 'texto.txt');
         x.onload = function () { console.log('xhr', x.status, x.responseText, x.getResponseHeader('content-type')); };
         x.send();
         var s = document.createElement('script');
         s.src = 'extra.js';
         s.onload = function () { console.log('script cargado', window.extra); };
         document.body.appendChild(s);
         var en_linea = document.createElement('script');
         en_linea.textContent = 'console.log(\"en línea\")';
         document.body.appendChild(en_linea);",
    );
    assert_eq!(d.consola(), ["en línea"]);
    let pedidas = d.tomar_peticiones();
    let resumen: Vec<String> = pedidas
        .iter()
        .map(|p| alloc::format!("{} {} {:?} {:?} {}", p.metodo, p.url, p.cuerpo, p.cabeceras, p.es_script))
        .collect();
    assert_eq!(
        resumen,
        [
            r#"POST /datos.json Some("x=1") [("x-prueba", "si")] false"#,
            "GET https://otro.example/ None [] false",
            "GET texto.txt None [] false",
            "GET extra.js None [] true",
        ]
    );
    let resp = |cuerpo: &str, tipo: &str| RespuestaJs {
        estado: 200,
        url: String::from("https://ejemplo.com/x"),
        cabeceras: alloc::vec![(String::from("content-type"), String::from(tipo))],
        cuerpo: String::from(cuerpo),
    };
    d.responder(pedidas[0].id, Ok(resp("{\"a\": 5}", "application/json")));
    d.responder(pedidas[1].id, Err(String::from("CORS")));
    d.responder(pedidas[2].id, Ok(resp("hola", "text/plain")));
    d.responder(pedidas[3].id, Ok(resp("window.extra = 'sí';", "text/javascript")));
    assert_eq!(
        d.consola(),
        ["fetch true 200 application/json", "json 5", "falló true", "xhr 200 hola text/plain", "script cargado sí"]
    );
    assert!(d.errores().is_empty());
}

#[test]
fn url_y_parametros() {
    let mut d = dom("");
    let r = correr(
        &mut d,
        "const u = new URL('https://user:pw@Ejemplo.COM:8080/a/b/../c?x=1&y=dos+tres#frag');
         console.log(u.protocol, u.username, u.hostname, u.port, u.host, u.pathname, u.search, u.hash, u.origin);
         console.log(u.searchParams.get('y'), u.searchParams.has('x'), [...u.searchParams.keys()].join());
         u.searchParams.set('x', 'nuevo valor'); u.searchParams.append('z', 'ñ');
         console.log(u.search, u.href);
         console.log(new URL('../img/a.png', 'https://e.com/docs/x/y.html').href, new URL('?q=1', 'https://e.com/p').href, new URL('//cdn.e.com/l.js', 'https://e.com/').href, new URL('https://e.com:443/').port);
         const p = new URLSearchParams({a: '1', b: 'x y'}); p.append('a', '2');
         console.log(p.toString(), p.getAll('a').join(), URL.canParse('nada'), String(new URL('http://e.com')));",
    );
    assert_eq!(
        r,
        [
            "https: user ejemplo.com 8080 ejemplo.com:8080 /a/c ?x=1&y=dos+tres #frag https://ejemplo.com:8080",
            "dos tres true x,y",
            "?x=nuevo+valor&y=dos+tres&z=%C3%B1 https://user:pw@ejemplo.com:8080/a/c?x=nuevo+valor&y=dos+tres&z=%C3%B1#frag",
            "https://e.com/docs/img/a.png https://e.com/p?q=1 https://cdn.e.com/l.js ",
            "a=1&b=x+y&a=2 1,2 false http://e.com/",
        ]
    );
}

/// Las clases del DOM, `customElements` (con un elemento que ya estaba en la página), `template`,
/// `CustomEvent` y `dispatchEvent` (`clases_dom.js`), con la salida de Chromium.
#[test]
fn clases_del_dom_y_elementos_personalizados() {
    let mut d = dom(r#"<saludo-x nombre="ana"></saludo-x>"#);
    let r = correr(&mut d, include_str!("clases_dom.js"));
    assert_eq!(
        r,
        [
            "true true true true true true false true false true 1 [object HTMLDivElement]",
            "atributo nombre null ana",
            "conectado 1",
            "atributo nombre null luis",
            "conectado 1",
            r#"true true SALUDO-X 0 true <saludo-x nombre="ana">Hola ana</saludo-x><saludo-x nombre="luis">Hola luis</saludo-x>"#,
            "desconectado",
            "2 B 0 2",
            "true 5 true true",
            "false true false",
            "antes<span>a</span>después 10 20",
            "definido después",
        ]
    );
}

#[test]
fn event_target_propio_y_abort_controller() {
    let mut d = dom("");
    let r = correr(
        &mut d,
        "class Bus extends EventTarget {}
         const b = new Bus(); let n = 0;
         b.addEventListener('x', (e) => { n += e.detail; });
         b.dispatchEvent(new CustomEvent('x', {detail: 3}));
         const c = new AbortController();
         c.signal.addEventListener('abort', () => console.log('abortado', c.signal.reason.name));
         c.abort();
         try { c.signal.throwIfAborted(); } catch (e) { console.log('lanza', e instanceof DOMException); }
         console.log(n, b instanceof EventTarget, c.signal.aborted, AbortSignal.abort('r').reason);",
    );
    assert_eq!(r, ["abortado AbortError", "lanza true", "3 true true r"]);
}

/// Un elemento personalizado compilado al estilo de Babel/TypeScript (`Reflect.construct` con
/// `new.target`), `adoptedStyleSheets`, `document.domain` y errores con `stack`. Como Chromium.
#[test]
fn elementos_compilados_y_errores_con_traza() {
    let mut d = dom(r#"<mio-x id="a"></mio-x>"#);
    let r = correr(
        &mut d,
        r#"function Mio() { var self = Reflect.construct(HTMLElement, [], Mio); self.dato = 7; return self; }
          Mio.prototype = Object.create(HTMLElement.prototype);
          Mio.prototype.constructor = Mio;
          Object.setPrototypeOf(Mio, HTMLElement);
          Mio.prototype.connectedCallback = function () { console.log('conectado', this.id, this.dato); };
          var antes = document.getElementById('a');
          customElements.define('mio-x', Mio);
          var nuevo = new Mio();
          console.log(antes instanceof Mio, antes.dato, nuevo instanceof Mio, nuevo.tagName, nuevo.dato);
          document.adoptedStyleSheets = [...document.adoptedStyleSheets, new CSSStyleSheet()];
          console.log(document.adoptedStyleSheets.length, typeof document.domain);
          try { null.x; } catch (e) { console.log(e instanceof TypeError, typeof e.stack, e.stack.split('\n')[0] === String(e)); }"#,
    );
    assert_eq!(r, ["conectado a 7", "true 7 true MIO-X 7", "1 string", "true string true"]);
}

/// `href` y `src` absolutos, `currentSrc` y `srcset` (como Chromium en https://example.com/).
#[test]
fn urls_absolutas_en_enlaces_e_imagenes() {
    let mut d = Dom::nuevo(
        quimera_html::analizar(r#"<a id="a" href="/x?y=1">a</a><img id="i" src="img/b.png"><a id="v">v</a>"#),
        "https://example.com/dir/pag.html",
    );
    let r = correr(
        &mut d,
        r#"var a = document.getElementById('a'), i = document.getElementById('i'), v = document.getElementById('v');
          console.log(a.href, i.src, i.currentSrc, JSON.stringify(i.srcset), JSON.stringify(v.href), a.getAttribute('href'));
          a.href = 'otra.html'; console.log(a.href, a.getAttribute('href'));"#,
    );
    assert_eq!(
        r,
        [
            r#"https://example.com/x?y=1 https://example.com/dir/img/b.png https://example.com/dir/img/b.png "" "" /x?y=1"#,
            "https://example.com/dir/otra.html otra.html",
        ]
    );
}

/// Un `<a>` se descompone como una URL (lo usa axios), `postMessage` llega como `message`, y
/// `toGMTString`. Como Chromium en https://example.com/dir/pag.html.
#[test]
fn enlaces_como_url_y_post_message() {
    let mut d = Dom::nuevo(quimera_html::analizar(""), "https://example.com/dir/pag.html");
    let r = correr(
        &mut d,
        r#"var a = document.createElement('a');
          a.setAttribute('href', '../x/y.html?q=1#f');
          console.log(a.protocol, a.host, a.hostname, JSON.stringify(a.port), a.pathname, a.search, a.hash, a.origin);
          a.pathname = '/otra'; console.log(a.href, JSON.stringify(document.createElement('a').pathname));
          window.addEventListener('message', function (e) { console.log('mensaje', e.data, e.origin, e.source === window, e instanceof MessageEvent); });
          window.postMessage('hola', '*');
          console.log(new Date(0).toGMTString());"#,
    );
    let mut r = r;
    d.correr_temporizadores(10.0);
    r.extend(d.consola());
    assert_eq!(
        r,
        [
            r#"https: example.com example.com "" /x/y.html ?q=1 #f https://example.com"#,
            r#"https://example.com/otra?q=1#f """#,
            "Thu, 01 Jan 1970 00:00:00 GMT",
            "mensaje hola https://example.com true true",
        ]
    );
}

/// Lo que pide YouTube al arrancar: TreeWalker, NodeIterator, Range, Image, canvas, MessageChannel,
/// `performance.timing`, `Window` e `initCustomEvent`. Como Chromium.
#[test]
fn piezas_para_youtube() {
    let mut d = dom(r#"<div id="r"><p>uno<b>dos</b></p><!--c--><i>tres</i></div>"#);
    let r = correr(
        &mut d,
        r#"var r = document.getElementById('r'), v = [];
          var tw = document.createTreeWalker(r, NodeFilter.SHOW_ELEMENT);
          while (tw.nextNode()) v.push(tw.currentNode.tagName);
          var ni = document.createNodeIterator(r, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_COMMENT), t = [], n;
          while ((n = ni.nextNode())) t.push(n.nodeValue);
          var tw2 = document.createTreeWalker(r, NodeFilter.SHOW_ALL, { acceptNode: function (x) { return x.nodeName === 'P' ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT; } });
          var u = []; while (tw2.nextNode()) u.push(tw2.currentNode.nodeName);
          console.log(v.join(), t.join(), u.join(), tw.parentNode() && tw.currentNode.tagName);
          var rg = document.createRange(); rg.selectNodeContents(r.firstChild);
          var f = rg.createContextualFragment('<span>a</span><em>b</em>');
          console.log(rg.toString(), rg.collapsed, f.childNodes.length, f.firstChild.tagName, rg.commonAncestorContainer === r.firstChild);
          rg.deleteContents(); console.log(r.firstChild.childNodes.length, rg.collapsed);
          var img = new Image(10, 20), cv = document.createElement('canvas'), cx = cv.getContext('2d');
          cx.fillStyle = 'red';
          console.log(img.tagName, img.width === 10 || img.getAttribute('width'), img instanceof HTMLImageElement, cv.width, cv.height, cx.fillStyle, cv.getContext('webgl'), typeof cx.measureText('abc').width);
          var e = document.createEvent('CustomEvent'); e.initCustomEvent('hola', true, false, {x: 1});
          console.log(e.type, e.bubbles, e.detail.x, window instanceof Window, window instanceof EventTarget, typeof performance.timing.responseStart, performance.getEntriesByType('navigation').length);
          var mc = new MessageChannel(); mc.port2.onmessage = function (ev) { console.log('puerto', ev.data); }; mc.port1.postMessage('ping');"#,
    );
    let mut r = r;
    d.correr_temporizadores(10.0);
    r.extend(d.consola());
    assert_eq!(
        r,
        [
            "P,B,I uno,dos,c,tres #comment,I,#text DIV",
            "unodos false 2 SPAN true",
            "0 true",
            "IMG true true 300 150 #ff0000 null number",
            "hola true 1 true true number 1",
            "puerto ping",
        ]
    );
}

/// Lo que está en un `<template>` es inerte: no se mejora ni se conecta hasta que su copia
/// (`importNode`) entra en el documento. Como Chromium.
#[test]
fn plantillas_inertes() {
    let mut d = dom(r#"<template id="t"><mi-p id="dentro"></mi-p></template><div id="caja"></div>"#);
    let r = correr(
        &mut d,
        r#"class P extends HTMLElement {
            constructor() { super(); console.log('construido', this.isConnected); }
            connectedCallback() { console.log('conectado', this.id); }
          }
          customElements.define('mi-p', P);
          var t = document.createElement('template');
          t.innerHTML = '<div><mi-p id="otro"></mi-p></div>';
          console.log('definido', t.content.firstChild.firstChild instanceof P);
          var copia = document.importNode(t.content, true);
          console.log('copiado', copia.firstChild.firstChild instanceof P);
          document.getElementById('caja').appendChild(copia);
          var c2 = document.getElementById('t').content.cloneNode(true);
          console.log('t', document.getElementById('t').content.firstChild instanceof P);
          var t2 = document.createElement('template'); t2.innerHTML = '<b id="x">1</b><template><i>2</i></template>';
          t2.content.firstChild.id = 'y';
          var caja = document.createElement('div'); caja.appendChild(t2);
          var c = caja.cloneNode(true).firstChild, c2 = t2.cloneNode(true);
          console.log(c.content.firstChild.id, c2.content.childNodes.length, c2.content.lastChild.content.firstChild.textContent);
          var dentro = t.content.firstChild, od = dentro.ownerDocument, nuevo = od.createElement('mi-p');
          console.log(od === document, nuevo instanceof P, document.createElement('mi-p') instanceof P, document.body.ownerDocument === document);"#,
    );
    assert_eq!(
        r,
        [
            "definido false",
            "construido false",
            "copiado true",
            "conectado otro",
            "t false",
            "y 2 2",
            "construido false",
            "false false true true"
        ]
    );
}

/// Las propiedades del DOM son accesores de los prototipos, como en Chromium.
#[test]
fn accesores_en_los_prototipos() {
    let mut d = dom(r#"<div id="a" class="c"><b>x</b>y</div>"#);
    let r = correr(
        &mut d,
        r#"var a = document.getElementById('a');
          var fc = Object.getOwnPropertyDescriptor(Node.prototype, 'firstChild');
          console.log(typeof fc.get, fc.set, fc.configurable, fc.get.call(a) === a.firstChild, a.hasOwnProperty('firstChild'));
          var ih = Object.getOwnPropertyDescriptor(Element.prototype, 'innerHTML');
          ih.set.call(a, '<i>z</i>'); console.log(ih.get.call(a), a.firstChild.nodeName, 'childNodes' in a);
          console.log(Object.getOwnPropertyDescriptor(Node.prototype, 'childNodes').get.call(a).length, a.className, a.id);
          var f = document.createDocumentFragment(); f.appendChild(document.createElement('p'));
          console.log(Object.getOwnPropertyDescriptor(DocumentFragment.prototype, 'children').get.call(f).length);"#,
    );
    assert_eq!(r, ["function undefined true true false", "<i>z</i> I true", "1 c a", "1"]);
}

/// Medidas y observadores con un maquetador de prueba: la caja de cada elemento sale de su
/// atributo `data-caja="x y ancho alto"`.
#[test]
fn medidas_y_observadores() {
    let mut d = dom(r#"<div id="a" data-caja="10 20 300 40"></div><div id="b" data-caja="0 2000 50 50"></div>"#);
    d.con_medidas(
        (800.0, 600.0),
        alloc::boxed::Box::new(|doc: &quimera_html::Documento| {
            let mut cajas = alloc::collections::BTreeMap::new();
            for id in doc.descendientes(quimera_html::Documento::RAIZ) {
                let Some(c) = doc.elemento(id).and_then(|e| e.atributo("data-caja")) else { continue };
                let v: Vec<f32> = c.split(' ').filter_map(|n| n.parse().ok()).collect();
                cajas.insert(id.0, [v[0], v[1], v[2], v[3]]);
            }
            crate::Medidas { cajas, ventana: (800.0, 600.0) }
        }),
    );
    let mut r = correr(
        &mut d,
        r#"var a = document.getElementById('a'), b = document.getElementById('b');
          var g = a.getBoundingClientRect();
          console.log(innerWidth, a.clientWidth, a.offsetHeight, a.offsetTop, g.x, g.right, g.bottom, document.documentElement.clientHeight);
          new ResizeObserver(function (es) { es.forEach(function (e) { console.log('tamaño', e.target.id, e.contentRect.width, e.contentRect.height); }); }).observe(a);
          new IntersectionObserver(function (es) { es.forEach(function (e) { console.log('cruce', e.target.id, e.isIntersecting, e.intersectionRatio); }); }).observe(b);"#,
    );
    d.correr_temporizadores(20.0);
    d.ejecutar("document.getElementById('a').setAttribute('data-caja', '10 20 500 40'); document.getElementById('b').setAttribute('data-caja', '0 590 50 50');");
    d.correr_temporizadores(40.0);
    r.extend(d.consola());
    assert_eq!(
        r,
        ["800 300 40 20 10 310 60 600", "tamaño a 300 40", "cruce b false 0", "tamaño a 500 40", "cruce b true 0.2"]
    );
}
