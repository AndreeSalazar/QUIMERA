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
