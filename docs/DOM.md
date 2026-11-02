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

**`window` y `document`, más completos** (algunos, aproximaciones para que las páginas no se caigan): `location` entero, `history` (sin cambiar la dirección), `navigator`, `screen`, `innerWidth`, `matchMedia` (nunca coincide), `getComputedStyle` (devuelve el `style` del elemento), `performance.now`, `requestIdleCallback`, `MutationObserver`/`IntersectionObserver`/`ResizeObserver` (no avisan nunca), `Event`/`CustomEvent`, `document.implementation.createHTMLDocument`, `createDocumentFragment`, `dataset`.

**Wikipedia** ya pasa su prueba de compatibilidad (pide `async function*`, `/./.dotAll` y `Object.fromEntries`), arranca su cargador de módulos, los pide por la red y ejecuta jQuery y varios módulos. Algunos fallan aún por detalles del DOM.

## Módulos

`<script type=module>` (en línea o con `src`), `import`/`export` y `import()` dinámico:

- **El parser** convierte un módulo en una función `async (__exportar, __importar, __meta)`: los `import` son `await __importar("...")` al principio (así vale `await` fuera de las funciones), y cada `export` es un getter en el objeto de exportaciones, con el valor vivo. Lo importado se copia al importar: si el otro módulo lo cambia después, aquí no se ve (en los navegadores sí).
- **El cargador** (`red.rs`, en JavaScript) pide cada módulo con `fetch` (con las mismas reglas de origen), lo compila y lo ejecuta una sola vez. En un ciclo, quien importa recibe el espacio de nombres a medio llenar, como en los navegadores. Resuelve rutas relativas, URLs y los nombres de `<script type=importmap>` (exactos o por prefijo).
- Los `<script nomodule>` ya no corren, como en cualquier navegador con módulos.
- `docs/ejemplos/modulos/`: tres módulos que se importan entre sí, con `await fetch` arriba del todo.

**`URL` y `URLSearchParams`**, iguales a Chromium en las pruebas (con `url.searchParams.set(...)` que cambia `href`).

## Comprobado

- **Contra Chromium:** las mismas salidas de `console.log` que Chromium en tres scripts que leen el árbol, lo cambian y escuchan eventos (`web/dom/src/pruebas.rs`), también el orden de `onclick` frente a `addEventListener` y cómo queda el atributo `style`. Las pruebas se ejecutan en cada commit.
- **Una página hecha por sus scripts** (`docs/ejemplos/javascript.html`): título, colores, barras con `createElement` y `style`, fichas con `innerHTML`, y cambios a los 100 ms con `setTimeout`. Los anchos y posiciones horizontales coinciden con Chromium. En vertical hay 2 px de diferencia por la letra del título, no por el JavaScript.
- **Páginas reales:** Hacker News (1,4 ms de scripts) y Wikipedia (11 ms) cargan sin errores y se ven como antes. El arranque de Wikipedia comprueba si el navegador tiene lo moderno (`Promise`, `fetch`...) y, al no encontrarlo, se queda en su versión sin JavaScript, que es lo previsto.
- **Memoria:** Wikipedia («Gato», en español), 194 MB frente a 504 MB de Edge. MDN con todos sus módulos ejecutados, 153 MB frente a 536 MB.
- **MDN** ejecuta todos sus módulos sin errores; **GitHub** carga los suyos (con su mapa de importación) y se para en las clases del DOM (`HTMLElement`, `customElements`), que son la fase 4.

## Falta (por orden, para llegar a YouTube)

1. **JavaScript moderno en la VM:** ✅ `JSON`, `Promise` y microtareas (se vacían tras cada script, evento y temporizador), `Map`/`Set`, clases, plantillas, desestructuración, `...spread`, `?.`, y `localStorage` en memoria ([VM.md](VM.md#la-biblioteca-en-javascript-preludio), [FRONTEND.md](FRONTEND.md#qué-entiende-hoy)). ✅ Accesores `get`/`set`, `Symbol`, `dataset`. rust-lang.org y MDN ya ejecutan todos sus scripts sin errores. ✅ `async`/`await` y generadores. rust-lang.org, MDN, Wikipedia, GitHub y Hacker News ejecutan sus scripts sin errores. Falta: módulos (`<script type=module>`) y los scripts que añaden otros scripts.
2. ✅ **`fetch`, `XMLHttpRequest`** y scripts añadidos, con CORS básico. `localStorage` (en memoria; falta guardarlo).
3. **Más eventos:** `input`, `change`, `keydown`, `submit`, `focus`; y que `input.value` y lo escrito en la ventana sean lo mismo (hoy JavaScript lee el atributo `value`, no lo que tecleó la persona).
4. **Medidas de la maquetación:** `getBoundingClientRect`, `offsetWidth`, `scrollTop`, `getComputedStyle`.
5. **Listas vivas** (`childNodes` es hoy una foto), `DocumentFragment`, `dataset`, `insertAdjacentHTML`.
6. Imágenes y hojas que añaden los scripts (hoy solo se descargan las del HTML inicial), `document.write`, `iframe`.
7. Generar el puente desde WebIDL cuando la tabla pase de unas decenas de métodos.
