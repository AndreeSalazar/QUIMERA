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
