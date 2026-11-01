# La red: HTTP y HTTPS

```bash
cargo run --release -p quimera-ventana -- https://example.com/
cargo run --release -p quimera-ventana -- http://info.cern.ch/hypertext/WWW/TheProject.html
```

`quimera-red` (nivel 2, capa `red`): pedir una URL y recibir su respuesta.

| Parte | Qué hace | De quién |
|---|---|---|
| `url.rs` | URLs: análisis (esquema y anfitrión en minúsculas, `\` como `/`, IPv6, puertos, `usuario@`), resolución de relativas con el RFC 3986 (sección 5.2) y codificación `%XX` para la petición | propio |
| `http.rs` | HTTP/1.1: petición GET, respuesta con `Content-Length`, por trozos (`chunked`) o hasta el cierre, `1xx` previos, `charset` de la cabecera o del `<meta>` (UTF-8, Windows-1252/Latin-1, BOM) | propio |
| `lib.rs` | El `Cliente`: `http`, `https`, `file:` y `data:` (texto o base64); hasta 10 redirecciones; tiempos de espera; 64 MB como máximo | propio |
| TLS | TLS 1.2 y 1.3, comprobación del certificado y del nombre del servidor | `rustls` 0.23.20 con `ring`, `webpki-roots` 0.26.7 (raíces de Mozilla) |

TLS es lo único de fuera, a propósito: la criptografía no se escribe a mano (un error ahí es un agujero de seguridad), y rustls es Rust, auditado, y lo usan Servo, Cloudflare y AWS.

## La ventana con red

- La ventana acepta una URL o una ruta local. La página y sus hojas `<link rel="stylesheet">` se descargan **una vez**: redimensionar no vuelve a pedir nada, y F5 recarga.
- Las direcciones relativas se resuelven contra la URL final (tras las redirecciones) o contra `<base href>`.
- El título de la ventana lleva el sitio (`Example Domain · example.com — QUIMERA`).
- Si algo falla (DNS, conexión, certificado, estado HTTP sin contenido), se muestra una página de error de QUIMERA con el motivo.
- El texto plano, CSS, JSON y JavaScript se muestran como texto dentro de un `<pre>`, como hacen los navegadores.

## Comprobado

- **Sin internet** (en el hook): un servidor HTTP de verdad en `127.0.0.1` dentro de la prueba, con respuesta por trozos, redirección relativa con consulta, bucle de redirecciones (se corta en 10), 404, Latin-1, espacios codificados y conexión rechazada. Además, los ejemplos de resolución de relativas del RFC 3986.
- **Con internet** (`cargo test -p quimera-red --test internet -- --ignored`):
  - `https://example.com/` llega bien, y `http://github.com/` redirige a `https`.
  - Se **rechazan** los certificados caducados (`expired.badssl.com`), de otro nombre (`wrong.host`), autofirmados (`self-signed`) y de raíz no confiable (`untrusted-root`), cada uno con su motivo.
- **Páginas reales en la ventana**: `example.com` (que usa `light-dark()`, ya soportado) y la primera página web de la historia, `info.cern.ch` (Tim Berners-Lee, 1991), con sus enlaces y sus listas de definiciones.

## Falta

- Conexiones reutilizadas (*keep-alive*) y varias peticiones a la vez: hoy cada recurso abre su conexión, una tras otra.
- HTTP/2, compresión (`gzip`, `br`; hoy se pide `identity`), caché, cookies, formularios (`POST`).
- Descargar en segundo plano: hoy la ventana espera a que llegue la página.
- Enlaces que se puedan pulsar y una barra de direcciones (`quimera-navegador`).
- Imágenes.
