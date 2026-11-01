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

- **JavaScript:** frontend propio (3,9× el parser de Boa) y VM (3,3× la puntuación de Boa en los 8 benchmarks de V8), con GC generacional y RegExp propias.
- **HTML y CSS:** el mismo árbol que html5ever en 5.000 páginas reales; la cascada; bloques, líneas, listas y flexbox como Chromium.
- **Red:** HTTP/1.1 y HTTPS (rustls).
- **Imágenes:** PNG, JPEG (también progresivo) y GIF, al píxel igual que las referencias.
- **Ventana:** wgpu, fuentes del sistema, barra de dirección, enlaces e historial.
- **Memoria:** ~110 MB por página sencilla frente a ~470 MB de Edge.

## Fase 1: leer bien la web

Que las páginas de lectura se vean como en Chrome.

- ✅ **Tablas:** el algoritmo automático, `colspan`/`rowspan`, bordes que colapsan, atributos antiguos y modo de compatibilidad. Las mismas posiciones que Chromium en 39 cajas; Hacker News se ve igual ([detalle](ESTILO-MAQUETACION.md#tablas)).
- `float` y `clear`, `position` (`relative`, `absolute`, `fixed`, `sticky`), `z-index`.
- `overflow` con desplazamiento propio, `inline-block`.
- Variables de CSS `var(--x)`, `calc()`, `min()`/`max()`.
- `@font-face`: cargar las fuentes de la página.
- Respuestas comprimidas: `gzip` y `deflate` (el descompresor de PNG ya sirve).
- Imágenes: `srcset`, `<picture>`, `background-image`, WebP, y mipmaps para que las fotos reducidas no salgan con dientes.

**Comprobación:** capturas lado a lado con Chromium (Wikipedia, el Rust Book, MDN, un blog, un periódico).

## Fase 2: navegar de verdad (todavía sin JavaScript)

- **Formularios:** `<input>`, `<textarea>`, `<select>`, `<button>`, envío GET y POST, codificación del formulario.
- Foco, cursor de texto, selección, copiar y pegar.
- **Cookies** (con sus reglas de dominio, ruta, `Secure`, `SameSite`), guardadas en disco.
- Caché HTTP, HTTP/2 (opcional), descargas.
- Pestañas (en `quimera-ventana`, o ya con VERX).

**Hito:** buscar en DuckDuckGo HTML, leer los resultados y abrirlos. Es decir, navegar.

## Fase 3: JavaScript y DOM

La fase más importante: lo que separa "leer páginas" de "usar aplicaciones web". Ver por qué en [qué es el DOM](#qué-es-el-dom-y-por-qué-cuesta).

- **`quimera-js` (nivel 6):** la interfaz del motor, lo único que ve el resto del navegador.
- **`quimera-dom` (nivel 7):** `document`, `Node`, `Element`, `Text`, atributos, `classList`, `querySelector`, `innerHTML`, `textContent`, `style`, eventos (`addEventListener`, propagación, clic y teclado).
- **El puente se genera desde WebIDL**, como hacen Chromium y Servo, con un script (Bun + `webidl2`) que lee las interfaces y escribe el Rust. Solo las interfaces que usan los sitios del examen.
- **Un solo GC:** los nodos del DOM viven en el mismo montón que los objetos de JavaScript. Así no hay ciclos entre dos montones, el problema que a Chromium le costó años (Oilpan).
- **El bucle de eventos** del estándar: tareas, microtareas (Promises), `setTimeout`/`setInterval`, `requestAnimationFrame`, y cuándo se vuelve a maquetar y dibujar.
- `fetch` y `XMLHttpRequest`, `JSON`, `localStorage`.
- **Completar JavaScript moderno** en la VM: clases completas, `async`/`await`, generadores, módulos, desestructuración, `Map`/`Set`/`WeakMap`, `Proxy` donde haga falta.
- Rehacer solo lo necesario tras cada cambio del DOM (estilo, maquetación y pintado por partes), para que sea rápido.

**Comprobación:** las *web-platform-tests* oficiales de las partes hechas, automatizadas con Bun. **Hito:** la tienda de Steam y páginas con JavaScript normal.

## Fase 4: la web moderna

- Web Components: `customElements`, Shadow DOM, `<template>`, `<slot>`.
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

## En paralelo

- **VERX:** la interfaz de QUIMERA (pestañas, barra, ajustes) con Bun + Rust, encima del motor.
- **QUIMERA como motor de interfaces.** Steam, Epic, Discord, Spotify y VS Code cargan un Chromium entero solo para dibujar sus botones. Para una interfaz propia el HTML lo escribe uno mismo, así que QUIMERA sirve para eso **mucho antes** que para YouTube. Es el primer uso real para VERX.
- **Velocidad:** decodificar PNG más rápido (hoy 0,5× de la referencia), marcar el GC mayor por partes.

## Qué es el DOM y por qué cuesta

El DOM es la página viva en memoria, en forma de árbol (`documento → body → h1 → "Hola"`), que JavaScript puede leer y cambiar. Existe para que la página cambie sin recargarse: YouTube llega casi vacío y su JavaScript construye toda la página metiendo nodos en ese árbol.

QUIMERA ya construye el árbol (`quimera-html`). Lo difícil es todo lo demás:
- son cientos de interfaces que deben comportarse exactamente como en Chrome;
- el objeto de JavaScript y el nodo se apuntan entre sí, y la memoria debe liberarse sin romper nada;
- el orden de eventos, Promises y redibujado está definido al detalle;
- cada cambio debe rehacer solo lo necesario.

Un solo GC y el puente generado desde WebIDL lo hacen manejable.
