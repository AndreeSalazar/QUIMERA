# QUIMERA

> Quería un **Chromium personal**: por eso esta carpeta todavía se llama «Chromiun personal». Lo que salió no es Chromium. Es **QUIMERA**: un navegador propio, escrito desde cero en Rust.

## Por qué existe

Chrome es enorme. Son unos 35 millones de líneas, con 30 años de lastre (APIs que casi nadie usa, compatibilidades con páginas de 2005), y gasta cientos de MB por pestaña. Además decide Google, no yo.

**QUIMERA no viene a reemplazar a Chrome, viene a desplazarlo.** No intenta hacer todo lo que hace Chrome: hace bien lo que yo uso (leer, buscar, Steam, Gmail, YouTube) y **manda a Chrome a la miércoles en consumo de RAM**. :3

- **Consume poco:** pensado desde el diseño para gastar poca memoria y batería.
- **Control total:** el código es mío y se entiende entero; hace lo que yo quiero y nada más.
- **Privado:** no manda nada a nadie (ni siquiera lo escrito en la barra va a un buscador).
- **Seguro:** Rust, sin errores de memoria; código pequeño y revisado por los guardianes.
- **Para ULTRA-LinuX**, con interfaz VERX (Bun + Rust).

Se llama QUIMERA porque es una criatura hecha de varios animales: el frontend de JavaScript viene de las ideas de **Boa**, la ejecución del diseño de **SpiderMonkey**, la arquitectura de **Servo**, y lo aprendido de **Chromium**. Ninguno se copia; de cada uno se toma lo mejor.

## La memoria, medida

Misma página, ventana visible, Windows, `--release`. Edge 156 es Chromium por dentro, con un perfil nuevo y sin extensiones:

| Página | QUIMERA | Edge (Chromium) |
|---|---:|---:|
| example.com | **110 MB**, 1 proceso | 463 MB, 9 procesos |
| The Rust Book | **115 MB** | 486 MB |
| Wikipedia, «Cat» (51 imágenes, 278 idiomas) | **259 MB** | 515–821 MB |
| Hacker News (todo tablas) | **113 MB** | 456 MB |

Memoria en uso de todos los procesos. **Hoy es una comparación a favor de QUIMERA:** todavía no ejecuta el JavaScript de las páginas, y Edge sí. La medida se repite en cada fase del [plan](docs/PLAN.md) para que la ventaja se mantenga.

Lo que bajó la memoria: las fuentes de reserva (chino, japonés, coreano, emoji: 70 MB) se leen solo cuando una página las usa, y en Windows se arranca solo DirectX 12 en vez de todos los sistemas gráficos (246 → 111 MB en example.com).

## Qué hace ya

```bash
cargo run --release -p quimera-ventana -- https://en.wikipedia.org/wiki/Cat
```

- Abre páginas por **HTTP y HTTPS**, con barra de dirección, enlaces que se pulsan, historial (Alt+← / Alt+→) y carga en otro hilo.
- **HTML** con el mismo árbol que html5ever en 5.000 páginas reales, y 2,2× más rápido.
- **CSS:** cascada, herencia, `@media`, bloques con márgenes que colapsan, texto en líneas, listas, **flexbox**, **tablas**, **flotantes** y **`position`/`z-index`** como Chromium (Hacker News se ve igual; en Wikipedia, la ficha a la derecha del texto).
- **Imágenes** PNG, JPEG y GIF con decodificador propio, al píxel igual que las bibliotecas de referencia. Se descargan en segundo plano.
- **JavaScript** propio, 3,8× más rápido que Boa en los benchmarks de V8, con cachés en línea al estilo CacheIR de SpiderMonkey. Aún no está conectado a las páginas: eso es la fase 3 del plan.
- Dibuja con la GPU (wgpu) y fuentes del sistema.

## Qué falta: el plan

Mis sitios son el examen. Cada fase deja algo que ya sirve. Detalle en [docs/PLAN.md](docs/PLAN.md).

| Fase | Qué | Desbloquea |
|---|---|---|
| ✅ 0 | Motor JS, HTML, CSS, flexbox, red, imágenes, ventana | leer páginas sencillas |
| 1 | ✅ Tablas, ✅ `float`, ✅ `position` · `var()`, `@font-face`, gzip, WebP | Wikipedia, documentación, blogs |
| 2 | Formularios, cookies, foco y teclado, caché | buscar (DuckDuckGo HTML): navegar de verdad |
| 3 | **JavaScript ↔ DOM** (puente generado desde WebIDL, un solo GC), bucle de eventos, `fetch` | Steam y páginas con JavaScript |
| 4 | Web Components, `grid`, SVG, inicio de sesión | Epic Games, Gmail |
| 5 | `<video>` con los decodificadores del sistema, audio | **YouTube** |
| 6 | Toque, zoom y `viewport` | **Android**, donde un navegador ligero más se nota |

En paralelo: **VERX** como interfaz, y QUIMERA como motor de interfaces. Steam, Discord y Spotify cargan un Chromium entero solo para dibujar sus botones; para una interfaz propia, QUIMERA sirve mucho antes que para YouTube.

## Estado por crate

| Crate | Nivel | Qué es | Estado |
|---|---:|---|---|
| `cimientos/valor` → `quimera-valor` | 0 | Valor JS de 64 bits, `Copy`, sin contador | ✅ 8 pruebas |
| `motor-js/gc` → `quimera-gc` | 1 | GC generacional: guardería en arena + zona principal también en un solo bloque (sin `malloc` por objeto), barrera de escritura; anota las cadenas vivas | ✅ 10 pruebas |
| `motor-js/regex` → `quimera-regex` | 1 | Expresiones regulares: patrón → programa → máquina con vuelta atrás sin recursión ([docs/REGEX.md](docs/REGEX.md)) | ✅ 6 pruebas |
| `motor-js/frontend` → `quimera-frontend` | 2 | JavaScript → bytecode de registros (Stencil): léxico, parser, ámbitos, compilador, literales `/re/` | ✅ 24 pruebas |
| `motor-js/objetos` → `quimera-objetos` | 3 | Formas, cadenas únicas que se liberan, objetos, arreglos densos, funciones y entornos sobre el GC, `delete` en su sitio | ✅ 10 pruebas |
| `motor-js/vm` → `quimera-vm` | 4 | Intérprete, cachés en línea al estilo CacheIR (prototipos, polimórficas, `length`, transiciones), superinstrucciones, ToPrimitive, `finally`/`for-in`/`for-of`/`delete`, `RegExp` y biblioteca de colecciones; perfil por instrucción | ✅ 34 pruebas |
| `web/html` → `quimera-html` | 2 | HTML5 → árbol, con la recuperación de errores de los navegadores; mismo árbol que html5ever en 5.000 páginas reales, 2,2× más rápido ([docs/HTML-CSS.md](docs/HTML-CSS.md)) | ✅ 10 pruebas |
| `web/css` → `quimera-css` | 2 | CSS → reglas, `@media`, selectores con especificidad, colores y longitudes | ✅ 9 pruebas |
| `web/estilo` → `quimera-estilo` | 8 | La cascada: hoja del navegador, `<style>`, `<link>`, `style=`, `@media`, especificidad, herencia, atajos, flexbox, tablas, `float`, `position`, modo de compatibilidad ([docs/ESTILO-MAQUETACION.md](docs/ESTILO-MAQUETACION.md)) | ✅ 13 pruebas |
| `web/maquetacion` → `quimera-maquetacion` | 9 | Bloques con márgenes que colapsan, texto en líneas, listas, **flexbox**, **tablas**, **flotantes**, **`position`** y capas de `z-index`, tamaños intrínsecos, **imágenes** (proporción, `max-width`) → lista de dibujo | ✅ 30 pruebas |
| `herramientas/ver` → `quimera-ver` | 100 | Una página → imagen SVG: [la primera página dibujada por QUIMERA](docs/ejemplos/primera.svg) | ✅ |
| `grafico/texto` → `quimera-texto` | 2 | Fuentes del sistema (Windows, Linux): medir, interletraje, rasterizar, reserva para símbolos; cada archivo se lee solo cuando hace falta ([docs/VENTANA.md](docs/VENTANA.md)) | ✅ 3 pruebas |
| `grafico/imagen` → `quimera-imagen` | 2 | PNG, JPEG (también progresivo) y GIF → RGBA, sin dependencias; mismos píxeles que las bibliotecas de referencia en 115 imágenes ([docs/IMAGENES.md](docs/IMAGENES.md)) | ✅ 5 pruebas |
| `grafico/gpu` → `quimera-gpu` | 2 | wgpu: una tubería que dibuja cuadros de color, letras del atlas e imágenes (una textura cada una); captura sin ventana | ✅ 4 pruebas |
| `grafico/pintar` → `quimera-pintar` | 10 | Lista de dibujo → cuadros, atlas de letras, medidas reales para la maquetación | ✅ 2 pruebas |
| `navegador/ventana` → `quimera-ventana` | 12 | La ventana (winit): barra de dirección, enlaces que se pulsan, historial, carga en otro hilo, imágenes en segundo plano; dibuja con wgpu | ✅ 6 pruebas |
| `red/red` → `quimera-red` | 2 | URLs, HTTP/1.1 propio y HTTPS con rustls (certificados de Mozilla); `file:` y `data:` ([docs/RED.md](docs/RED.md)) | ✅ 9 pruebas (+2 con internet) |
| `bancos/frontend` → `quimera-banco-frontend` | 99 | Compara el frontend con el parser de Boa | ✅ 1 prueba |
| `bancos/web` → `quimera-banco-web` | 99 | Compara `quimera-html` con html5ever en páginas reales | ✅ |
| `bancos/vm` → `quimera-banco-vm` | 99 | Compara la VM con boa_engine y comprueba RegExp contra Boa | ✅ 1 prueba |
| `bancos/gc` → `quimera-banco-gc` | 99 | Compara `quimera-gc` con `boa_gc` | ✅ 1 prueba |
| `bancos/imagen` → `quimera-banco-imagen` | 99 | Compara `quimera-imagen` con `png`, `jpeg-decoder` y `gif` | ✅ 1 prueba |
| `herramientas/guardianes` → `quimera-guardianes` | 100 | Semáforo rojo/amarillo/verde, 1000 líneas por archivo, jerarquía | ✅ 12 pruebas |

## Medidas del motor JavaScript

**El GC**, con la misma carga estilo Splay (200.000 pasos, 5 millones de objetos, Windows, `--release`, tres corridas):

| | Total | Pasos de más de 1 ms | Peor pausa |
|---|---:|---:|---:|
| `quimera-gc` | 522–571 ms | 25–48 | 14–19 ms |
| `boa_gc` 0.22 | 667–729 ms | 79–81 | 10–22 ms |

El GC de QUIMERA es **1,26–1,36× más rápido** y tiene un tercio de los pasos lentos. La peor pausa aún depende de la recolección mayor, que marca todo de una vez; marcarla por partes (como SpiderMonkey) es la siguiente mejora del GC.

El frontend, con el corpus de Boa y V8 (los 8 benchmarks de Octane) repetido hasta 8,2 MB:

| | Texto → árbol | Velocidad |
|---|---:|---:|
| **`quimera-frontend`** | **483 ms** | **16,9 MB/s** |
| parser de Boa 0.22 | 1.906 ms | 4,3 MB/s |

**3,9× más rápido** (con la salvedad de que Boa cubre todo ECMAScript; ver [docs/FRONTEND.md](docs/FRONTEND.md)).

**Ejecutando JavaScript** (los benchmarks de Boa, mismos resultados en los dos motores):

| | Media geométrica frente a boa_engine 0.22 |
|---|---:|
| **`quimera-vm`** | **3,08× más rápido** (mediana de tres corridas, los dos con LTO) |
| **V8 (Octane), los 8 benchmarks con su arnés completo** | **3,81× la puntuación de Boa** (mediana de tres corridas, los dos con LTO; de 2,23× en NavierStokes a 12× en RegExp; gana en los 8) |

Ver [docs/VM.md](docs/VM.md).

## Estructura

```text
QUIMERA (esta carpeta)
  Cargo.toml              workspace
  cimientos/              nivel 0: el valor JS
  motor-js/               niveles 1-4: gc, regex, frontend, objetos, vm
  web/                    html, css, estilo, maquetación         (dom: fase 3)
  grafico/                texto, imagen, gpu (wgpu), pintar
  red/                    HTTP y HTTPS
  navegador/              la ventana                              (+ interfaz VERX)
  bancos/                 comparaciones contra otros motores y bibliotecas
  herramientas/           guardianes y quimera-ver
  guardianes.toml         amarillos revisados, cada uno con su motivo
  .githooks/pre-commit    guardianes + formato + clippy + pruebas en cada commit
  datos-investigación/    la investigación archivada: Chromium, Servo, Boa, SpiderMonkey, SUPER GRAFO
```

Documentos:

| | |
|---|---|
| [docs/PLAN.md](docs/PLAN.md) | el plan por fases, los sitios del examen y qué es el DOM |
| [docs/JERARQUIA.md](docs/JERARQUIA.md) | todos los niveles, el orden y por qué wgpu |
| [docs/GUARDIANES.md](docs/GUARDIANES.md) | las reglas y por qué existen |
| [docs/FRONTEND.md](docs/FRONTEND.md), [docs/VM.md](docs/VM.md), [docs/REGEX.md](docs/REGEX.md) | el motor JavaScript |
| [docs/HTML-CSS.md](docs/HTML-CSS.md), [docs/ESTILO-MAQUETACION.md](docs/ESTILO-MAQUETACION.md) | HTML, CSS, cascada y maquetación |
| [docs/RED.md](docs/RED.md), [docs/IMAGENES.md](docs/IMAGENES.md), [docs/VENTANA.md](docs/VENTANA.md) | red, imágenes, ventana y GPU |

## Comandos

```bash
cargo run --release -p quimera-ventana -- https://example.com/
```

Sin ventana, dibuja la página en un PNG:

```bash
cargo run --release -p quimera-ventana -- https://example.com/ --captura captura.png 1100 800
```

```bash
cargo test
```

Las comparaciones:

```bash
cargo run --release -p quimera-banco-vm
```

```bash
cargo run --release -p quimera-banco-frontend
```

```bash
cargo run --release -p quimera-banco-gc
```

```bash
cargo run --release -p quimera-banco-web
```

```bash
cargo run --release -p quimera-banco-imagen
```

```bash
cargo run -p quimera-guardianes -- --estricto
```

Una vez por copia del repositorio, para que cada commit pase por los guardianes:

```bash
git config core.hooksPath .githooks
```

## Los guardianes

Ningún archivo pasa de 1000 líneas, un crate solo usa crates de niveles menores, `unsafe` solo donde se permite y siempre con `// SAFETY:`, y cada amarillo (unwrap, allow, TODO, funciones largas...) se revisa con un motivo. Un semáforo rojo/amarillo/verde lo muestra por crate y el commit no pasa con un rojo. Ver [docs/GUARDIANES.md](docs/GUARDIANES.md) y [docs/JERARQUIA.md](docs/JERARQUIA.md).

## Investigación

Todo lo anterior está en [datos-investigación/](datos-investigación/README.md):

- [cómo está construido Chromium](datos-investigación/docs/INVESTIGACION-CHROMIUM-COMPLETA.md) y [su jerarquía completa](datos-investigación/datos/chromium-jerarquia/LEEME.md);
- [Servo, Boa y SpiderMonkey medidos](datos-investigación/docs/INVESTIGACION-MOTORES-RUST.md);
- [el perfil de Boa y el diseño de QUIMERA](datos-investigación/docs/PERFIL-BOA-Y-MOTOR-PROPIO.md).
