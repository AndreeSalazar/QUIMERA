# Servo, Boa y SpiderMonkey con SUPER GRAFO

31-10-2026. Los tres proyectos se descargaron en el taller QEMU y se analizaron con el SUPER GRAFO mejorado (binario `carpetas-target-v2`, sin caché). Boa se compiló y se midió contra SpiderMonkey 140, el shell oficial `js140` de Arch.

Datos: `datos/motores/` (respuestas del SUPER GRAFO, SARIF, mapa de crates y carpetas, mediciones). Scripts: `scripts/analizar-proyecto-super-grafo.py`, `scripts/mapa-crates.py` y `scripts/medir-motores-js.py`.

## Los tres de un vistazo

| | Boa | SpiderMonkey (`js/src`) | Servo |
|---|---|---|---|
| Qué es | Motor JavaScript en Rust | Motor JavaScript de Firefox, en C++ | Motor de navegador en Rust |
| Revisión | `329e8974` (historia completa) | `servo/mozjs` `6e69c162` | `585585e3` |
| Código propio | 222.583 líneas Rust | 1.101.870 líneas C/C++ | 513.707 líneas Rust |
| Crates (niveles, ciclos) | 24 (9, 0) | 14 envoltorios Rust (7, 0) | 80 (9, 0) |
| Líneas con `unsafe` | 596 | — (C++) | 2.363 |
| SUPER GRAFO: archivos leídos | 763 | 1.894 | 5.179 (de 209.145; el resto son pruebas WPT) |
| Dependencias detectadas | 993 | 10.108 | 9.803 |
| Ciclos de cabeceras | 0 | 0 | 0 |
| Seguridad / Elegancia | 7 / 476 | 18 / 1.685 | 32 / 4.893 |
| Errores SARIF | 0 | 0 | 0 |

Los tres están mucho más limpios que Chromium (423 errores y 4.265 señales de Seguridad).

## Boa: cómo está hecho

**Orden de construcción** (mapa de crates, nivel 0 = no usa otros crates de Boa):

```
0  boa_string, boa_macros, small_btree, tag_ptr, boa_icu_provider
1  boa_gc            recolector de basura
2  boa_interner      tabla de nombres
3  boa_ast           árbol de sintaxis (21.600 líneas)
4  boa_parser        analizador (27.142)
5  boa_engine        el motor (141.360 líneas, 277 con unsafe)
6  boa_wintertc, boa_wasm
7  boa_runtime       console, fetch... (6.971)
8  boa_cli, boa_tester, boa_benches
```

**Por dentro** (`core/engine/src`):

- **Máquina virtual de bytecode** con 264 instrucciones, mezcla de pila y registros (`vm/opcode`).
- **Valores con NaN-boxing**: un `f64` lleva dentro punteros y enteros (`value/inner/nan_boxed.rs`).
- **Shapes y cachés en línea polimórficas** para acceder a propiedades (`object/shape`, `vm/inline_cache`).
- **Optimizador del AST**: plegado de constantes, eliminación de código muerto y reducción de fuerza.
- **GC mark-sweep** (`boa_gc`): no tiene generaciones ni mueve objetos.
- **Expresiones regulares** con el crate `regress` 0.12, un intérprete.
- **No tiene JIT.**
- Solo `boa_interner` y `boa_icu_provider` son `no_std`. `boa_engine` usa `std`.

**Historia** (`cronologia` del SUPER GRAFO): nace el 08-08-2018 con un commit de Jason Williams. En febrero de 2020 pasa a workspace de crates (#247) y en septiembre de 2020 añade test262 (#567). Suma 3.939 commits de 292 autores.

**Señales de Seguridad** (7, todas en fronteras de memoria): `from_raw_parts` en `array_buffer/shared.rs` y `boa_string`, y `mem::zeroed` en `dataview/mod.rs`. Todas son para revisar, ninguna es un fallo confirmado.

## Boa contra SpiderMonkey: velocidad medida

Los mismos 8 benchmarks V8 (Octane), en la misma VM de 4 núcleos. La puntuación es mayor cuanto mejor; cada benchmark corre unos 2 s.

| Motor | Media geométrica | Frente a Boa |
|---|---:|---:|
| Boa 0.22 (sin JIT) | 302 | 1× |
| SpiderMonkey, solo intérprete C++ | 1.000 | 3,3× |
| SpiderMonkey, + Baseline Interpreter | 2.215 | 7,3× |
| SpiderMonkey completo (Baseline JIT + Warp) | 24.458 | **81×** |

| Benchmark | Intérprete SM / Boa | SM completo / Boa |
|---|---:|---:|
| Richards | 1,8× | 76× |
| DeltaBlue | 2,1× | 84× |
| NavierStokes | 2,3× | 56× |
| Crypto | 2,8× | 103× |
| RayTrace | 3,0× | 89× |
| Splay | 3,4× | 29× |
| EarleyBoyer | 3,8× | 109× |
| RegExp | 15,4× | 175× |

Memoria máxima:

- Al arrancar, Boa usa ~28 MB y SpiderMonkey ~16 MB.
- En las pruebas que reservan mucho, Boa usa **menos** que SpiderMonkey completo: Splay 278 MB frente a 402 MB, y EarleyBoyer 74 MB frente a 174 MB.

Límite: en el modo "solo intérprete" no se desactivó la compilación nativa de expresiones regulares (`--no-native-regexp`), así que en RegExp SpiderMonkey sigue compilando.

## SpiderMonkey: cómo está hecho

Tamaño de `js/src` por carpeta (líneas no vacías de C/C++):

| Carpeta | Líneas | Qué hace |
|---|---:|---|
| `jit/` | 410.206 | Los compiladores. 60.543 son solo ARM64; hay backends para x64, x86, ARM, RISC-V, LoongArch y MIPS |
| `vm/` | 149.028 | Intérprete C++ (`Interpreter.cpp`), objetos y runtime |
| `builtin/` | 109.913 | Array, Map, Promise, Intl... |
| `wasm/` | 107.306 | WebAssembly con su propio Baseline e Ion |
| `frontend/` | 72.811 | Parser, `BytecodeEmitter` y *Stencil* (bytecode serializable, sin GC) |
| `gc/` | 47.391 | GC generacional y compactante |
| `irregexp/` | 11.996 | Expresiones regulares (adaptado de V8, compila a código máquina) |

**Las cuatro capas de ejecución** (lo que mide la tabla de arriba):

1. **Intérprete C++**: arranca al instante y no genera código.
2. **Baseline Interpreter**: el mismo bytecode, pero con cachés en línea compartidas. Ganó 2,2×.
3. **Baseline JIT**: traduce cada instrucción a código máquina, como una plantilla.
4. **WarpMonkey** (el antiguo IonMonkey): compilador optimizador (`MIR` → `LIR`, `RangeAnalysis`, `ValueNumbering`, `ScalarReplacement`). Lo construye a partir de lo que observaron las cachés (`WarpOracle`, `WarpCacheIRTranspiler`).

La pieza que lo une todo es **CacheIR**: un lenguaje pequeño para las cachés en línea que comparten las tres capas superiores. Una propiedad se aprende una vez y se reutiliza al subir de capa.

**GC**:

- Las asignaciones nuevas van al *nursery* (`Nursery.cpp`); lo que sobrevive pasa a la zona principal (`Tenuring.cpp`).
- Marcado incremental y paralelo (`ParallelMarking.cpp`), compactación (`Compacting.cpp`) y barreras (`Barrier.cpp`).

**SUPER GRAFO**: 18 señales de Seguridad (14 `ctype` con `char`, 3 `volatile`, 1 vector invalidado), 0 ciclos y 0 errores.

## Servo: cómo está hecho

| Componente | Líneas Rust | Papel |
|---|---:|---|
| `script` | 251.947 | DOM y APIs web: la mitad de Servo |
| `layout` | 46.066 | Maquetación (usa `taffy` para flex/grid) |
| `shared` | 31.076 | Tipos compartidos entre procesos |
| `net` | 21.135 | Red (hyper, rustls) |
| `script_webcrypto` | 20.495 | WebCrypto |
| `media` | 16.774 | Audio y vídeo |
| `script_bindings` | 11.945 | Puente entre el DOM y SpiderMonkey |
| `servo` | 10.339 | La API para embeberlo |
| `fonts` | 9.756 | Fuentes (`skrifa`, `read-fonts`) |
| `constellation` | 8.080 | Coordina pestañas y procesos |
| `paint` | 5.371 | Pintado hacia WebRender |

Las piezas más famosas **no están en el repositorio**, son crates aparte:

- `stylo`: CSS, el mismo de Firefox.
- `webrender`: dibujado por GPU.
- `html5ever`: parser de HTML.
- `mozjs`: SpiderMonkey.

**Servo está atado a SpiderMonkey**: 808 archivos usan su API directamente (694 en `script`). Para cambiar de motor JS habría que reescribir los bindings del DOM.

**SUPER GRAFO**: 32 señales de Seguridad, casi todas en `script_bindings`, la frontera con C++ (`transmute`, `from_raw_parts`). Sin ciclos.

## Qué significa para tu navegador

**¿Se puede mejorar Boa?** Sí, y los datos dicen en qué orden. Cada paso tiene como referencia una capa medida de SpiderMonkey:

| Paso | Qué cambiar en Boa | Referencia medida | Ganancia esperable |
|---|---|---|---|
| 1 | Intérprete más rápido: despacho de instrucciones, más registros y menos pila, menos copias de valores | Intérprete C++ de SM | hasta ~2,7× (sin RegExp) |
| 2 | Expresiones regulares: compilarlas (o un motor tipo irregexp en Rust) | RegExp: 15× | el benchmark más lento |
| 3 | Cachés en línea compartidas al estilo CacheIR | Baseline Interpreter: 2,2× sobre el intérprete | ~2× más |
| 4 | GC con *nursery* (generacional) | Splay, EarleyBoyer | menos pausas; Boa ya gasta menos memoria pico |
| 5 | JIT de plantilla (Baseline JIT), por ejemplo con Cranelift, en Rust | De 7× a 81× | el salto grande, y el más costoso |

Los pasos 1 a 4 mantienen el motor sin JIT. Así es más pequeño, más seguro y gasta menos memoria, que es justo tu objetivo de consumir menos. Para páginas personales, un intérprete 3 a 7 veces más rápido que el Boa actual ya es útil. El JIT (paso 5) solo hace falta para webs muy pesadas.

**Arquitectura recomendada** (inspirada en Servo, no copiada):

- **Motor JS detrás de una interfaz propia desde el día uno.** Servo no la tiene, y por eso no puede cambiar SpiderMonkey. Con la interfaz, empiezas con Boa y lo mejoras, o lo cambias, sin tocar el DOM.
- **Reutilizar crates de Servo que ya son independientes**: `html5ever` (HTML), `stylo` (CSS), `taffy` (maquetación), `skrifa`/`read-fonts` (fuentes) y, si hace falta GPU, `webrender`.
- **Escribir tu propio DOM**, más pequeño que el `script` de Servo (252.000 líneas): solo las APIs que uses.
- **Interfaz con VERX** (Bun + JSX compilado a HTML) dentro de la ventana de `verx-desktop`.

## Pendiente

- Medir Boa con `--no-native-regexp` en SpiderMonkey, para aislar el intérprete de expresiones regulares.
- Compilar Servo (`./mach build --release`) y medir memoria con páginas reales. Necesita más RAM que la VM actual.
- Perfilar Boa (`perf`) en Crypto y EarleyBoyer para saber qué instrucciones dominan antes de tocar el intérprete.
