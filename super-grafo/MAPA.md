# Mapa de Chromiun personal (SUPER GRAFO)

> Generado por SUPER GRAFO el 2026-11-04, en el commit 88a9b32 (ventana: trazas en vivo (QUIMERA_VIVO) para la Ciudad 4D de SUPER GRAFO). Lo rehace «Save para SUPER GRAFO» (o `--consulta guardar`).
> **Para retomar:** leer este mapa primero; para el detalle, preguntar a SUPER GRAFO (MCP: `resumen`, `archivo`, `tiempo`, `vivo`...) solo lo que haga falta.

## Resumen

- 28 crates en 6 capas (por sus dependencias de `Cargo.toml`), 51331 lineas de Rust.
- 852 archivos indexados, 308 dependencias entre archivos, 0 ciclos.
- Sospechas del museo: 2 de Seguridad, 0 de Rendimiento, 40 de Elegancia.
- Marcados por SARIF o la terminal: 8 archivos.

## Capas, de la base a la cima

En el grafo, cada capa es una columna (la 0 a la izquierda).

| Capa | Crate | Carpeta | Archivos | Lineas | Usa | Lo usan | Marcados |
|---|---|---|---|---|---|---|---|
| 0 | `quimera-comprimir` | `cimientos/comprimir` | 3 | 447 |  | quimera-imagen, quimera-texto, quimera-red | 0 |
| 0 | `quimera-valor` | `cimientos/valor` | 1 | 378 |  | quimera-banco-gc, quimera-gc, quimera-objetos, quimera-vm | 0 |
| 0 | `quimera-gpu` | `grafico/gpu` | 2 | 772 |  | quimera-pintar, quimera-ventana | 0 |
| 0 | `quimera-vector` | `grafico/vector` | 5 | 1232 |  | quimera-imagen, quimera-maquetacion | 0 |
| 0 | `quimera-video` | `grafico/video` | 6 | 881 |  | quimera-ventana, quimera-dom | 0 |
| 0 | `quimera-guardianes` | `herramientas/guardianes` | 8 | 1508 |  |  | 0 |
| 0 | `quimera-regex` | `motor-js/regex` | 7 | 1838 |  | quimera-parser, quimera-vm, quimera-red | 0 |
| 0 | `quimera-css` | `web/css` | 8 | 1768 |  | quimera-banco-web, quimera-dom, quimera-estilo, quimera-maquetacion | 0 |
| 0 | `quimera-html` | `web/html` | 9 | 1846 |  | quimera-banco-web, quimera-pintar, quimera-ver, quimera-ventana, quimera-dom, quimera-estilo, quimera-maquetacion | 0 |
| 1 | `quimera-banco-web` | `bancos/web` | 1 | 223 | quimera-css, quimera-html |  | 1 |
| 1 | `quimera-imagen` | `grafico/imagen` | 5 | 1568 | quimera-comprimir, quimera-vector | quimera-banco-imagen, quimera-ventana | 0 |
| 1 | `quimera-texto` | `grafico/texto` | 2 | 533 | quimera-comprimir | quimera-pintar, quimera-ventana | 0 |
| 1 | `quimera-gc` | `motor-js/gc` | 2 | 702 | quimera-valor | quimera-banco-gc, quimera-objetos, quimera-vm | 3 |
| 1 | `quimera-parser` | `motor-js/parser` | 13 | 3347 | quimera-regex | quimera-frontend | 0 |
| 1 | `quimera-red` | `red/red` | 13 | 2832 | quimera-comprimir, quimera-regex | quimera-ventana | 0 |
| 1 | `quimera-estilo` | `web/estilo` | 15 | 3515 | quimera-css, quimera-html | quimera-pintar, quimera-ver, quimera-ventana, quimera-maquetacion | 0 |
| 2 | `quimera-banco-gc` | `bancos/gc` | 1 | 192 | quimera-gc, quimera-valor |  | 0 |
| 2 | `quimera-banco-imagen` | `bancos/imagen` | 1 | 349 | quimera-imagen |  | 0 |
| 2 | `quimera-frontend` | `motor-js/frontend` | 10 | 2608 | quimera-parser | quimera-banco-frontend, quimera-banco-vm, quimera-vm | 1 |
| 2 | `quimera-objetos` | `motor-js/objetos` | 6 | 1508 | quimera-gc, quimera-valor | quimera-vm | 0 |
| 2 | `quimera-maquetacion` | `web/maquetacion` | 19 | 4490 | quimera-css, quimera-estilo, quimera-html, quimera-vector | quimera-pintar, quimera-ver, quimera-ventana | 0 |
| 3 | `quimera-banco-frontend` | `bancos/frontend` | 1 | 100 | quimera-frontend |  | 7 |
| 3 | `quimera-pintar` | `grafico/pintar` | 2 | 341 | quimera-estilo, quimera-gpu, quimera-html, quimera-maquetacion, quimera-texto | quimera-ventana | 0 |
| 3 | `quimera-ver` | `herramientas/ver` | 1 | 146 | quimera-estilo, quimera-html, quimera-maquetacion |  | 0 |
| 3 | `quimera-vm` | `motor-js/vm` | 28 | 8373 | quimera-frontend, quimera-gc, quimera-objetos, quimera-regex, quimera-valor | quimera-banco-vm, quimera-ventana, quimera-dom | 2 |
| 4 | `quimera-banco-vm` | `bancos/vm` | 1 | 343 | quimera-frontend, quimera-vm |  | 24 |
| 4 | `quimera-dom` | `web/dom` | 15 | 5771 | quimera-css, quimera-html, quimera-video, quimera-vm | quimera-ventana | 1 |
| 5 | `quimera-ventana` | `navegador/ventana` | 18 | 3720 | quimera-dom, quimera-estilo, quimera-gpu, quimera-html, quimera-imagen, quimera-maquetacion, quimera-pintar, quimera-red, quimera-texto, quimera-video, quimera-vm |  | 0 |

## Lo mas grande (lineas de Rust)

| Archivo | Lineas |
|---|---|
| `motor-js/vm/tests/moderno.rs` | 682 |
| `web/dom/src/pruebas.rs` | 671 |
| `navegador/ventana/src/navegador.rs` | 669 |
| `web/maquetacion/src/bloque.rs` | 655 |
| `grafico/gpu/src/lib.rs` | 651 |
| `web/estilo/src/propiedades.rs` | 640 |
| `web/dom/src/lib.rs` | 624 |
| `motor-js/parser/src/lexico.rs` | 622 |
| `web/dom/src/puente.rs` | 614 |
| `web/maquetacion/src/tabla.rs` | 599 |

## Lo mas tocado (commits que lo cambiaron)

| Archivo | Cambios |
|---|---|
| `navegador/ventana/src/main.rs` | 24 |
| `motor-js/vm/src/nativas/mod.rs` | 19 |
| `navegador/ventana/src/navegador.rs` | 17 |
| `web/dom/src/lib.rs` | 17 |
| `web/maquetacion/src/bloque.rs` | 16 |
| `motor-js/vm/src/lib.rs` | 15 |
| `web/dom/src/pruebas.rs` | 15 |
| `motor-js/vm/src/propiedades.rs` | 14 |
| `motor-js/vm/src/interprete.rs` | 13 |
| `motor-js/objetos/src/mundo.rs` | 12 |

## Historia

67 commits, del 2026-10-31 al 2026-11-04. De 449 a 741 archivos y de 361783 a 570626 lineas (segun git).

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
