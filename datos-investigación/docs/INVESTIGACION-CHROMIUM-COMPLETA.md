# Chromium completo en SUPER GRAFO: cómo está construido

31-10-2026. Segunda pasada, ahora con las dependencias de `gclient` dentro (V8, Skia, ANGLE, Dawn, ffmpeg, BoringSSL…). Ejecutada en el taller QEMU con el mismo binario `ultra-carpetas` (SHA-256 `f1919181…5f00`) en modo MCP, como UID 1000. Script: `scripts/analizar-chromium-completo.py`. 109 consultas, ninguna falló.

Datos crudos: `evidencias/chromium-real/resultados-completo/` y `evidencias/chromium-real/chromium-completo.sarif`.

## Cifras

| | 1.ª pasada (solo principal) | 2.ª pasada (con dependencias) |
|---|---:|---:|
| Entradas indexadas | 558.218 | 1.064.375 |
| Archivos de código leídos | 160.231 | 319.871 |
| Código leído | 1.398 MiB | 3.498 MiB |
| Dependencias (#include / use) | 1.271.361 | 2.044.874 |
| Ciclos de cabeceras | 30 | 82 (32 dependen de un `#if`) |
| Señales Seguridad / Rendimiento / Elegancia | 352 / 6.620 / 18.649 | 3.718 / 10.199 / 91.443 |
| Resultados SARIF (error / aviso / nota) | 67 / 285 / 25.269 | 935 / 2.783 / 101.642 |
| Tiempo de análisis | 51,5 s | 54,0 s (reutilizó 160.231 de caché) |
| Recortado | no | no |

Revisión de Chromium: `e9b6e64b6fa3072793adc5d651051a2b8dd054ed`, clon superficial. Dependencias: `gclient sync` salió con código 0 (solo Git; sin CIPD, GCS ni hooks). `compile_commands` sigue en null: todo es mapa léxico.

## Las capas y cómo se usan

Tamaño de cada capa (archivos que el motor relacionó):

| Capa | Archivos | Papel |
|---|---:|---|
| `chrome` | 35.384 | El producto: perfiles, UI del navegador, funciones |
| `third_party/blink` | 14.285 | Motor web: DOM, CSS, layout, bindings JS |
| `ui` | 6.212 | Ventanas, eventos, geometría |
| `third_party/skia` | 6.136 | Dibujo 2D |
| `content` | 5.850 | Multiproceso: navegador ↔ renderizador, embebible |
| `net` | 4.515 | HTTP, QUIC, TLS, caché |
| `v8` | 4.376 | JavaScript / WebAssembly |
| `base` | 3.180 | Hilos, tareas, memoria, callbacks |
| `services` | 2.784 | Servicios separados (red, audio, datos…) |
| `cc`, `gpu`, `components/viz` | 1.115 / 1.108 / 749 | Composición y GPU |
| `mojo`, `ipc` | 741 / 58 | Comunicación entre procesos |
| `sandbox` | 481 | Aislamiento |

Aristas directas más gruesas (capa → capa que usa):

- `content → base` 18.954, `blink → base` 12.548, `cc → base` 2.260, `mojo → base` 1.897
- `content → ui` 2.690, `content → mojo` 2.434, `content → blink` 2.427, `content → net` 2.385, `content → services` 1.593
- `blink → ui` 1.685, `blink → mojo` 1.139, `blink → cc` 843, `blink → skia` 732, `blink → v8` 636
- `cc → ui` 725, `cc → viz` 696, `cc → skia` 603; `viz → gpu` 476
- `blink → content` solo 11 y `mojo → blink` 11: Blink casi no conoce a `content`. La frontera la cruza Mojo.

El orden de construcción que muestran los datos es:

```
base ─► mojo/ipc ─► net, services, url ─► v8, skia ─► cc/viz/gpu ─► blink ─► content ─► chrome
                         (sandbox se apoya en base; content lo usa: 216 aristas)
```

### Piezas que lo sostienen todo (peso = archivos que se recompilan si cambia)

| Archivo | Directos | Se recompilan |
|---|---:|---:|
| `build/build_config.h` | 11.269 | 120.623 |
| `base/memory/raw_ptr.h` | 17.601 | 112.173 |
| `base/logging.h` | 7.117 | 99.961 |
| `base/functional/callback.h` | 9.021 | 83.344 |
| `base/task/sequenced_task_runner.h` | 4.376 | 58.431 |
| `url/gurl.h` | 8.036 | 54.128 |
| `ui/gfx/geometry/rect.h` | 2.077 | 37.404 |
| `mojo/public/cpp/bindings/remote.h` | 2.318 | 26.459 |
| `net/base/net_errors.h` | 1.606 | 22.201 |
| `third_party/skia/include/core/SkCanvas.h` | 1.205 | 12.926 |
| `content/public/browser/web_contents.h` | 4.847 | 12.693 |
| `v8/include/v8-isolate.h` | 188 | 11.594 |
| `chrome/browser/profiles/profile.h` | 6.013 | 11.451 |
| `blink/.../heap/garbage_collected.h` | 2.318 | 10.082 |

Conclusión: Chromium es, ante todo, `base` (punteros vigilados `raw_ptr`, callbacks, tareas por secuencia) + Mojo. Cualquier Chromium propio en Rust necesita primero esos contratos; las capas de arriba son clientes de ellos.

### Caminos entre capas

- `browser.h` → `web_contents.h`: uso directo (1 salto). `chrome` es cliente de `content`.
- `web_contents.h` ↔ `document.h`: **ninguno usa al otro**; solo comparten `scoped_refptr.h`. El navegador no ve el DOM: habla con el renderizador por Mojo.
- `document.h` → `v8-isolate.h`: 3 saltos. Blink depende de V8, no al revés.
- `local_frame.h` ↔ `layer_tree_host.h`: solo comparten `raw_ptr.h`. Blink entrega capas a `cc` mediante interfaces.
- `layer_tree_host.h` → `SkCanvas.h`: 3 saltos. `cc` dibuja con Skia.
- `resource_request.h` ↔ `url_request.h`: solo comparten `gurl.h`. El servicio de red está separado de `net`.
- `sandbox.h` → `raw_ptr.h`: 2 saltos, con un paso imprescindible: es el único sitio donde se corta limpio.

## Rust que Chromium ya tiene

- 9.688 archivos `.rs`; 332 crates externos en `third_party/rust/chromium_crates_io/vendor`.
- 424 archivos GN (fuera de `vendor`) declaran objetivos Rust: 287 en `third_party/rust`, 52 en `build/rust`, 11 en `mojo/public`, más `pdfium`, `blink`, `webrtc`, `skia`, `net`, `url`, `media`, `services/data_decoder`, `base/logging`.
- Puentes `cxx::bridge` con C++ en: `mojo/public/rust` (8), `skia/rust` (5), `webrtc` (4+), `pdfium` (4), `angle` (3), `base/logging/rust_logger` (3), `blink/renderer` (2), `content/browser/security` (2), `url/gurl.rs` y `url/origin.rs`.
- Usos reales: decodificadores de imagen (PNG, JPEG XL, AVIF vía `crabbyavif`), fuentes (`skrifa`, `read-fonts`, `harfrust`), ICU4X y `temporal_rs` (fechas para V8), JSON (`serde_json_lenient`), QR, Mojo en Rust.

Es decir: Chromium ya mete Rust **en las hojas** (analizadores de datos no confiables) y lo conecta con `cxx`. No hay Rust en `base`, `content`, Blink core ni V8.

## Cuánto fiarse de las señales

Las señales son léxicas. Comprobadas a mano en el código:

| Señal | Archivo | Veredicto |
|---|---|---|
| UB-015, causa n.º 1 y 2 del fallo conectado (≈112.000 archivos) | `base/numerics/safe_conversions_impl.h:91` y la copia de PartitionAlloc | **Falso**: la escritura en `*(volatile char*)0` está en la rama `#else` que no compila con GCC, Clang ni MSVC. El motor no evalúa `#if`. |
| UB-015 | `sandbox/linux/seccomp-bpf/die.cc:43` | **A propósito**: provoca el cierre del proceso. |
| UB-004 `iov[2]..iov[6]` | `components/crash/.../crash_handler_host_linux.cc:209` | **Falso**: el arreglo real es `iov[kCrashIovSize]` (6 o 7); el motor tomó otro `iov` de 2 elementos. |
| UB-005 `checksum` | `chrome/browser/ash/login/hwid_checker.cc:38` | **Falso**: la función devuelve `std::string`, que copia el búfer. |
| UB-004 "0 elementos, último índice 2¹²⁸−1" | 75 errores (lzma, libpng, Blink…) | **Falso y fallo del motor**: `n - 1` con `n = 0` en `museo.rs:998`. |

Por tanto, la cifra "se pueden romper: 150.179" del fallo conectado está inflada casi por completo por esas dos cabeceras. La seguridad de Chromium no se puede juzgar con este SARIF sin filtrar.

Además, **el motor no tiene ninguna regla para `.rs`**: 0 resultados en archivos Rust. Las "señales Rust" de cada carpeta vienen de los `.hpp/.cc` que hay dentro (bindings de ICU4X, por ejemplo).

## SUPER GRAFO mejorado (31-10-2026)

Cambios en `Linux Reconstruido/src/apps/carpetas-nucleo` (sin commit), con pruebas para cada caso real de Chromium:

1. **UB-004**: cada uso se mide contra la declaración de su propia función; un tamaño calculado (`iov[kCrashIovSize]`) también cuenta como declaración; se ignoran los arreglos flexibles `x[0]`/`x[1]` al final de una estructura; `} campo[32];` ya no se lee como acceso; desaparece el desbordamiento `n - 1`.
2. **UB-005**: `return buf;` no se marca si la función devuelve `std::string`, `String`, `std::vector`…; sí se marca si devuelve un puntero o una vista (`string_view`, `span`).
3. **`#if`**: se saltan las ramas que ningún GCC ni Clang compila (`#if 0`, `!defined(__clang__)`, el `#else` tras `defined(__GNUC__)`). `_MSC_VER` y las macros del proyecto quedan como "no se sabe" y se siguen leyendo.
4. **Rust**: lexer propio (vidas, cadenas crudas, comentarios anidados) y 6 fichas nuevas en `Museo/triangulo.json`: RS-001 `unsafe` sin `// SAFETY:` (Elegancia), RS-002 `transmute`, RS-003 `static mut`, RS-004 `mem::uninitialized/zeroed`, RS-005 `from_raw_parts` (Seguridad), RS-006 `unwrap/expect` fuera de pruebas (Elegancia).
5. `VERSION_CACHE` sube a 3 para que la caché no reutilice señales de detectores viejos.

Pruebas: 113/113 en Windows; en el taller 112/113 (la que falla lee `CASM/`, que no se copió a la compilación aislada). Binario nuevo en el taller: `/var/tmp/ultra-main-work/cache/carpetas-target-v2/release/ultra-carpetas` (SHA-256 `bba42033…5af9`); el original queda intacto.

### Chromium antes y después (misma revisión, mismas 109 consultas)

| | Antes | Después |
|---|---:|---:|
| Errores SARIF | 935 | 423 |
| UB-004 / UB-005 / UB-015 | 695 / 111 / 36 | 226 / 11 / 26 |
| "Último índice 2¹²⁸−1" | 75 | 0 |
| Fallo conectado: se pueden romper | 150.179 | 22.495 |
| Resultados en `.rs` | 0 | 17.323 (12.129 en crates `vendor`) |
| RS-002 / 003 / 004 / 005 | — | 421 / 68 / 194 / 439 |

Los seis falsos positivos verificados a mano ya no aparecen. Las causas principales del fallo conectado son ahora concretas: `from_raw_parts` y `transmute` en `boringssl/src/rust`, CXX-002 en ANGLE `cl_types.h` y UB-005 en ANGLE `Common.h`, todas pendientes de revisar a mano.

Datos: `evidencias/chromium-real/resultados-v3/` y `chromium-completo-v3.sarif`.

### Pendiente

- Cargar `compile_commands.json` (generado con `gn gen`) para resolver los `#if` de verdad y validar includes.
- `cronologia` no sirve: el clon superficial atribuye todo al último commit. Hace falta `git fetch --deepen` o `--shallow-since` por intervalos.

## Qué significa para un Chromium propio en Rust

Los datos apuntan a seguir el mismo orden en que Chromium está construido, sin intentar sustituir Blink o V8 de golpe:

1. **Contratos de `base` en Rust**: tareas por secuencia, callbacks, punteros vigilados. Son las piezas de más peso (58.000–112.000 archivos dependen de ellas), así que una versión Rust debe ofrecer la misma interfaz por `cxx`.
2. **Mojo en Rust** ya existe parcialmente (`mojo/public/rust`). Es la frontera entre procesos y el lugar natural para crecer.
3. **Hojas no confiables**: imágenes, fuentes, formatos de medios, analizadores. Chromium ya lo hace; seguir esa línea es lo que da más seguridad por línea escrita.
4. **Servicios separados** (`services/*`): están aislados por Mojo, así que se pueden reescribir uno a uno.
5. Blink, V8, `cc` y Skia quedan en C++ hasta tener lo anterior; cambiarlos es escribir un motor nuevo.

Para ULTRA-LinuX, la pieza propia encaja debajo de `base` (plataforma) y en `sandbox` (que en los datos se corta limpio), igual que propone `docs/PLAN-CHROMIUM-RUST.md`.

## Estado del taller

- El taller se arrancó con el disco de trabajo `main-escritorio-taller.qcow2`. `/dev/vdb` no se monta solo: se montó a mano en `/var/tmp/ultra-main-work`.
- Se reutilizó el servidor HTTP local de la sesión anterior (puerto 18765).
- El SARIF completo también queda en el taller: `/var/tmp/ultra-main-work/chromium-personal/src/super-grafo-chromium-completo.sarif`.
