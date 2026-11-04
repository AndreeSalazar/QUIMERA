# El plan de QUIMERA

QUIMERA no intenta hacer todo lo que hace Chrome. Intenta hacer **bien lo que yo uso**, gastando poca memoria. El examen son mis sitios:

| Sitio | Qué necesita | Fase |
|---|---|---|
| Wikipedia, documentación, blogs, noticias | tablas, `float`, `position` | 1 |
| Buscar (DuckDuckGo en su versión HTML) | formularios, cookies | 2 |
| Tienda de Steam (web) | lo anterior + JavaScript clásico | 3 |
| Epic Games Store (web) | JavaScript + DOM completo, React | 4 |
| Gmail | todo lo anterior + inicio de sesión | 4 |
| **YouTube** | todo lo anterior + video y audio | 5 |

Cada fase deja algo que ya sirve por sí solo. El orden es el que más sitios desbloquea por trabajo.

## Reglas del plan

1. **Solo lo que las páginas reales usan, medido.** Antes de construir algo se mira qué etiquetas, qué CSS y qué funciones de JavaScript usan de verdad los sitios del examen. Lo viejo que nadie usa no entra; lo viejo que las páginas siguen usando sí (como la recuperación de errores del HTML).
2. **Comprobado contra una referencia.** Como hasta ahora: el árbol HTML contra html5ever, la maquetación contra Chromium, las imágenes contra las bibliotecas de referencia, la VM contra Boa. Cada pieza nueva tiene su banco o su comparación.
3. **La memoria se mide en cada fase**, con la misma página en QUIMERA y en Edge (Chromium), y la tabla del README se actualiza. Si una fase sube la memoria sin motivo, se arregla antes de seguir.
4. **El motor en Rust; la interfaz en VERX (Bun + Rust).** Lo que tiene que ser rápido y ligero (red, HTML, CSS, maquetación, JavaScript, GPU) es Rust. Lo que cambia mucho (pestañas, ajustes, descargas) puede ser VERX. Bun también sirve para herramientas: generar código y automatizar pruebas.
5. **Lo que no se escribe a mano:** la criptografía (rustls) y los códecs de video (los del sistema). Un error ahí es un agujero de seguridad, o miles de horas que ya hizo otro y la GPU trae hechas.

## Fase 0: hecho

- **JavaScript:** frontend propio (3,9× el parser de Boa) y VM (3,8× la puntuación de Boa en los 8 benchmarks de V8, con cachés al estilo CacheIR), con GC generacional y RegExp propias.
- **HTML y CSS:** el mismo árbol que html5ever en 5.000 páginas reales; la cascada; bloques, líneas, listas y flexbox como Chromium.
- **Red:** HTTP/1.1 y HTTPS (rustls).
- **Imágenes:** PNG, JPEG (también progresivo) y GIF, al píxel igual que las referencias.
- **Ventana:** wgpu, fuentes del sistema, barra de dirección, enlaces e historial.
- **Memoria:** ~110 MB por página sencilla frente a ~470 MB de Edge.

## Fase 1: leer bien la web

Que las páginas de lectura se vean como en Chrome.

- ✅ **Tablas:** el algoritmo automático, `colspan`/`rowspan`, bordes que colapsan, atributos antiguos y modo de compatibilidad. Las mismas posiciones que Chromium en 39 cajas; Hacker News se ve igual ([detalle](ESTILO-MAQUETACION.md#tablas)).
- ✅ **`float` y `clear`**, `display: flow-root` y `overflow` como contexto propio. Las mismas posiciones que Chromium en 39 casos; en Wikipedia, la ficha a la derecha del texto ([detalle](ESTILO-MAQUETACION.md#flotantes)).
- ✅ **`position` y `z-index`**: `relative`, `absolute`, capas y contextos de apilamiento. Las mismas posiciones que Chromium en 19 cajas, y la misma caja encima en 7 puntos ([detalle](ESTILO-MAQUETACION.md#posición-y-z-index)). Falta que `fixed` y `sticky` se queden quietos al desplazar.
- ✅ **`inline-block`** y **`box-sizing`** ([detalle](ESTILO-MAQUETACION.md#cajas-en-línea-y-controles)). Falta `overflow` con desplazamiento propio.
- ✅ **Variables de CSS** (`--x`, `var()` con reserva, anidadas, heredadas) y `unset` correcto ([detalle](ESTILO-MAQUETACION.md#variables-de-css)). Falta `calc()`, `min()`/`max()`.
- ✅ **`@font-face`** con TrueType, OpenType y WOFF (rust-lang.org sale con sus fuentes; [detalle](VENTANA.md#fuentes-de-la-página-font-face)). Falta WOFF2 (Brotli).
- Respuestas comprimidas: `gzip` y `deflate` (el descompresor de PNG ya sirve).
- Imágenes: `srcset`, `<picture>`, `background-image`, WebP, y mipmaps para que las fotos reducidas no salgan con dientes.

**Comprobación:** capturas lado a lado con Chromium (Wikipedia, el Rust Book, MDN, un blog, un periódico).

## Fase 2: navegar de verdad (todavía sin JavaScript)

- ✅ **Formularios:** `<input>` (texto, contraseña, casillas, opciones, botones), `<textarea>` y `<select>` dibujados con las medidas de Chromium (13 cajas iguales); foco con clic, escribir y borrar, Enter para enviar; envío GET y POST con la codificación del formulario ([detalle](VENTANA.md#formularios)). Falta la lista desplegable de `<select>` (hoy cada clic pasa a la opción siguiente) y `<button>` con contenido.
- Cursor de texto que se mueve, selección, copiar y pegar.
- ✅ **Cookies** con sus reglas de dominio, ruta, `Secure` y caducidad, guardadas en disco ([detalle](RED.md#cookies)). Falta `SameSite` (sin JavaScript ni peticiones de otros sitios aún no hace falta).
- Caché HTTP, HTTP/2 (opcional), descargas.
- Pestañas (en `quimera-ventana`, o ya con VERX).

**Hito:** buscar en DuckDuckGo HTML, leer los resultados y abrirlos. Es decir, navegar. ✅ La búsqueda va por POST y los resultados se ven como en Chromium (114 MB frente a 481 MB de Edge); la cabecera de DuckDuckGo todavía no (falta el logotipo y las dos listas de región y fecha quedan tapadas).

## Fase 3: JavaScript y DOM

La fase más importante: lo que separa "leer páginas" de "usar aplicaciones web". Ver por qué en [qué es el DOM](#qué-es-el-dom-y-por-qué-cuesta).

✅ **Empezada** ([detalle](DOM.md)): `quimera-dom` con un solo GC, los `<script>` de la página, `document` y los nodos, `innerHTML`, `style`, `classList`, selectores, `click` con burbujeo y `preventDefault`, y temporizadores. Las mismas salidas que Chromium en sus pruebas. ✅ JavaScript moderno, primera parte: `JSON`, `Promise`, `Map`/`Set`, clases, plantillas, desestructuración, spread y `?.`, con las mismas salidas que Chromium. ✅ Después: accesores, `Symbol`, generadores, `async`/`await` (también generadores asíncronos y `for await`), `fetch` y `XMLHttpRequest` con CORS, scripts añadidos, módulos con mapas de importación, `URL`, `Date` completo. rust-lang.org, MDN, Hacker News y GitHub ejecutan sus scripts; Wikipedia, jQuery y su cargador de módulos. ✅ Las clases del DOM (`Node`, `Element`, `HTMLElement` y las de cada etiqueta), `customElements` y `Event` con sus subclases (adelanto de la fase 4). Lo que sigue: más eventos (`input`, `keydown`) y medidas de la maquetación (`getBoundingClientRect`).

- **`quimera-js` (nivel 6):** la interfaz del motor, lo único que ve el resto del navegador. Por ahora la hace el rasgo `Anfitrion` de la VM.
- **`quimera-dom` (nivel 7):** `document`, `Node`, `Element`, `Text`, atributos, `classList`, `querySelector`, `innerHTML`, `textContent`, `style`, eventos (`addEventListener`, propagación, clic y teclado).
- **El puente se genera desde WebIDL**, como hacen Chromium y Servo, con un script (Bun + `webidl2`) que lee las interfaces y escribe el Rust. Solo las interfaces que usan los sitios del examen.
- **Un solo GC:** los nodos del DOM viven en el mismo montón que los objetos de JavaScript. Así no hay ciclos entre dos montones, el problema que a Chromium le costó años (Oilpan).
- **El bucle de eventos** del estándar: tareas, microtareas (Promises), `setTimeout`/`setInterval`, `requestAnimationFrame`, y cuándo se vuelve a maquetar y dibujar.
- `fetch` y `XMLHttpRequest`, `JSON`, `localStorage`.
- **Completar JavaScript moderno** en la VM: clases completas, `async`/`await`, generadores, módulos, desestructuración, `Map`/`Set`/`WeakMap`, `Proxy` donde haga falta.
- Rehacer solo lo necesario tras cada cambio del DOM (estilo, maquetación y pintado por partes), para que sea rápido.

**Comprobación:** las *web-platform-tests* oficiales de las partes hechas, automatizadas con Bun. **Hito:** la tienda de Steam y páginas con JavaScript normal. 🟡 La tienda de Steam ejecuta todos sus scripts sin errores (jQuery 1.8, su app de React con `Intl`) y pinta lo que construyen, como los juegos destacados, con 436 MB frente a 726 MB de Edge. Faltan las imágenes que añaden los scripts y que la maquetación coincida con Chromium.

## Fase 4: la web moderna

- Web Components: ✅ `customElements` (con su ciclo de vida) y `<template>`; falta Shadow DOM que se pinte y `<slot>`.
- CSS `grid`, `transform`, `transition`/`animation`, `opacity`, sombras, bordes redondeados.
- SVG (los iconos de casi todos los sitios).
- Inicio de sesión: redirecciones con cookies, formularios con JavaScript, y que Google acepte el navegador (la identificación que se manda).
- Opcional: **lector de correo propio por IMAP** para Gmail, más ligero que la página y sin depender de que Google acepte el navegador.

**Hito:** Epic Games Store y Gmail.

## Fase 5: video

- `<video>` y `<audio>`, Media Source Extensions (lo que usa YouTube).
- Separar los formatos MP4 y WebM (esto sí es propio: es sencillo).
- **Decodificar con el sistema:** Media Foundation en Windows, VA-API en Linux, MediaCodec en Android: H.264, VP9, AV1, Opus y AAC por hardware, sin escribir códecs.
- Salida de audio (WASAPI, PipeWire), sincronía de audio y video, pantalla completa.

**Hito:** YouTube.

## Fase 6: celular (Android)

Donde un navegador ligero más se nota: los teléfonos baratos de 2–3 GB de RAM.

- Rust, wgpu (Vulkan / OpenGL ES) y winit ya funcionan en Android.
- Falta: toque y gestos (desplazar, pellizcar para hacer zoom), `<meta name="viewport">`, teclado en pantalla.
- iPhone solo donde Apple permite motores propios (Unión Europea y Japón).

## Después: SUPER QUIMERA

Cuando QUIMERA termine sus fases, la evolución que debe desplazar del todo al original: **streaming en tiempo real**. La página se muestra y se procesa según llega, aprovechando el tiempo que tarda la red en responder, en vez de esperar respuestas enteras.

La idea viene de los motores de videojuegos: **solo se trabaja en lo que se ve**, y el GC no compite con los fotogramas. Partes ya existen en otros navegadores: Chrome rasteriza por baldosas, V8 recolecta en el tiempo libre entre fotogramas, Chrome descarta imágenes. Lo propio de SUPER QUIMERA es llevarlo hasta el fondo para gastar menos RAM.

Mientras tanto, QUIMERA ya aplica ese principio en lo que se va haciendo: decodificar y subir a la GPU solo lo que está cerca de la vista, y limpiar en los huecos.

**Capas, *streaming* y *zero copy*** (lo pidió el usuario el 4 de noviembre de 2026: «que todo vaya por capas, en *streaming* y sin copias; lo que llegue, que se abra al momento»). Por pasos, cada uno medible:

1. ✅ **Compositor en otro hilo** ([VENTANA.md](VENTANA.md#el-compositor)): el pintado y el vídeo ya no esperan al JavaScript. Vídeo de YouTube: de ~4 a 30 fotogramas por segundo.
2. 🟡 **Capas.** ✅ La **capa fija**: lo `fixed` se queda en su sitio al desplazar, y sus enlaces se pulsan bien (antes se iba con la página: la cabecera de YouTube desaparecía al bajar). ✅ El vídeo ya va en su textura, pintada por el compositor. Falta: cada zona desplazable (`overflow: auto`) en su propia capa, y que el compositor guarde las capas por separado para cambiar una sin volver a subir las demás.
3. ✅ **Desplazar sin esperas.** La página va en su propio hilo; el de la ventana atiende la rueda y se la manda directa al compositor. Con YouTube cargando, cada giro se pinta en 0,5 ms de mediana (26 ms el peor), aunque un script tarde segundos.
4. ***Streaming* del HTML**: analizar y pintar el HTML mientras llega, sin esperar a tenerlo entero, y empezar a pedir hojas e imágenes en cuanto aparecen sus etiquetas.
5. ***Streaming* de imágenes**: decodificar a medida que llegan los bytes (JPEG progresivo, PNG entrelazado) y pintar lo que haya.
6. ***Zero copy***: los bytes de red van al decodificador sin pasar a texto (hoy las respuestas se copian y se convierten); las imágenes y los fotogramas, del decodificador a la GPU sin copias intermedias.
7. **Maquetación incremental**: que cada elemento se recoloque solo si cambió algo que le afecta. Que cada elemento sea del todo independiente no se puede (en CSS la posición de uno depende de los demás, y el JavaScript puede tocar cualquier cosa), pero sí que cada uno se pinte en cuanto tiene lo suyo y que solo se rehaga lo que cambió.
8. **Caminos paralelos para HTML, CSS y JavaScript** (idea del usuario, 4 de noviembre de 2026: «que cada uno tenga su jerarquía independiente para acelerar todo»). Que el trabajo pesado de cada uno vaya en su propio hilo a la vez: analizar el HTML mientras llega; analizar cada hoja de estilo en paralelo y calcular los estilos repartidos entre los núcleos, como Stylo en Firefox (hoy 54 ms en un núcleo en YouTube); compilar los scripts a bytecode en otros hilos mientras se descargan, como V8. Lo que no se puede separar son los puntos de encuentro: el JavaScript puede cambiar el HTML y leer los estilos, y la maquetación necesita los estilos, así que esos van en orden.

## En paralelo

- **VERX:** la interfaz de QUIMERA (pestañas, barra, ajustes) con Bun + Rust, encima del motor.
- **QUIMERA como motor de interfaces.** Steam, Epic, Discord, Spotify y VS Code cargan un Chromium entero solo para dibujar sus botones. Para una interfaz propia el HTML lo escribe uno mismo, así que QUIMERA sirve para eso **mucho antes** que para YouTube. Es el primer uso real para VERX.
- **Velocidad:** decodificar PNG más rápido (hoy 0,5× de la referencia), marcar el GC mayor por partes.
- **El coste fijo de la GPU (potencial para después).** Medido fase a fase (noviembre de 2026): arrancar el driver de DirectX 12 suma **~123 MB** antes de cargar nada, en todas las páginas. Es el mayor coste fijo que queda (Wikipedia entera, 144 MB en uso). Ideas para probar, midiendo cada una: otro sistema gráfico (Vulkan, OpenGL) o el adaptador de bajo consumo; límites y funciones de wgpu más pequeños (`Limits::downlevel_defaults`, sin funciones que no usamos); un atlas de letras y búferes más chicos; y, si nada baja lo suficiente, un modo de pintado por CPU para páginas sencillas. Que la escena suba a la GPU solo lo visible encaja aquí y en SUPER QUIMERA.

## Pendiente: el bloqueador de anuncios

Hecho (noviembre de 2026): filtro de red con EasyList y EasyPrivacy enteras, ocultar elementos con `:has()`, *scriptlets*, anuncios de vídeo de YouTube quitados y Ctrl+B por sitio. Detalle y medidas en [RED.md](RED.md#bloqueo-de-anuncios-y-rastreo). Queda:

- [ ] **Probar Ctrl+B a mano en la ventana** (apagar y encender en YouTube, que se guarde en `sin-bloqueo.txt` y que la recarga lo respete). Hoy solo lo cubren las pruebas unitarias.
- [ ] **Ver un anuncio de vídeo bloqueado de verdad**, cuando el reproductor de YouTube reproduzca ([VIDEO.md](VIDEO.md)): que empiece el vídeo y no el anuncio, y que no pida `/pagead/` ni `/api/stats/ads`.
- [ ] *Scriptlets* que faltan: `json-edit`, `rmnt` (`remove-node-text`), `aost` (`abort-on-stack-trace`), `trusted-*`.
- [ ] Opciones de red que se ignoran (la regla entera no se aplica): `$popup`, `$csp=`, `$redirect=` y `$redirect-rule=` (responder con un recurso vacío en vez de un fallo), `$removeparam=`, `$ping`, `$websocket`.
- [ ] `$generichide` y `$elemhide`: hoy esas excepciones se ignoran, y en esos sitios se ocultan igual las genéricas.
- [ ] Las variantes de Adblock Plus: `#?#` (`:-abp-has`, `:-abp-contains`), `#$#` (sus *snippets*) y `#%#`.
- [ ] Reglas genéricas de ocultar también para las clases e ids que añade el JavaScript después de cargar (hoy solo se miran las del HTML que llega).
- [ ] La lista de sufijos públicos (`co.uk`, `github.io`…): `sitio()` es una aproximación, y también la usan las cookies.
- [ ] Dominios con comodín en las reglas de sitio (`google.*##…`): hoy se ignoran.
- [ ] Actualizar EasyList y EasyPrivacy de vez en cuando (ver `red/red/listas/LEEME.md`), y aclarar su licencia (GPLv3 o CC BY-SA 3.0) antes de repartir el binario.
- [ ] Un indicador en la barra con cuántas peticiones se bloquearon en la página, como el escudo de Brave.
- [ ] Leer las listas en otro hilo al arrancar (hoy tardan 44–97 ms la primera vez que se usa el filtro).

## Qué es el DOM y por qué cuesta

El DOM es la página viva en memoria, en forma de árbol (`documento → body → h1 → "Hola"`), que JavaScript puede leer y cambiar. Existe para que la página cambie sin recargarse: YouTube llega casi vacío y su JavaScript construye toda la página metiendo nodos en ese árbol.

QUIMERA ya construye el árbol (`quimera-html`). Lo difícil es todo lo demás:
- son cientos de interfaces que deben comportarse exactamente como en Chrome;
- el objeto de JavaScript y el nodo se apuntan entre sí, y la memoria debe liberarse sin romper nada;
- el orden de eventos, Promises y redibujado está definido al detalle;
- cada cambio debe rehacer solo lo necesario.

Un solo GC y el puente generado desde WebIDL lo hacen manejable.
