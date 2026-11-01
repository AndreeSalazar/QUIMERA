# La jerarquía de QUIMERA

QUIMERA se construye **de abajo arriba, por niveles**, igual que se construye Chromium por dentro: los datos de `datos-investigación/datos/chromium-jerarquia/` muestran 207 niveles de objetivos GN. Aquí son pocos niveles y cada uno con un propósito claro.

**La regla**: un crate solo usa crates de niveles **menores**. Así cada pieza se puede probar, medir y sustituir sola. La comprueban los guardianes (`herramientas/guardianes`, ver [GUARDIANES.md](GUARDIANES.md)), que también vigilan los crates de fuera, `no_std`, `unsafe` y el límite de 1000 líneas por archivo.

## Capas y niveles

`✅` hecho · `🔨` siguiente · resto: planeado. El nivel decide qué puede usar cada crate; la capa es la carpeta.

| Nivel | Capa (carpeta) | Crate | Qué hace | Inspiración | `no_std` |
|---:|---|---|---|---|---|
| 0 | `cimientos` | ✅ `quimera-valor` | Valor JS de 64 bits, `Copy`, sin contador | SpiderMonkey `JS::Value` | sí |
| 0 | `cimientos` | `quimera-cadenas` | Cadenas Latin-1/UTF-16 compartidas (hoy: UTF-8 únicas y liberables, en `quimera-objetos`) | Boa `boa_string` | sí |
| 0 | `cimientos` | `quimera-plataforma` | Lo que se pide al sistema (tiempo, hilos, archivos) como *traits*; Linux y ULTRA lo implementan | Chromium `base` | sí |
| 1 | `motor-js` | ✅ `quimera-gc` | GC generacional: guardería en arena + zona principal | SpiderMonkey `gc/Nursery` | sí |
| 1 | `motor-js` | ✅ `quimera-regex` | Expresiones regulares compiladas (no depende de nada: la usan el frontend y la VM) | SpiderMonkey irregexp | sí |
| 2 | `motor-js` | ✅ `quimera-frontend` | Parser y AST de JavaScript → bytecode tipo *Stencil* | Boa `boa_parser`, `boa_ast`; SpiderMonkey `frontend/Stencil` | sí |
| 2 | `web` | ✅ `quimera-html` | Tokenizador y constructor de árbol HTML5 | html5ever (Servo) | sí |
| 2 | `web` | ✅ `quimera-css` | Parser de CSS y selectores | Stylo (Servo) | sí |
| 2 | `grafico` | ✅ `quimera-gpu` | GPU con **wgpu**: superficies, texturas, *pipelines* | WebRender, Servo | no (wgpu usa `std`) |
| 2 | `grafico` | ✅ `quimera-texto` | Fuentes y *shaping* | skrifa, harfrust | sí |
| 2 | `grafico` | ✅ `quimera-imagen` | PNG, JPEG y GIF → RGBA, sin dependencias | image-rs, stb_image, libjpeg | sí |
| 2 | `red` | ✅ `quimera-red` | HTTP/1.1, HTTP/2, TLS (rustls), caché | Chromium `net` | no |
| 3 | `motor-js` | ✅ `quimera-objetos` | *Shapes*, elementos densos, propiedades | SpiderMonkey `NativeObject` | sí |
| 3 | `motor-js` | ✅ `quimera-ic` → dentro de `quimera-vm` (`ic.rs`) | Cachés en línea con un mini-lenguaje común: guardas de forma por la cadena de prototipos + acción. Vive en la VM porque necesita los objetos (nivel 3) y no puede ser un crate del mismo nivel | SpiderMonkey CacheIR | sí |
| 3 | `proceso` | `quimera-aislamiento` | Procesos, IPC y sandbox (seccomp en ULTRA) | Chromium `sandbox`, `mojo` | no |
| 4 | `motor-js` | ✅ `quimera-vm` | Intérprete de registros, un único bucle | SpiderMonkey `Interpreter.cpp` | sí |
| 5 | `motor-js` | `quimera-builtins` | Array, Map, Promise, JSON... | Boa `builtins` | sí |
| 6 | `motor-js` | `quimera-js` | **La interfaz del motor**: lo único que ve el resto del navegador | — (Servo no la tiene) | sí |
| 7 | `web` | `quimera-dom` | DOM y eventos, conectados al motor por `quimera-js` | Servo `script` (mucho más pequeño) | sí |
| 8 | `web` | ✅ `quimera-estilo` | Cascada: qué valor tiene cada propiedad en cada elemento | Stylo | sí |
| 9 | `web` | ✅ `quimera-maquetacion` | Bloques, línea (después flex y grid) → lista de dibujo; usa el estilo, por eso va un nivel encima | Servo `layout`, taffy | sí |
| 10 | `grafico` | ✅ `quimera-pintar` | Lista de dibujo → GPU con `quimera-gpu` | WebRender | no |
| 11 | `navegador` | `quimera-navegador` | Pestañas, navegación, historial, descargas | Chromium `content` | no |
| 12 | `navegador` | 🔨 `quimera-ventana` | Ventana nativa (Wayland/Hyprland en ULTRA) + interfaz VERX | `verx-desktop` | no |
| 99 | `bancos` | ✅ `quimera-banco-gc`, `quimera-banco-frontend` | Medidas contra otros motores | — | no |
| 100 | `herramientas` | ✅ `quimera-guardianes` | Semáforo, límites y jerarquía | guardián de VERX, SUPER GRAFO | no |

## Por qué wgpu

Sí, **wgpu** es la API gráfica correcta para QUIMERA:

- **Es Rust** y es la base de WebGPU en Firefox y en Servo (Servo usa `wgpu-core` 30; lo vimos en su `Cargo.toml`).
- **Funciona en ULTRA**: en Linux usa Vulkan, con OpenGL ES de reserva, y abre ventanas en Wayland.
- **Sirve dos veces**:
  1. Para **pintar las páginas**: `quimera-pintar` convierte la maquetación en una lista de rectángulos, texto e imágenes, y la dibuja con *shaders* en la GPU, como hace WebRender. Es mucho más ligero que pintar en CPU.
  2. Más adelante, para dar **WebGPU a las páginas** con el mismo crate.
- **Límite**: wgpu necesita `std`, así que la capa gráfica no es `no_std`. El motor JS y el DOM siguen siéndolo. La plataforma de ULTRA solo tiene que dar Vulkan o GL.

## Orden de construcción recomendado

1. **Motor JS hasta `quimera-js`** (niveles 0–6), medido en cada paso contra Boa y SpiderMonkey con `bancos/`. Ya se puede usar solo, sin navegador.
2. **HTML + CSS + DOM + maquetación** (niveles 2, 7, 8), con páginas de prueba en modo texto.
3. **wgpu y pintado** (niveles 2 y 9): la primera página dibujada en una ventana.
4. **Red y aislamiento** (niveles 2–3): navegar por HTTPS con el renderizador en su propio proceso.
5. **Navegador e interfaz VERX** (niveles 10–11).

## Qué se mide en cada paso

- Pruebas unitarias en cada crate (`cargo test`).
- Benchmarks en `bancos/`, siempre contra una referencia ya medida (Boa, SpiderMonkey `js140`).
- Los guardianes en modo estricto en cada commit (`.githooks/pre-commit`).
- SUPER GRAFO sobre el propio QUIMERA (`ultra-carpetas --mcp`), para vigilar dependencias, `unsafe` y señales.
