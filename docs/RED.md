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

Como el escudo de Brave, pero más pequeño. Va activado de serie y la lista va **dentro del binario** (`red/red/src/bloqueo.txt`): no se descarga nada ni se avisa a nadie.

- **El filtro** (`bloqueo.rs`, `Filtro`): reglas al estilo EasyList. `||dominio^` bloquea el dominio y sus subdominios (se mira cada sufijo del anfitrión en un `HashSet`); `||youtube.com/ptracking`, el anfitrión con esa ruta; un texto suelto (`/pagead/*`) se busca en la URL; `^` es un separador y `*` cualquier trozo; `@@` es una excepción. Un texto entre barras (`/…/`) es una expresión regular, como en EasyList, y todavía se ignora.
- **Opciones** (lo que va tras `$`): `third-party`/`3p` y `~third-party`/`1p`; los tipos `script`, `image`, `stylesheet`, `xmlhttprequest`, `font`, `media`, `subdocument`, `other` (y `~tipo`); `domain=a.com|~b.com`; y `@@…$document`, que deja una página entera sin bloqueo. Una regla con una opción que no se entiende (`$popup`, `$csp=`, `$redirect=`) se ignora entera: mejor no bloquear de más. Para saber si algo es «de otro sitio» cada petición lleva su tipo y la página que la pide (`Peticion::recurso`). El sitio son los dos últimos trozos del anfitrión, o tres en `co.uk`, `gob.pe` y parecidos. Es una aproximación de la lista de sufijos públicos, que aún no está.
- **Ocultar elementos** (`cosmetico.rs`): `##selector`, `sitio##selector`, `~sitio` y la excepción `sitio#@#selector`. Con ellas se hace una hoja `selector{display:none!important}` (una regla por selector, para que uno que no se entienda no anule a los demás). La ventana la añade al final de la página como `<style id="quimera-bloqueo">`, como hace Brave, y vale también para lo que añade después el JavaScript. Las variantes `#?#`, `#$#` y `##+js(…)` se ignoran. Un selector con `:has()` no casa con nada (el estilo todavía no lo entiende), así que en la portada de YouTube puede quedar el hueco de la tarjeta que envuelve un anuncio.
- **Apagar por sitio**: **Ctrl+B** en la ventana apaga o enciende el bloqueo en el sitio actual (`youtube.com`, con sus subdominios) y recarga. La lista se guarda en `%APPDATA%\QUIMERA\sin-bloqueo.txt`, un sitio por línea. En un sitio apagado no se filtra ninguna petición de sus páginas, no se quitan los anuncios de vídeo y no se añade la hoja de ocultar. En el modo captura, `QUIMERA_SIN_BLOQUEO=1` quita el filtro entero (para comparar).
- **La lista**: Google Ads (`doubleclick.net`, `googlesyndication.com`, `googleadservices.com`, IMA), Google Analytics y Tag Manager, las rutas de anuncios de YouTube (`/pagead/`, `/api/stats/ads`, `/ptracking`, `/get_midroll_`) y unas 30 redes de anuncios y rastreo. Además, el rastreo de redes sociales y analítica (Facebook, LinkedIn, Segment, Amplitude…), con `$third-party` cuando el dominio también es el del propio sitio, scripts típicos de anuncios (`/gpt.js$script,third-party`) y reglas para ocultar los anuncios de YouTube (`ytd-ad-slot-renderer`, `#masthead-ad`, `#player-ads`…) y los de AdSense en todas partes. Solo dominios que no sirven otra cosa, para no romper sitios.
- **Dónde**: en `Cliente::solicitar` y `Cliente::pedir`, en cada salto de las redirecciones. Por ahí pasa todo lo que pide la página: `fetch`, `XMLHttpRequest`, scripts, hojas, imágenes y fuentes. Lo bloqueado devuelve `Error::Bloqueado`, que la página ve como un fallo de red y sigue funcionando. **La navegación** (`Cliente::enviar`: la barra, un enlace, un formulario) no se filtra: la elige el usuario.
- **Anuncios de vídeo de YouTube** (`anuncios.rs`): no vienen de otro dominio, sino dentro de la respuesta del reproductor. En YouTube se quitan las claves `adPlacements`, `playerAds` y `adSlots` (con su valor JSON entero y su coma) del HTML de `/watch` (`ytInitialPlayerResponse`) y de las respuestas JSON (`/youtubei/v1/player`) antes de que la página las lea. Es lo que hace Brave con su *scriptlet* `json-prune`.
- `QUIMERA_RED=1` en el modo captura dice cuántas peticiones se bloquearon y cuáles, y cuántas claves de anuncios se quitaron.

**Medido** (modo captura, 1280×900, `QUIMERA_RED=1`, 3 de noviembre de 2026):

| Página | Sin bloqueo | Con bloqueo |
|---|---|---|
| YouTube, `watch?v=GWprpnIG-w4` | pide `googleads.g.doubleclick.net/pagead/id` (2 veces) y `static.doubleclick.net/instream/ad_status.js`; el HTML trae un anuncio al principio del vídeo (`AD_PLACEMENT_KIND_START`) | **3 peticiones bloqueadas**, el anuncio quitado; `ytInitialPlayerResponse` se sigue leyendo (título y 26 formatos), sin `adPlacements` |
| YouTube, RAM (pico, dos pasadas) | 866 y 846 MB privados · 656 y 638 MB en uso | 848 y 802 MB privados · 641 y 595 MB en uso |
| Steam, portada | no pide publicidad | 0 bloqueadas; se ve igual (cambian los juegos destacados, que rota el servidor); 261 → 269 MB privados (ruido) |

La memoria baja poco (de 20 a 40 MB en YouTube, dentro del ruido entre pasadas): hoy los scripts de anuncios ya fallaban o no hacían casi nada en QUIMERA. Lo que se gana sobre todo es privacidad, y que el reproductor no pida anuncios cuando reproduzca vídeo.

**Con las opciones y la hoja de ocultar** (misma medida, `QUIMERA_SIN_BLOQUEO=1` para el «sin»):

| Página | Sin bloqueo | Con bloqueo |
|---|---|---|
| YouTube, vídeo | 0 bloqueadas · 804 MB privados | 3 bloqueadas, el anuncio previo quitado · 818 MB |
| YouTube, portada (sin sesión) | `#masthead-ad` con `display: flex`; el reproductor descarga 6 trozos de `videoplayback` (sin sesión la portada no tiene vídeos, así que casi seguro son del anuncio de cabecera) · 741 MB | 1 bloqueada (`pagead/id`), `#masthead-ad` con `display: none`, **ningún** `videoplayback` · 791–792 MB |
| Steam, portada | 0 bloqueadas · 259 MB | 0 bloqueadas, se ve igual · 271 MB |

La RAM sigue dentro del ruido entre pasadas (en la portada, el «sin» de 741 MB salió más bajo que los dos «con»). Las capturas de la portada son iguales: sin sesión, YouTube solo enseña «Busca algo para comenzar», y el anuncio de cabecera no llegaba a dibujarse.

Falta: las expresiones regulares (`/…/`), los *scriptlets* (`##+js`), `:has()` en el estilo, la lista de sufijos públicos y una lista más grande (EasyList entera tiene unas 80.000 reglas).

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
