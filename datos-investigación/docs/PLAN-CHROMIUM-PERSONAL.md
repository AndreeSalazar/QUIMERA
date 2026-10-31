# Plan: Chromium personal en Rust `no_std`, pieza a pieza y aislado

31-10-2026. Se basa en los datos de `datos/chromium-jerarquia/` (jerarquía completa y orden de construcción) y en `docs/INVESTIGACION-CHROMIUM-COMPLETA.md` (análisis del SUPER GRAFO). Amplía `docs/PLAN-CHROMIUM-RUST.md` con lo que ya se ha medido.

## La idea en una frase

No se reescribe Chromium de golpe. Se sigue su **orden de construcción real** (207 niveles de objetivos GN): cada pieza se aísla detrás de su interfaz, se escribe en Rust propio y se sustituye solo cuando pasa las mismas pruebas que la original. En cada momento hay un navegador que arranca.

## Por qué así

- Chromium tiene 987.266 archivos y 19.246 objetivos de producción solo para Linux. Un motor nuevo desde cero tardaría años antes de abrir una página.
- Los datos muestran que todo descansa en pocas piezas: `base` (`raw_ptr`, callbacks, tareas), Mojo y `url`. Si cambias `raw_ptr.h`, se recompilan 112.173 archivos. Esas son las primeras que hay que controlar.
- Chromium ya hace esto mismo con Rust (decodificadores, fuentes, ICU4X, `cxx`). Seguir esa vía significa usar mecanismos que su compilación ya admite.
- `no_std` encaja con ULTRA: el núcleo no depende de la biblioteca estándar de Linux, solo de una capa de plataforma propia.

## Qué significa aislar una pieza

1. **Elegir por objetivos GN**, no por carpeta (`objetivos-orden.tsv.gz`). Las carpetas mezclan niveles; los objetivos no.
2. **Congelar su interfaz**: cabeceras públicas y mensajes `.mojom`. SUPER GRAFO (`archivo`, `peso`, `camino`) dice quién la usa y por qué línea.
3. **Copiar sus pruebas**: las de Chromium (`*_unittests`) son el contrato.
4. **Escribir el crate Rust** con la misma interfaz, `#![no_std]` + `alloc`, y un puente `cxx` hacia C++.
5. **Sustituir con un interruptor GN** (`use_ultra_<pieza> = true`): la original sigue disponible para comparar.
6. **Medir**: mismas pruebas, SUPER GRAFO sin señales de Seguridad nuevas, tamaño y tiempo.

## Fases (de abajo arriba, siguiendo los niveles)

| Fase | Piezas de Chromium | Niveles de sus objetivos | Crate propio | Listo cuando |
|---|---|---|---|---|
| 0. Taller | Compilar Chromium sin cambios para Linux en QEMU; `gn gen` con `compile_commands.json` | — | — | `chrome --headless` abre una página local; SUPER GRAFO con `compile_commands` |
| 1. Plataforma ULTRA | Lo que `base` pide al sistema: memoria, hilos, tiempo, archivos, señales | `base` 0–37 | `ultra-plataforma` (`no_std`, syscalls de ULTRA) | `base_unittests` pasa con la plataforma propia en Linux y en ULTRA |
| 2. Cimientos | `base/memory`, `base/functional`, `base/task`, `base/containers` | `base` 0–37 | `ultra-base` | Mismas pruebas; las 112.173 dependencias de `raw_ptr.h` compilan sin cambios |
| 3. Comunicación | Mojo (núcleo y bindings), `ipc` | `mojo` 0–76, `ipc` 0–61 | `ultra-ipc` (ya hay `mojo/public/rust` como punto de partida) | Un proceso hijo intercambia mensajes `.mojom` con el navegador |
| 4. Datos no confiables | Imágenes, fuentes, JSON, URL, formatos de medios | `url` 0–62 y hojas de `third_party` | `ultra-formatos` | Cada decodificador pasa sus pruebas y fuzzers; Chromium ya tiene Rust aquí |
| 5. Red | `net` (HTTP, caché, cookies); TLS sigue en BoringSSL | `net` 0–50 | `ultra-red` | Navegación HTTPS real en el servicio de red aislado |
| 6. Aislamiento | `sandbox` (seccomp-bpf) y política de procesos | `sandbox` 0–48 | `ultra-aislamiento` usando el núcleo de ULTRA | Renderizador sin acceso a archivos ni red; pruebas de escape |
| 7. Producto | La interfaz propia (pestañas, ajustes, descargas) sobre `content` | `chrome` hasta 203 | `navegador-nucleo` (ya previsto en el plan anterior) | Navegador personal usable en ULTRA |
| 8. Motores (opcional, largo) | Composición (`cc`, `viz`), Blink, V8, Skia | `cc` 0–93, `viz` 0–100, Blink 0–114, V8 0–48 | Solo piezas concretas, nunca el motor entero de golpe | Cada sustitución con las pruebas web de Chromium (`web_tests`, WPT) |

Las fases 1 a 6 dan **control absoluto de todo lo que toca el sistema** (memoria, procesos, red, aislamiento). Blink y V8 pueden quedarse en C++ mucho tiempo sin perder ese control, porque viven dentro del renderizador aislado.

## Piezas que hay que mover juntas

Ciclos entre objetivos para Linux (`aislamiento-produccion.json`):

- `ui/ozone` + `ui/gl` + `gpu/vulkan` + `ui/base` (11 objetivos): ventana y GPU. Se sustituyen como una sola pieza de plataforma gráfica (Wayland/Hyprland en ULTRA).
- `components/crash` + `gwp_asan` (4 objetivos): informes de fallos.
- `device/fido` + `services/device/.../hid` (3 objetivos).
- Dos parejas en `chrome/browser` (ajustes de contenido ↔ extensiones; gestión de UI).

## Carpetas del proyecto

```text
Chromiun personal/
  datos/chromium-jerarquia/   # jerarquía completa, objetivos GN y orden de construcción (esta entrega)
  docs/                       # investigación y planes
  evidencias/chromium-real/   # resultados del SUPER GRAFO y SARIF
  scripts/                    # análisis reproducibles
  referencia/ULTRA-LinuX/     # referencia del sistema
  crates/                     # futuro: un crate por pieza, cada uno no_std + alloc
    ultra-plataforma/
    ultra-base/
    ultra-ipc/
    ultra-formatos/
    ultra-red/
    ultra-aislamiento/
    navegador-nucleo/
  puentes/                    # futuro: cxx y objetivos GN que conectan cada crate con Chromium
  parches-chromium/           # futuro: interruptores use_ultra_* y cambios mínimos a BUILD.gn
```

El checkout de Chromium vive en el disco del taller (`/var/tmp/ultra-main-work/chromium-personal/src`) y no se modifica: los cambios van como parches en `parches-chromium/`, para poder actualizar la revisión de Chromium.

## Siguiente paso concreto

**Fase 0**: compilar Chromium sin cambios en el taller. Hace falta:

1. Ampliar la VM: Chromium pide unos 100 GB de disco y 16 GB de RAM o más para enlazar (ahora hay 14 GB libres y 4 GB de RAM).
2. Ejecutar los hooks de `gclient` (`gclient runhooks`) que se omitieron, para tener toolchain Clang y Rust de Chromium.
3. `gn gen out/Linux --args='is_debug=false symbol_level=0' --export-compile-commands` para tener `compile_commands.json` (los interruptores `use_ultra_*` llegan con la fase 1).
4. Repetir el SUPER GRAFO con `compile_commands.json`: resuelve los `#if` de verdad y las cabeceras generadas.

## Licencias

De los 319 componentes de `third_party`: 95 Apache-2.0, 71 BSD-3-Clause, 46 MIT, 13 BSD-2-Clause, 7 LGPL-2.1, 6 GPL-2.0, y 13 sin `README.chromium`. ULTRA es GPL-2.0-only. Antes de distribuir, revisar la compatibilidad pieza a pieza (`plan-componentes.tsv`, columna `licencia`).
