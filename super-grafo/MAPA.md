# Mapa de Chromiun personal (SUPER GRAFO)

> Generado por SUPER GRAFO el 2026-11-04, en el commit d3c8d7a (html, estilo, dom, ventana: estilo incremental (YouTube: 47 → 4,2 ms por cambio)). Lo rehace «Save para SUPER GRAFO» (o `--consulta guardar`).
> **Para retomar:** leer este mapa primero; para el detalle, preguntar a SUPER GRAFO (MCP: `resumen`, `archivo`, `tiempo`, `vivo`...) solo lo que haga falta.

## Resumen

- 28 crates en 6 capas (por sus dependencias de `Cargo.toml`), 52435 lineas de Rust.
- 857 archivos indexados, 319 dependencias entre archivos, 0 ciclos.
- Sospechas del museo: 2 de Seguridad, 0 de Rendimiento, 40 de Elegancia.
- Marcados por SARIF o la terminal: 9 archivos.

## Capas, de la base a la cima

En el grafo, cada capa es una columna (la 0 a la izquierda).

| Capa | Crate | Que es | Archivos | Lineas | Usa | Marcados |
|---|---|---|---|---|---|---|
| 0 | `quimera-comprimir` | Descompresión sin dependencias: DEFLATE, zlib y gzip. | 3 | 447 |  | 0 |
| 0 | `quimera-valor` | El valor de JavaScript en 64 bits (NaN-boxing, como `JS::Value` de SpiderMonkey): el ladrillo del… | 1 | 378 |  | 0 |
| 0 | `quimera-gpu` | La GPU de QUIMERA (wgpu): una sola tubería que dibuja cuadros (rectángulos de color y letras de un… | 2 | 772 |  | 0 |
| 0 | `quimera-vector` | SVG → imagen RGBA, propio y `no_std`: trazos, formas, rellenos, degradados, `use`. | 5 | 1232 |  | 0 |
| 0 | `quimera-video` | Vídeo: MP4 propio (también el fragmentado, el que usa MediaSource), H.264 a Annex B, color NV12 →… | 6 | 881 |  | 0 |
| 0 | `quimera-guardianes` | El semáforo: archivos de menos de 1000 líneas (aviso a las 700), la jerarquía de niveles, `unsafe`… | 8 | 1508 |  | 0 |
| 0 | `quimera-regex` | Expresiones regulares de JavaScript, `no_std`, como irregexp: patrón → programa → una máquina con… | 7 | 1838 |  | 0 |
| 0 | `quimera-css` | Analizador de CSS, `no_std`: hojas, `@media`, selectores con especificidad, colores, longitudes,… | 8 | 1768 |  | 0 |
| 0 | `quimera-html` | Analizador de HTML5, `no_std`: cualquier texto da un árbol, y se recupera de los errores como un… | 10 | 1978 |  | 0 |
| 1 | `quimera-banco-web` | Banco de `quimera-html` contra html5ever (Servo) en páginas reales, por defecto la documentación de… | 1 | 223 | css, html | 1 |
| 1 | `quimera-imagen` | Decodificadores propios de imágenes, `no_std`: PNG (todas las profundidades, Adam7), JPEG… | 5 | 1568 | comprimir, vector | 0 |
| 1 | `quimera-texto` | Fuentes: busca las del sistema (Windows y Linux), mide texto y rasteriza letras con `ab_glyph`. | 2 | 533 | comprimir | 0 |
| 1 | `quimera-gc` | Recolector de basura generacional (guardería más zona principal, como SpiderMonkey). | 2 | 702 | valor | 3 |
| 1 | `quimera-parser` | Léxico y parser de JavaScript → árbol de sintaxis. | 13 | 3347 | regex | 0 |
| 1 | `quimera-red` | La red propia: URLs, HTTP/1.1, HTTPS con rustls, `file:` y `data:`, cookies, bloqueo de anuncios… | 13 | 2832 | comprimir, regex | 0 |
| 1 | `quimera-estilo` | La cascada de estilos: para cada elemento, el valor de cada propiedad (origen, `!important`,… | 16 | 4065 | css, html | 1 |
| 2 | `quimera-banco-gc` | Banco de `quimera-gc` contra `boa_gc`, con una carga que imita a Splay (una tabla de larga vida y… | 1 | 192 | gc, valor | 0 |
| 2 | `quimera-banco-imagen` | Banco de los decodificadores de imágenes contra `png`, `jpeg-decoder` y `gif`. | 1 | 349 | imagen | 0 |
| 2 | `quimera-frontend` | JavaScript → bytecode de registros (Stencil, como SpiderMonkey): el resultado no tiene objetos del… | 10 | 2608 | parser | 1 |
| 2 | `quimera-objetos` | Los objetos sobre el GC: una cabecera fija y una celda de propiedades que crece sin mover el objeto… | 6 | 1508 | gc, valor | 0 |
| 2 | `quimera-maquetacion` | La maquetación: documento con sus estilos → lista de dibujo | 20 | 4692 | css, estilo, html, vector | 0 |
| 3 | `quimera-banco-frontend` | Banco del frontend de JavaScript contra el parser de Boa 0.22, con más de 1 MB de benchmarks… | 1 | 100 | frontend | 7 |
| 3 | `quimera-pintar` | El pintado: la lista de dibujo de la maquetación → cuadros para la GPU | 2 | 364 | estilo, gpu, html, maquetacion, texto | 0 |
| 3 | `quimera-ver` | Ver una página sin ventana: HTML → estilo → maquetación → un SVG con la lista de dibujo | 1 | 146 | estilo, html, maquetacion | 0 |
| 3 | `quimera-vm` | La máquina virtual: ejecuta el bytecode de registros, con cachés en línea, sin recursión de Rust y… | 28 | 8373 | frontend, gc, objetos, regex, valor | 2 |
| 4 | `quimera-banco-vm` | Banco de la máquina virtual contra `boa_engine` 0.22, con los benchmarks de Boa: los dos tienen que… | 1 | 343 | frontend, vm | 24 |
| 4 | `quimera-dom` | El DOM visto desde JavaScript: un solo árbol y un solo GC. | 15 | 5776 | css, html, video, vm | 1 |
| 5 | `quimera-ventana` | La ventana de QUIMERA: tres hilos (la ventana con la rueda, la página con JS, estilo y maquetación,… | 19 | 3912 | dom, estilo, gpu, html, imagen, maquetacion, pintar, red, texto, video, vm | 0 |

Como se comporta cada parte, con sus medidas y lo que hay que cuidar: [NOTAS.md](NOTAS.md) (39 notas). Quien usa a quien, entero: `mapa.json`.

## Lo mas grande (lineas de Rust)

| Archivo | Lineas |
|---|---|
| `motor-js/vm/tests/moderno.rs` | 682 |
| `web/estilo/src/cascada.rs` | 677 |
| `web/dom/src/pruebas.rs` | 671 |
| `web/maquetacion/src/bloque.rs` | 655 |
| `grafico/gpu/src/lib.rs` | 651 |
| `web/estilo/src/propiedades.rs` | 640 |
| `web/dom/src/lib.rs` | 629 |
| `motor-js/parser/src/lexico.rs` | 622 |
| `web/dom/src/puente.rs` | 614 |
| `navegador/ventana/src/navegador.rs` | 609 |

## Lo mas tocado (commits que lo cambiaron)

| Archivo | Cambios |
|---|---|
| `navegador/ventana/src/main.rs` | 24 |
| `motor-js/vm/src/nativas/mod.rs` | 19 |
| `navegador/ventana/src/navegador.rs` | 18 |
| `web/dom/src/lib.rs` | 18 |
| `web/maquetacion/src/bloque.rs` | 16 |
| `motor-js/vm/src/lib.rs` | 15 |
| `web/dom/src/pruebas.rs` | 15 |
| `motor-js/vm/src/propiedades.rs` | 14 |
| `motor-js/vm/src/interprete.rs` | 13 |
| `web/estilo/src/lib.rs` | 13 |

## Historia

70 commits, del 2026-10-31 al 2026-11-04. De 449 a 748 archivos y de 361783 a 572530 lineas (segun git).

## En vivo (lo ultimo que se vio, 10 s)

| Carpeta | ms por segundo |
|---|---|
| `motor-js/vm` | 523.8 |
| `red/red` | 91.5 |
| `web/estilo` | 13.2 |
| `grafico/gpu` | 3.2 |
| `grafico/video` | 2.7 |
| `web/maquetacion` | 1.1 |
| `grafico/pintar` | 0.0 |

Trafico: grafico/pintar -> grafico/gpu (12), motor-js/vm -> red/red (7), web/dom -> motor-js/vm (4), web/dom -> web/estilo (3), web/estilo -> web/maquetacion (3), web/maquetacion -> grafico/pintar (3).

## Como seguir

- Ver el grafo organizado: abrir esta carpeta en SUPER GRAFO, vista Grafo (el orden esta en `super-grafo/orden.json`).
- La Ciudad en el tiempo: `--tiempo`; en vivo: `--vivo` con el programa escribiendo `.super-grafo-vivo.jsonl`.
- Rehacer este mapa: «Save para SUPER GRAFO» o `ultra-carpetas --consulta guardar <carpeta>`.
