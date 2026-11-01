# QUIMERA

Navegador personal escrito en Rust, construido desde cero y por niveles, para ULTRA-LinuX. El objetivo es **consumir poco y tener control total**, no copiar Chromium entero.

Su motor JavaScript mezcla el frontend de **Boa** con el diseño de ejecución de **SpiderMonkey**. Por eso se llama QUIMERA: una criatura hecha de varios animales (una serpiente, una araña y un mono).

## Estado

| Crate | Nivel | Qué es | Estado |
|---|---:|---|---|
| `cimientos/valor` → `quimera-valor` | 0 | Valor JS de 64 bits, `Copy`, sin contador | ✅ 8 pruebas |
| `motor-js/gc` → `quimera-gc` | 1 | GC generacional: guardería en arena + zona principal, barrera de escritura; anota las cadenas vivas | ✅ 11 pruebas |
| `motor-js/regex` → `quimera-regex` | 1 | Expresiones regulares: patrón → programa → máquina con vuelta atrás sin recursión ([docs/REGEX.md](docs/REGEX.md)) | ✅ 6 pruebas |
| `motor-js/frontend` → `quimera-frontend` | 2 | JavaScript → bytecode de registros (Stencil): léxico, parser, ámbitos, compilador, literales `/re/` | ✅ 24 pruebas |
| `motor-js/objetos` → `quimera-objetos` | 3 | Formas, cadenas únicas que se liberan, objetos, arreglos densos, funciones y entornos sobre el GC, `delete` en su sitio | ✅ 10 pruebas |
| `motor-js/vm` → `quimera-vm` | 4 | Intérprete, cachés en línea, superinstrucciones, ToPrimitive, `finally`/`for-in`/`for-of`/`delete`, `RegExp` y biblioteca de colecciones | ✅ 29 pruebas |
| `web/html` → `quimera-html` | 2 | HTML5 → árbol, con la recuperación de errores de los navegadores; mismo árbol que html5ever en 5.000 páginas reales, 2,2× más rápido ([docs/HTML-CSS.md](docs/HTML-CSS.md)) | ✅ 10 pruebas |
| `web/css` → `quimera-css` | 2 | CSS → reglas, `@media`, selectores con especificidad, colores y longitudes | ✅ 9 pruebas |
| `web/estilo` → `quimera-estilo` | 8 | La cascada: hoja del navegador, `<style>`, `<link>`, `style=`, `@media`, especificidad, herencia, atajos, flexbox ([docs/ESTILO-MAQUETACION.md](docs/ESTILO-MAQUETACION.md)) | ✅ 9 pruebas |
| `web/maquetacion` → `quimera-maquetacion` | 9 | Bloques con márgenes que colapsan, texto en líneas, listas, **flexbox**, tamaños intrínsecos → lista de dibujo | ✅ 11 pruebas |
| `herramientas/ver` → `quimera-ver` | 100 | Una página → imagen SVG: [la primera página dibujada por QUIMERA](docs/ejemplos/primera.svg) | ✅ |
| `grafico/texto` → `quimera-texto` | 2 | Fuentes del sistema (Windows, Linux): medir, interletraje, rasterizar, reserva para símbolos ([docs/VENTANA.md](docs/VENTANA.md)) | ✅ 1 prueba |
| `grafico/gpu` → `quimera-gpu` | 2 | wgpu: una tubería que dibuja cuadros de color y letras del atlas en una pasada; captura sin ventana | ✅ 1 prueba |
| `grafico/pintar` → `quimera-pintar` | 10 | Lista de dibujo → cuadros, atlas de letras, medidas reales para la maquetación | ✅ 2 pruebas |
| `navegador/ventana` → `quimera-ventana` | 12 | La ventana (winit): barra de dirección, enlaces que se pulsan, historial, carga en otro hilo; dibuja con wgpu | ✅ 4 pruebas |
| `red/red` → `quimera-red` | 2 | URLs, HTTP/1.1 propio y HTTPS con rustls (certificados de Mozilla); `file:` y `data:` ([docs/RED.md](docs/RED.md)) | ✅ 9 pruebas (+2 con internet) |
| `bancos/frontend` → `quimera-banco-frontend` | 99 | Compara el frontend con el parser de Boa | ✅ 1 prueba |
| `bancos/web` → `quimera-banco-web` | 99 | Compara `quimera-html` con html5ever en páginas reales | ✅ |
| `bancos/vm` → `quimera-banco-vm` | 99 | Compara la VM con boa_engine y comprueba RegExp contra Boa | ✅ 1 prueba |
| `bancos/gc` → `quimera-banco-gc` | 99 | Compara `quimera-gc` con `boa_gc` | ✅ 1 prueba |
| `herramientas/guardianes` → `quimera-guardianes` | 100 | Semáforo rojo/amarillo/verde, 1000 líneas por archivo, jerarquía | ✅ 12 pruebas |

Primera medida, con la misma carga estilo Splay (200.000 pasos, 5 millones de objetos, Windows, `--release`, tres corridas):

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
| **`quimera-vm`** | **2,81× más rápido** (de 1,13× en nested-loop a 5,60× en closures-create) |
| **V8 (Octane), los 8 benchmarks con su arnés completo** | **3,33× la puntuación de Boa** (de 1,78× en NavierStokes a 12,5× en RegExp; gana en los 8) |

Ver [docs/VM.md](docs/VM.md).

## Estructura

```text
QUIMERA (esta carpeta)
  Cargo.toml              workspace
  cimientos/              nivel 0: valor, cadenas, plataforma
  motor-js/               niveles 1-6: gc, frontend, objetos, ic, vm, regex, builtins, js
  web/                    html, css, dom, estilo, maquetación      (planeado)
  grafico/                gpu (wgpu), texto, pintar                (planeado)
  red/  proceso/          red, aislamiento                         (planeado)
  navegador/              navegador, ventana + interfaz VERX       (planeado)
  bancos/                 medidas contra otros motores
  herramientas/guardianes los guardianes: semáforo, límites y jerarquía
  guardianes.toml         amarillos revisados, cada uno con su motivo
  .githooks/pre-commit    guardianes + formato + clippy + pruebas en cada commit
  docs/JERARQUIA.md       todos los niveles, el orden y por qué wgpu
  docs/GUARDIANES.md      las reglas y por qué existen
  docs/FRONTEND.md        cómo funciona el frontend, qué entiende y su medida
  docs/VM.md              la máquina virtual: diseño, medida y qué falta
  datos-investigación/    la investigación archivada: Chromium, Servo, Boa, SpiderMonkey, SUPER GRAFO
```

## Comandos

```bash
cargo test
```

```bash
cargo run --release -p quimera-banco-gc
```

```bash
cargo run --release -p quimera-banco-frontend
```

```bash
cargo run --release -p quimera-banco-vm
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
