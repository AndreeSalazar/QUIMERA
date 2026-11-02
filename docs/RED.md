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

`Cliente::solicitar(url, &Peticion)`: cualquier método, cuerpo, cabeceras extra (las peligrosas, como `Cookie`, `Host` o `Content-Length`, y cualquier salto de línea, se ignoran) y si van las cookies. Lo usan `fetch` y `XMLHttpRequest` (ver [DOM.md](DOM.md#la-red-desde-javascript)). Sin cookies (una petición a otro origen), tampoco se guardan las que lleguen.

**Codificación del texto**: la de `Content-Type`; si no la dice, la de `<meta charset>`, pero **solo en HTML**. Un script o un JSON puede llevar el texto `charset=` dentro de una cadena y antes se leía entero como Windows-1252.

## Cookies

El `Cliente` lleva un tarro (`Mutex<Tarro>`, lo comparten los hilos que descargan imágenes y fuentes). Cada respuesta HTTP deja sus `Set-Cookie` y cada petición lleva su `Cookie`, también en las redirecciones (así funciona un inicio de sesión que pone la cookie y redirige).

- **Dominio:** sin `Domain`, solo para ese anfitrión. Con `Domain`, también sus subdominios, pero solo si la respuesta viene de ese dominio y el dominio tiene un punto: `ejemplo.com` no puede poner cookies a `otro.com` ni a todo `com`. La regla completa usa la lista de sufijos públicos (`co.uk`); QUIMERA todavía no la tiene.
- **Ruta:** sin `Path`, la carpeta de la dirección. `/a` vale para `/a/b`, no para `/ab`. Se mandan primero las de ruta más larga, como pide el RFC 6265.
- **`Secure`:** solo por HTTPS, y solo una respuesta HTTPS puede ponerla.
- **Caducidad:** `Max-Age` gana a `Expires`; `Max-Age=0` o una fecha pasada borra la cookie. Las fechas se leen en sus dos formas (`21 Oct 2015` y `21-Oct-15`).
- **En disco:** las que no son de sesión se guardan tras cada página en `%APPDATA%\QUIMERA\galletas.txt` (o `~/.local/share/quimera/`). Es **texto sin cifrar**, como el perfil de Firefox sin contraseña maestra: quien pueda leer la carpeta del usuario ve las sesiones. El modo captura no las lee ni las guarda.

Falta `SameSite`, `HttpOnly` (sin JavaScript da igual: nadie puede leerlas desde la página) y la lista de sufijos públicos.

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
