# El DOM: JavaScript en las páginas

```bash
cargo run --release -p quimera-ventana -- docs/ejemplos/javascript.html
```

`quimera-dom` (nivel 7, capa `web`) conecta el motor de JavaScript (`quimera-vm`) con el árbol del documento (`quimera-html`). Es el comienzo de la fase 3 del [plan](PLAN.md): lo que separa leer páginas de usar aplicaciones web.

## Cómo está hecho

- **Un solo árbol.** Los scripts cambian el mismo `Documento` que leen el estilo y la maquetación; no hay una copia para JavaScript. Tras un cambio (o un clic, o un temporizador), la ventana vuelve a maquetar.
- **Un solo GC.** Cada nodo que un script toca tiene un objeto en el montón de la VM, con el número del nodo dentro. Se crea la primera vez y se guarda en la raíz del anfitrión, así que el mismo nodo da siempre el mismo objeto (`a === b`). Los listeners y los temporizadores también viven en ese montón. No hay dos montones que se apunten entre sí, el problema que a Chromium le costó años resolver (Oilpan).
- **Objetos del anfitrión en la VM** (`motor-js/vm/src/anfitrion.rs`). Son una clase nueva de objeto: al leer o escribir una propiedad suya, la máquina primero pregunta al navegador (`textContent`, `id`...). Si el navegador no la conoce, es una propiedad normal y sus métodos están en el prototipo. Las cachés en línea no guardan nunca estos objetos: comprobarlo es una comparación más sobre un valor que la caché ya leía, y la puntuación de V8 no cambió (3,80× Boa, antes 3,81×).
- **El anfitrión va en un `Rc`** y sus métodos toman `&self`. Un evento puede llamar a JavaScript y JavaScript al navegador sin quedarse sin anfitrión a mitad (`el.click()` dentro de un listener funciona).
- **Hecho a mano, en tablas.** Los métodos son una tabla (prototipo → nombre → número) y las propiedades, un `match` por nombre. Cuando la superficie crezca, esas tablas se generarán desde WebIDL, como hacen Chromium y Servo.
- **Sin `quimera-js` todavía.** La [jerarquía](JERARQUIA.md) prevé una interfaz del motor en el nivel 6. Por ahora ese papel lo hace el rasgo `Anfitrion` de la VM (pocas funciones). La capa se separará cuando la interfaz deje de cambiar.

## Qué hay

| | |
|---|---|
| **Scripts** | Los `<script>` clásicos en orden, en línea y con `src` (se descargan con la página). Un script que lanza un error no detiene a los siguientes. Luego `DOMContentLoaded` y `load` |
| **`window`** | `window`, `self`, `document`, `location.href`, `navigator.userAgent`, `setTimeout`, `setInterval`, `clearTimeout`, `requestAnimationFrame`, `alert` (a la consola), `addEventListener` |
| **`document`** | `getElementById`, `querySelector(All)`, `getElementsByTagName/ClassName`, `createElement`, `createTextNode`, `createComment`, `body`, `head`, `documentElement`, `title` (también escribirlo), `URL`, `readyState` |
| **Nodos** | `parentNode`, `childNodes`, `children`, `first/lastChild`, `next/previousSibling` y sus versiones de elemento, `nodeType`, `nodeName`, `textContent`, `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `remove`, `append`, `cloneNode`, `contains` |
| **Elementos** | `tagName`, `id`, `className`, atributos reflejados (`href`, `src`, `name`, `type`...), `checked`/`disabled`/`hidden`, `value`, `get/set/remove/hasAttribute`, `innerHTML` (leer y escribir), `outerHTML`, `matches`, `closest`, `click()` |
| **`style`** | `el.style.backgroundColor = ...`, `cssText`, `setProperty`, `getPropertyValue`, `removeProperty`. Se escriben en el atributo `style`, como `a: b; c: d;` igual que Chromium |
| **`classList`** | `add`, `remove`, `toggle` (con o sin forzar), `contains`, `length` |
| **Selectores** | Tipo, `*`, `#id`, `.clase`, `[atributo]`, `=`, `^=`, `$=`, `*=`, `~=`, descendiente, `>` y listas con comas. Un selector con algo más (`:hover`, `+`) no coincide con nada |
| **Eventos** | `click` al pulsar en la ventana: en el enlace o control pulsado, o en `<body>`. Sube por los antepasados hasta `window`. Primero el manejador `onclick` (propiedad o atributo), luego los listeners, como Chromium. `preventDefault()` evita seguir el enlace o enviar el formulario; `stopPropagation()` corta la subida |
| **Temporizadores** | Con el reloj de la página (ms desde que llegó). La ventana duerme hasta el siguiente (`ControlFlow::WaitUntil`). En el modo captura se adelanta el reloj un segundo, para ver lo que hacen |

## La red desde JavaScript

`fetch` (con `Response`, `Headers`, `text()`, `json()`), `XMLHttpRequest` y los `<script src>` que añade un script (`red.rs`). El DOM no tiene red: apunta lo pedido en una cola, y la ventana la atiende en otros hilos y devuelve cada respuesta (en el modo captura, al momento). Las reglas de origen las aplica la ventana (`peticiones.rs`):

- Una página web no puede leer archivos locales (`file:`).
- Las cookies solo van al mismo origen (esquema, anfitrión y puerto), salvo `credentials: 'include'`.
- **CORS**: a otro origen se manda `Origin`, y la respuesta solo llega si el servidor lo permite (`Access-Control-Allow-Origin`). Un `<script src>` de otro origen se ejecuta (no se lee), pero va sin cookies.
- Falta la petición previa `OPTIONS` de las peticiones «no simples».

`docs/ejemplos/fetch.html` pide un JSON con `fetch` + `async`/`await` y lo pinta.

**`window` y `document`, más completos** (algunos, aproximaciones para que las páginas no se caigan): `location` entero, `history` (sin cambiar la dirección), `navigator`, `screen`, `innerWidth`, `matchMedia` (de verdad: ver abajo), `getComputedStyle` (devuelve el `style` del elemento), `performance.now`, `requestIdleCallback`, `MutationObserver`/`IntersectionObserver`/`ResizeObserver` (avisan de verdad: ver abajo), `Event`/`CustomEvent`, `document.implementation.createHTMLDocument`, `createDocumentFragment`, `dataset`.

**Wikipedia** ya pasa su prueba de compatibilidad (pide `async function*`, `/./.dotAll` y `Object.fromEntries`), arranca su cargador de módulos, los pide por la red y ejecuta jQuery y varios módulos. Algunos fallan aún por detalles del DOM.

## Módulos

`<script type=module>` (en línea o con `src`), `import`/`export` y `import()` dinámico:

- **El parser** convierte un módulo en una función `async (__exportar, __importar, __meta)`: los `import` son `await __importar("...")` al principio (así vale `await` fuera de las funciones), y cada `export` es un getter en el objeto de exportaciones, con el valor vivo. Lo importado se copia al importar: si el otro módulo lo cambia después, aquí no se ve (en los navegadores sí).
- **El cargador** (`red.rs`, en JavaScript) pide cada módulo con `fetch` (con las mismas reglas de origen), lo compila y lo ejecuta una sola vez. En un ciclo, quien importa recibe el espacio de nombres a medio llenar, como en los navegadores. Resuelve rutas relativas, URLs y los nombres de `<script type=importmap>` (exactos o por prefijo).
- Los `<script nomodule>` ya no corren, como en cualquier navegador con módulos.
- `docs/ejemplos/modulos/`: tres módulos que se importan entre sí, con `await fetch` arriba del todo.

**`URL` y `URLSearchParams`**, iguales a Chromium en las pruebas (con `url.searchParams.set(...)` que cambia `href`).

## Lo que pidió YouTube

`complementos.rs`, y más clases de eventos en `interfaces.rs`:

- `NodeFilter`, `document.createTreeWalker` y `createNodeIterator`, con el algoritmo de la especificación.
- `Range` (`createContextualFragment`, `selectNodeContents`, `deleteContents`…), sin medir ni pintar; `getSelection()` vacía.
- `new Image()`; `img.width` y `img.height` como números.
- `canvas.getContext('2d')`: no dibuja, pero guarda su estado. Normaliza los colores con el CSS de QUIMERA (`'red'` → `#ff0000`) y mide texto de forma aproximada. WebGL da `null`.
- `Window` (`window` hereda de `Window.prototype` y de `EventTarget`), `MessageChannel`, `postMessage`, `performance.timing` y la entrada de navegación, `document.importNode`/`adoptNode`, `CDATASection` y `ProcessingInstruction`.
- Los `init...Event` antiguos y unas veinte clases de eventos más (`WheelEvent`, `DragEvent`, `TouchEvent`, `AnimationEvent`, `PopStateEvent`…).
- Los errores en elementos personalizados llevan traza.

Todo comparado con Chromium en `pruebas.rs`.

- **Plantillas inertes**: lo que hay dentro de un `<template>` (también su `content`) no se mejora ni se conecta, y su `ownerDocument` es un documento inerte que crea elementos sin mejorar (Polymer convierte `<template is="dom-repeat">` con él). `importNode` mejora la copia; `cloneNode` copia también el `content`. El analizador HTML deja el contenido de un `<template>` de `<head>` dentro de él.
- Meter un fragmento en el documento conecta sus hijos (antes el fragmento llegaba ya vacío).
- **Las propiedades del DOM son accesores de los prototipos**, como en los navegadores: `Object.getOwnPropertyDescriptor(Node.prototype, 'firstChild').get` existe y funciona con `.call(nodo)`. Son nativas que leen del puente directamente (`__quimera_accesor_anfitrion`). ShadyDOM, el polyfill de Shadow DOM que usa YouTube, mira ese descriptor para elegir su camino. En `Node`: `parentNode`, `firstChild`, `childNodes`, `textContent`…; en `Element`: `children`, `innerHTML`, `id`, `className`…
- `el.attributes` es un `NamedNodeMap` (`getNamedItem`, `item`…), aún como copia, no vivo.
- `new Audio()` y lo mínimo de `HTMLMediaElement` (`load`, `play`, `pause` y el estado de uno sin cargar). `canPlayType` da `''`: todavía no hay reproductor.

- **Medidas de la maquetación** (`clientWidth`, `offsetWidth`, `offsetHeight`, `offsetTop`, `scrollWidth`, `getBoundingClientRect`, `getClientRects`, `innerWidth`). El DOM no maqueta (va por debajo del estilo): el anfitrión le da una función con `Dom::con_medidas`, y el DOM guarda sus medidas hasta que el árbol cambia. La ventana maqueta con las hojas de la página y anchos de texto aproximados: las páginas miden sobre todo contenedores. La cascada guarda las hojas ya analizadas (`CacheHojas`): con las 19 000 reglas de YouTube, cada maquetación baja de unos 200 ms a 25–50 ms. Aún sin restar bordes en `client*` ni contar el desplazamiento de la página.
- **`ResizeObserver` e `IntersectionObserver`** de verdad (`observadores.rs`): tras una tarea que cambió el árbol, miran en el siguiente fotograma (16 ms) y avisan la primera vez y en cada cambio de tamaño, de entrada o salida o de umbral. **`MutationObserver`** también (`mutaciones.rs` y `observadores.rs`): mientras alguien observa, el árbol anota cada cambio de hijos, de atributo o de texto (`quimera_html::Cambio`; no las copias de `innerHTML` ni de `cloneNode`, que son nodos nuevos); tras el primer cambio se encola una microtarea, como en los navegadores, que reparte los cambios según `childList`, `attributes`, `attributeFilter`, `characterData`, `subtree` y los `...OldValue`. Polymer encola sus propias microtareas cambiando el texto de un nodo observado: sin esto, YouTube nunca calculaba si mostrar la guía.
- **Los avisos son `IntersectionObserverEntry` y `ResizeObserverEntry` de verdad**: el polyfill de la W3C que trae YouTube solo se aparta si ve `'intersectionRatio' in IntersectionObserverEntry.prototype`; si no, ponía el suyo, que nunca avisaba. `rootMargin` acepta porcentajes y un elemento sin área (una imagen sin cargar) también cruza si está dentro.
- **Imágenes que llegan**: `Dom::imagen_cargada(src, tamaño)` lanza `load` (o `error`), sin burbujear, en cada `<img>` con ese `src`, y desde entonces `naturalWidth`, `naturalHeight` y `complete` dicen la verdad. La ventana lo llama al llegar cada imagen; en captura, `Guion::adelantar` trae las imágenes nuevas entre tarea y tarea. YouTube pone sus miniaturas con `opacity: 0` hasta su `load`.
- **`matchMedia`** contra la ventana: `min-`/`max-` `width`/`height` (px y em), rangos (`width >= 600px`), `orientation`, `aspect-ratio`, `resolution`, `not`, `and`, listas, `screen`/`print`, y las preferencias de QUIMERA (tema claro, sin reducir movimiento, ratón con `hover`).
- **`Request`** y `fetch(request)`; **los cuerpos binarios** (`ArrayBuffer`, vistas tipadas) salen como bytes: YouTube manda su JSON comprimido con gzip y antes llegaba como `"31,139,8,..."`.

- **`data` en un elemento** es una propiedad suelta (solo los nodos de texto y comentarios cambian su texto con ella); Polymer escribe `<a data="[[command]]">`.
- Lo que se escribe como texto (`innerHTML`, `textContent`, `setAttribute`, atributos reflejados como `href`) usa el `toString` del objeto (`SafeHtml`, `TrustedScriptURL`...).
- **`element.animate()`** (`animaciones.rs`): sin fotogramas intermedios; al acabar su retraso y su duración avisa (la promesa `finished` y luego `onfinish` y `finish`, como Chromium) y, con `fill: forwards`, escribe el último fotograma en el estilo (Chromium lo guarda en una capa de efectos; se ve igual).
- `document.getElementsByName`, `CSS.escape`, `CSS.supports` y `PerformanceObserver.supportedEntryTypes`.

**Dónde está YouTube:** arranca sin errores, como en Chrome (antes eran 6), y pinta su portada: la cabecera con la caja de búsqueda y la tarjeta «Busca algo para comenzar». Sin sesión, Chrome muestra lo mismo: YouTube no trae vídeos a la portada de quien no tiene historial. Faltaban el `new` sobre funciones atadas (su inyector de dependencias), las plantillas inertes, los descriptores del DOM, las medidas y los observadores. Los iconos SVG se dibujan (menú, logo, lupa, cuenta) con los colores de Chrome. Las cajas negras que tapaban los botones eran sus capas de pulsación, con `opacity: 0`: ahora el estilo aplica `opacity`. La mini guía de la izquierda (Principal, Shorts, Suscripciones, Tú) también sale: faltaban `matchMedia`, `MutationObserver` y `Request`, sin los que YouTube creía que no había red y se iba a su copia local, que estaba vacía. Falta la petición del reproductor (`player`), que se manda sin cuerpo: es de la fase de vídeo.

## Clases del DOM y elementos personalizados

`interfaces.rs` (JavaScript sobre los prototipos que crea `puente.rs`):

- **La cadena de los navegadores**: `EventTarget` → `Node` → `Element` → `HTMLElement` → `HTMLDivElement`..., más `CharacterData`/`Text`/`Comment`, `DocumentFragment`, `Document` y `SVGElement`. Cada nodo nace con el prototipo de su etiqueta (unas 45 interfaces `HTML...Element`, y `HTMLUnknownElement` para las demás). Así funcionan `el instanceof HTMLInputElement`, `Node.ELEMENT_NODE` y `Object.prototype.toString.call(el)` (`[object HTMLDivElement]`).
- **`customElements`**: `define` (mejora también los elementos que ya estaban en la página), `get`, `getName`, `whenDefined` y `upgrade`. Al mejorar un elemento, se le pone el prototipo de la clase y se ejecuta su constructor sobre él. Después llegan `attributeChangedCallback` (por cada atributo de `observedAttributes`) y `connectedCallback`. Insertar y quitar elementos llama a `connectedCallback`/`disconnectedCallback`. `document.createElement('mi-x')`, `new MiX()` e `innerHTML` crean elementos ya mejorados.
- **`super()` que devuelve otro objeto**: en `class X extends HTMLElement`, el `this` del constructor es el elemento que crea el navegador, no un objeto nuevo. El parser cambia el `this` de los constructores de subclase por una variable que toma el valor de `super(...)` (`parser/este.rs`).
- **Eventos de verdad**: `Event` y sus subclases (`CustomEvent`, `MouseEvent`, `KeyboardEvent`, `FocusEvent`, `InputEvent`, `PointerEvent`, `UIEvent`) con `bubbles`, `cancelable` y `detail`; `dispatchEvent` en nodos y en `window`. Un evento que no burbujea solo llega a su objetivo.
- **`EventTarget` propio** (`new EventTarget()`, o una clase que lo extiende), `AbortController`/`AbortSignal` y `DOMException`.
- **Más de `Node` y `Element`**: `prepend`, `before`, `after`, `replaceWith`, `replaceChildren`, `toggleAttribute`, `getAttributeNames`, `attributes`, `insertAdjacentHTML/Element/Text`, `compareDocumentPosition`, `template.content`. `DocumentFragment` de verdad: al insertarlo se mueven sus hijos.
- **Clases compiladas** (Babel, TypeScript): crean el elemento con `Reflect.construct(HTMLElement, args, Clase)`. Al mejorar un elemento que ya existe, `HTMLElement()` devuelve ese elemento, como en los navegadores.
- **También**: `adoptedStyleSheets` (se guarda, no se aplica), `document.domain`, y `href`/`src` con la URL absoluta (`a.href`, `img.src`), `img.currentSrc` (en Chrome está vacío hasta que la imagen carga) y `srcset`.
- **Errores en la consola**: cada error lleva su traza, con la URL del script o del módulo de cada llamada.
- **Aproximaciones**: `attachShadow` devuelve un fragmento aparte que **no se pinta** (falta Shadow DOM de verdad, con `<slot>`), `getBoundingClientRect` da ceros y `CSSStyleSheet` no aplica nada.

Probado contra Chromium: un elemento personalizado con atributos observados, ciclo de vida y eventos propios da las mismas 12 líneas en los dos (`web/dom/src/pruebas.rs`).

## Comprobado

- **Contra Chromium:** las mismas salidas de `console.log` que Chromium en tres scripts que leen el árbol, lo cambian y escuchan eventos (`web/dom/src/pruebas.rs`), también el orden de `onclick` frente a `addEventListener` y cómo queda el atributo `style`. Las pruebas se ejecutan en cada commit.
- **Una página hecha por sus scripts** (`docs/ejemplos/javascript.html`): título, colores, barras con `createElement` y `style`, fichas con `innerHTML`, y cambios a los 100 ms con `setTimeout`. Los anchos y posiciones horizontales coinciden con Chromium. En vertical hay 2 px de diferencia por la letra del título, no por el JavaScript.
- **Páginas reales:** Hacker News (1,4 ms de scripts) y Wikipedia (11 ms) cargan sin errores y se ven como antes. El arranque de Wikipedia comprueba si el navegador tiene lo moderno (`Promise`, `fetch`...) y, al no encontrarlo, se queda en su versión sin JavaScript, que es lo previsto.
- **Memoria:** Wikipedia («Gato», en español), 194 MB frente a 504 MB de Edge. MDN con todos sus módulos ejecutados, 153 MB frente a 536 MB. Con las clases del DOM (mediana de varias corridas): GitHub con sus elementos personalizados, 321 MB frente a 725 MB de Edge (varía entre 306 y 426); MDN, 100–121 MB frente a 513–525 MB. La velocidad no cambió: V8 da 3,70× Boa, y el commit anterior, medido el mismo día, 3,67×. Con trazas, `Proxy` y datos binarios (GitHub ejecuta mucho más): GitHub 317 MB frente a 736 MB, MDN 101 MB frente a 458 MB; V8 un 2 % por debajo del commit anterior ([VM.md](VM.md#accesores-y-símbolos)).
- **MDN** ejecuta todos sus módulos sin errores. **GitHub** carga los suyos (con su mapa de importación) y define sus elementos personalizados. Con las trazas de los errores ([VM.md](VM.md#errores-con-traza)) se encontraron los fallos uno a uno: hojas adoptadas, `let` de un bucle que tapaba otra variable, propiedades no enumerables, `Proxy`, datos binarios, clases compiladas por Babel... De 6 módulos con error queda uno, que necesita `BigInt` (todavía no está en la VM).
- **Wikipedia** sigue con sus 2 errores de siempre, ahora con traza: `buildFragment` de jQuery recibe un `ownerDocument` vacío, y `parseImageUrl` (de las imágenes de la portada) lee algo que no tenemos.

## Falta (por orden, para llegar a YouTube)

1. **JavaScript moderno en la VM:** ✅ `JSON`, `Promise` y microtareas (se vacían tras cada script, evento y temporizador), `Map`/`Set`, clases, plantillas, desestructuración, `...spread`, `?.`, y `localStorage` en memoria ([VM.md](VM.md#la-biblioteca-en-javascript-preludio), [FRONTEND.md](FRONTEND.md#qué-entiende-hoy)). ✅ Accesores `get`/`set`, `Symbol`, `dataset`. rust-lang.org y MDN ya ejecutan todos sus scripts sin errores. ✅ `async`/`await` y generadores. rust-lang.org, MDN, Wikipedia, GitHub y Hacker News ejecutan sus scripts sin errores. Falta: módulos (`<script type=module>`) y los scripts que añaden otros scripts.
2. ✅ **`fetch`, `XMLHttpRequest`** y scripts añadidos, con CORS básico. `localStorage` (en memoria; falta guardarlo).
3. **Más eventos:** `input`, `change`, `keydown`, `submit`, `focus`; y que `input.value` y lo escrito en la ventana sean lo mismo (hoy JavaScript lee el atributo `value`, no lo que tecleó la persona).
4. **Medidas de la maquetación:** ✅ `getBoundingClientRect`, `offsetWidth`, `clientWidth`, `ResizeObserver`, `IntersectionObserver`. ✅ `MutationObserver`. Falta `getComputedStyle` de verdad y `scrollTop` con el desplazamiento.
5. **Listas vivas** (`childNodes` es hoy una foto). ✅ `DocumentFragment`, `dataset`, `insertAdjacentHTML`, clases del DOM y `customElements`. Falta Shadow DOM que se pinte.
6. Imágenes y hojas que añaden los scripts (hoy solo se descargan las del HTML inicial), `document.write`, `iframe`.
7. Generar el puente desde WebIDL cuando la tabla pase de unas decenas de métodos.
