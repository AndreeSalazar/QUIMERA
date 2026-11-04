# La red: HTTP y HTTPS

```bash
cargo run --release -p quimera-ventana -- https://example.com/
cargo run --release -p quimera-ventana -- http://info.cern.ch/hypertext/WWW/TheProject.html
```

`quimera-red` (nivel 2, capa `red`): pedir una URL y recibir su respuesta.

| Parte | Qué hace | De quién |
|---|---|---|
| `url.rs` | URLs: análisis (esquema y anfitrión en minúsculas, `\` como `/`, IPv6, puertos, `usuario@`), resolución de relativas con el RFC 3986 (sección 5.2) y codificación `%XX` para la petición | propio |
| `http.rs` | HTTP/1.1: petición GET o POST (con la cabecera `Cookie`), respuesta con `Content-Length`, por trozos (`chunked`) o hasta el cierre, `1xx` previos, `charset` de la cabecera o del `<meta>` (UTF-8, Windows-1252/Latin-1, BOM) | propio |
| `lib.rs` | El `Cliente`: `http`, `https`, `file:` y `data:` (texto o base64); hasta 10 redirecciones (tras un POST, 301/302/303 siguen por GET y 307/308 reenvían el formulario); tiempos de espera; 64 MB como máximo | propio |
| `galletas.rs` | Las cookies: el tarro, `Set-Cookie` y `Cookie`, fechas HTTP | propio |
| TLS | TLS 1.2 y 1.3, comprobación del certificado y del nombre del servidor | `rustls` 0.23.20 con `ring`, `webpki-roots` 0.26.7 (raíces de Mozilla) |

TLS es lo único de fuera, a propósito: la criptografía no se escribe a mano (un error ahí es un agujero de seguridad), y rustls es Rust, auditado, y lo usan Servo, Cloudflare y AWS.

## La ventana con red

- La ventana acepta una URL o una ruta local. La página y sus hojas `<link rel="stylesheet">` se descargan **una vez**: redimensionar no vuelve a pedir nada, y F5 recarga.
- Las direcciones relativas se resuelven contra la URL final (tras las redirecciones) o contra `<base href>`.
- El título de la ventana lleva el sitio (`Example Domain · example.com — QUIMERA`).
- Si algo falla (DNS, conexión, certificado, estado HTTP sin contenido), se muestra una página de error de QUIMERA con el motivo.
- El texto plano, CSS, JSON y JavaScript se muestran como texto dentro de un `<pre>`, como hacen los navegadores.

## Peticiones de las páginas

`Cliente::solicitar(url, &Peticion)`: cualquier método, cuerpo, cabeceras extra (las peligrosas, como `Cookie`, `Host` o `Content-Length`, y cualquier salto de línea, se ignoran) y si van las cookies. Lo usan `fetch` y `XMLHttpRequest` (ver [DOM.md](DOM.md#la-red-desde-javascript)). Sin cookies (una petición a otro origen), tampoco se guardan las que lleguen. El cuerpo son bytes (puede ser binario), y un `POST`, `PUT` o `PATCH` sin cuerpo lleva `Content-Length: 0` (Google responde 411 si falta).

`QUIMERA_RED=1` en el modo captura imprime cada petición de la página: método, URL, bytes enviados, estado y tamaño de la respuesta (y su principio, si es un error).

**Compresión**: se piden `gzip, deflate` y se descomprimen con el DEFLATE propio (`quimera-comprimir`, con la envoltura gzip del RFC 1952 y su CRC-32). El CDN de Steam mandaba los scripts en gzip aunque se pidiera `identity`, y por eso jQuery no cargaba. Brotli todavía no: una respuesta en `br` da error.

**Codificación del texto**: la de `Content-Type`; si no la dice, la de `<meta charset>`, pero **solo en HTML**. Un script o un JSON puede llevar el texto `charset=` dentro de una cadena y antes se leía entero como Windows-1252.

## Cookies

El `Cliente` lleva un tarro (`Mutex<Tarro>`, lo comparten los hilos que descargan imágenes y fuentes). Cada respuesta HTTP deja sus `Set-Cookie` y cada petición lleva su `Cookie`, también en las redirecciones (así funciona un inicio de sesión que pone la cookie y redirige).

- **Dominio:** sin `Domain`, solo para ese anfitrión. Con `Domain`, también sus subdominios, pero solo si la respuesta viene de ese dominio y el dominio tiene un punto: `ejemplo.com` no puede poner cookies a `otro.com` ni a todo `com`. La regla completa usa la lista de sufijos públicos (`co.uk`); QUIMERA todavía no la tiene.
- **Ruta:** sin `Path`, la carpeta de la dirección. `/a` vale para `/a/b`, no para `/ab`. Se mandan primero las de ruta más larga, como pide el RFC 6265.
- **`Secure`:** solo por HTTPS, y solo una respuesta HTTPS puede ponerla.
- **Caducidad:** `Max-Age` gana a `Expires`; `Max-Age=0` o una fecha pasada borra la cookie. Las fechas se leen en sus dos formas (`21 Oct 2015` y `21-Oct-15`).
- **En disco:** las que no son de sesión se guardan tras cada página en `%APPDATA%\QUIMERA\galletas.txt` (o `~/.local/share/quimera/`). Es **texto sin cifrar**, como el perfil de Firefox sin contraseña maestra: quien pueda leer la carpeta del usuario ve las sesiones. El modo captura no las lee ni las guarda.

Falta `SameSite`, `HttpOnly` (sin JavaScript da igual: nadie puede leerlas desde la página) y la lista de sufijos públicos.

## Bloqueo de anuncios y rastreo

Como el escudo de Brave. Va activado de serie y las listas van **dentro del binario**: no se descarga nada al navegar ni se avisa a nadie.

**Las listas** (unas 130.000 reglas):

- `red/red/src/bloqueo.txt`, la propia de QUIMERA: Google Ads y Analytics, las rutas de anuncios de YouTube (`/pagead/*`, `/api/stats/ads`, `/ptracking`, `/get_midroll_`), redes de anuncios y rastreo, reglas de ocultar y *scriptlets* para YouTube.
- **EasyList** y **EasyPrivacy** enteras, tal como se publican, en `red/red/listas/` (ver su [LEEME](../red/red/listas/LEEME.md): licencia GPLv3 o CC BY-SA 3.0, distinta de la de QUIMERA, y cómo actualizarlas).

**Reglas de red** (`bloqueo.rs`, `regla.rs`):

- `||dominio^` (el dominio y sus subdominios), `||dominio/ruta`, texto suelto (`/pagead/*`), `^` separador, `*` comodín, `|` ancla al principio o al final, `@@` excepción.
- `/…/`: expresión regular de JavaScript con `quimera-regex`, sobre la URL entera.
- Opciones: `third-party`/`3p`, `~third-party`/`1p`; los tipos `script`, `image`, `stylesheet`, `xmlhttprequest`, `font`, `media`, `subdocument`, `other` (y `~tipo`); `domain=a.com|~b.com`; `match-case`; `@@…$document` para dejar una página entera sin bloqueo. Una regla con una opción que no se entiende (`$popup`, `$csp=`, `$redirect=`…, unas 3.300 en las dos listas) se ignora entera: mejor no bloquear de más.
- Cada petición lleva su tipo y la página que la pide (`Peticion::recurso`), para saber si es «de otro sitio». El sitio son los dos últimos trozos del anfitrión, o tres en `co.uk`, `gob.pe` y parecidos: una aproximación de la lista de sufijos públicos, que aún no está.
- **Sin gastar RAM**: las reglas no copian su texto, apuntan a las listas del binario (`&'static str`). Los 87.000 `||dominio^` sin opciones van en una lista ordenada (búsqueda binaria por cada sufijo del anfitrión). Las demás se guardan bajo su **trozo** más largo que seguro aparece entero en la URL (`doubleclick`, `pagead`…), como hace el motor de Brave: una URL solo se compara con las reglas de los trozos que tiene (unos 11.800 trozos; solo 31 reglas, casi todas expresiones regulares, se miran siempre).
- **Dónde**: `Cliente::solicitar` y `Cliente::pedir`, en cada salto de las redirecciones: `fetch`, `XMLHttpRequest`, scripts, hojas, imágenes y fuentes. Lo bloqueado devuelve `Error::Bloqueado`, que la página ve como un fallo de red. **La navegación** (`Cliente::enviar`: la barra, un enlace, un formulario) no se filtra: la elige el usuario.

**Ocultar elementos** (`cosmetico.rs`, y en la ventana `escudo.rs`):

- `sitio##selector`, `~sitio`, `sitio#@#selector` (excepción). Con ellas se hace una hoja `selector{display:none!important}`, una regla por selector para que uno que no se entienda no anule a los demás. Se añade al final de la página como `<style id="quimera-bloqueo">`, y vale también para lo que añade después el JavaScript.
- **Genéricas** (`##selector`, unas 13.600 en EasyList): meterlas todas en cada página haría lenta la cascada. Como Brave, se usan las que dependen de una clase o un id (`##.anuncio`, `###banner-ad`, `##div.ad-box`) y solo si el HTML que llega trae esa clase o ese id. Las genéricas sin clave (`##a[href*="ad."]`) solo valen si son de la lista propia. Para las genéricas no se miran las clases que añade después el JavaScript.
- Se usan los selectores que entiende el estilo, **`:has()` incluido** ([HTML-CSS.md](HTML-CSS.md)). EasyList lo usa para YouTube: `ytd-rich-item-renderer:has(> #content > ytd-ad-slot-renderer)` quita la tarjeta entera, no solo el anuncio de dentro. Las variantes de Adblock Plus `#?#`, `#$#` y `#%#` se ignoran.

**Scriptlets** (`scriptlets.rs`, `scriptlets.js`): `sitio##+js(nombre, args)` mete un poco de JavaScript en un `<script>` justo tras `<head>`, antes que los scripts de la página. Están `set-constant`, `json-prune` (en `JSON.parse` y `Response.json`), `abort-on-property-read` y `-write`, `abort-current-script`, `addEventListener-defuser`, `no-fetch-if`, `no-xhr-if`, `no-setTimeout-if`, `no-setInterval-if`, `set-local-storage-item` y `remove-cookie`, con sus alias cortos de uBlock Origin (`set`, `aopr`, `acs`, `aeld`, `nostif`…). Los demás (`json-edit`, `rmnt`…) no hacen nada. Los valores que se dejan poner son los de uBlock Origin (`undefined`, `true`, `noopFunc`, números pequeños…), para que una lista no pueda meter código.

**Anuncios de vídeo de YouTube** (`anuncios.rs`): vienen dentro de la respuesta del reproductor. En YouTube se quitan `adPlacements`, `playerAds` y `adSlots` (con su valor JSON entero y su coma) del HTML de `/watch` y de las respuestas JSON (`/youtubei/v1/player`) antes de que la página las lea. Además, la lista propia pone los *scriptlets* `set ytInitialPlayerResponse.adPlacements undefined` y `json-prune`, por si llegan por otro camino.

**Apagar por sitio**: **Ctrl+B** en la ventana apaga o enciende el bloqueo en el sitio actual (`youtube.com`, con sus subdominios) y recarga. La lista se guarda en `%APPDATA%\QUIMERA\sin-bloqueo.txt`, un sitio por línea. En un sitio apagado no se filtra ninguna petición de sus páginas ni se cambia su HTML.

**Para mirar**: `QUIMERA_RED=1` en el modo captura dice cuántas peticiones se bloquearon y cuáles, y cuántas claves de anuncios se quitaron. `QUIMERA_SIN_BLOQUEO=1` quita el filtro (para comparar). La página de prueba de `:has()` es [ejemplos/has.html](ejemplos/has.html).

### Medido

Modo captura, 1280×900, `QUIMERA_RED=1`, 3 y 4 de noviembre de 2026.

| | Sin bloqueo | Con bloqueo |
|---|---|---|
| YouTube, vídeo (`watch?v=GWprpnIG-w4`) | 49 peticiones de la página; `ytInitialPlayerResponse.adPlacements` es un objeto (un anuncio al principio, `AD_PLACEMENT_KIND_START`) | **4 bloqueadas**: `googleads.g.doubleclick.net/pagead/id` (2), `static.doubleclick.net/instream/ad_status.js` y `/api/stats/qoe` (telemetría, regla de EasyPrivacy). `adPlacements` es `undefined`; el título y los formatos siguen; 34 reglas de ocultar y los *scriptlets* puestos (`JSON.parse` ya devuelve los objetos sin `playerAds`) |
| YouTube, portada (sin sesión) | `#masthead-ad` con `display: flex` | 1 bloqueada (`pagead/id`); `#masthead-ad` con `display: none` |
| Steam, portada | 23 peticiones | 0 bloqueadas, se ve igual; solo le tocan las 3 reglas genéricas propias, que no ocultan nada suyo |
| `ejemplos/has.html` | — | igual que Chromium en los 9 casos (hijo, nieto, descendiente anclado, `+`, `~`, ocultar una tarjeta con `:has(> #contenido > ytd-ad-slot-renderer)`) |

**Memoria y tiempo del filtro** (las tres listas, prueba `medir_la_lista`): unos **8 MB** de memoria privada (y unos 10 MB más en uso, sobre todo el texto de las listas leído del binario), 44–97 ms para leerlo una vez por proceso, y 22–47 µs por URL. El binario crece 3,5 MB por las listas.

**En las páginas**, la RAM no cambia de forma que se pueda medir: las diferencias entre «sin» y «con» (YouTube vídeo 855 → 856 MB privados, portada 799 → 799, Steam 277 → 277) son menores que las que hay entre dos pasadas iguales (hasta 40 MB). Lo que se gana es privacidad y que el reproductor no pida anuncios cuando reproduzca vídeo.

**Corrección.** En el commit anterior escribí que con bloqueo la portada dejaba de bajar 6 trozos de `videoplayback`. No es así: esos trozos salen o no según la variante que sirve YouTube (cambia el `am=` de sus scripts), con bloqueo y sin él. Al volver a medir aparecieron en una pasada con bloqueo y no en la de sin.

**Falta**: los *scriptlets* que quedan (`json-edit`, `rmnt`, `aost`…); las opciones `$popup`, `$csp`, `$redirect` y `$removeparam`; `$generichide`; las variantes de Adblock Plus (`#?#`, `#$#`); la lista de sufijos públicos; y mirar las genéricas también en lo que añade el JavaScript.

## Comprobado

- **Sin internet** (en el hook): un servidor HTTP de verdad en `127.0.0.1` dentro de la prueba, con respuesta por trozos, redirección relativa con consulta, bucle de redirecciones (se corta en 10), 404, Latin-1, espacios codificados y conexión rechazada. Además, los ejemplos de resolución de relativas del RFC 3986.
- **Con internet** (`cargo test -p quimera-red --test internet -- --ignored`):
  - `https://example.com/` llega bien, y `http://github.com/` redirige a `https`.
  - Se **rechazan** los certificados caducados (`expired.badssl.com`), de otro nombre (`wrong.host`), autofirmados (`self-signed`) y de raíz no confiable (`untrusted-root`), cada uno con su motivo.
- **Páginas reales en la ventana**: `example.com` (que usa `light-dark()`, ya soportado) y la primera página web de la historia, `info.cern.ch` (Tim Berners-Lee, 1991), con sus enlaces y sus listas de definiciones.

## Falta

- Conexiones reutilizadas (*keep-alive*) y varias peticiones a la vez: hoy cada recurso abre su conexión, una tras otra.
- HTTP/2, compresión (`gzip`, `br`; hoy se pide `identity`), caché.
- Descargar en segundo plano: hoy la ventana espera a que llegue la página.
- Enlaces que se puedan pulsar y una barra de direcciones (`quimera-navegador`).
- Imágenes.
