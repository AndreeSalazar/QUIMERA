# Análisis de la base real

## Identidad y alcance

Origen: `C:/Users/Salazar/Documents/Linux Reconstruido`. Rama `main`, HEAD `4778c674a43511dbb5cd166f1e8859182bddc4a4`, 200 commits alcanzables desde HEAD y 2.846 archivos seguidos. Git declara `is-shallow-repository=false`; esto no garantiza que existan ramas remotas nunca descargadas. La carpeta de destino estaba vacía y no era repositorio Git.

ULTRA es una distribución experimental x86-64: kernel Linux upstream, entorno Arch, PID 1 propio C, raíz EROFS y escritorio Hyprland/Wayland. Es la plataforma donde integrar y medir el futuro navegador. No hace falta reconstruir toda la distribución para estudiar su grafo.

## Mapa de reutilización

Todas las rutas siguientes son relativas a `referencia/ULTRA-LinuX/`.

| Pieza real | Qué aporta | Aplicación al navegador |
|---|---|---|
| `src/apps/carpetas-nucleo/src/lib.rs` | `#![no_std]`, `alloc`, `forbid(unsafe_code)`, estado/acciones/efectos/vista | Modelo para lógica propia portable; no es motor HTML/JS |
| `src/apps/carpetas-nucleo/src/dependencias.rs` | Dependencias y motivos con líneas | Investigar fronteras entre módulos antes de migrar |
| `src/apps/carpetas-nucleo/src/cronologia.rs` | Reconstrucción temporal del mapa | Relacionar aparición de dependencias y commits |
| `src/apps/ultra-carpetas/src/consultas.rs` | Herramientas JSON acotadas | Consultas reproducibles sin capturas repetidas |
| `src/apps/ultra-carpetas/src/mcp.rs` | Servidor MCP y recursos | Acceso de asistentes al mapa y cambios |
| `src/apps/ultra-carpetas/src/indice.rs` | Índice de archivos | Explorar el checkout de Chromium una vez adquirido |
| `src/apps/ultra-carpetas/Cargo.toml` | eframe/wgpu/inotify, capa de plataforma | Prueba de que la interfaz depende del sistema |
| `scripts/revisar-proyecto.py` | Inventario y revisión C aislada | Patrón de banco para adaptadores y FFI |
| `docs/ESTADOS-Y-VIGENCIA.md` | Identidad de fuentes, includes y herramientas | Invalidar evidencia cuando cambia el contexto |
| `docs/REVISION-PROYECTO-REAL.md` | Alcance y límites de la revisión instalada | Evitar confundir revisión parcial con aprobación global |
| `scripts/taller_qemu.py`, `conectar-taller.ps1` | Taller Arch y SSH local | Compilar y medir en Linux sin depender de WSL |
| `scripts/probar_super_grafo_instalado.py` | Prueba sintética del ejecutable instalado | Verificar índice, caché, diagnósticos y cronología |
| `docs/SUPER-GRAFO.md`, `docs/ULTRA-CARPETAS.md` | Diseño, consultas y límites | Manual de investigación por grupos y causas |

## Qué enseña el SUPER GRAFO

En el código actual aparecen 22 herramientas, aunque algunos documentos históricos dicen 14. Incluyen `resumen`, `archivo`, `camino`, `peso`, `combinar`, `ciclos`, `cronologia`, `gcc_deps`, `gcc_diagnosticos`, `revision_estado` y exportación SARIF. El catálogo extraído queda en `evidencias/herramientas-super-grafo.csv`.

El método documentado es recorrer e indexar el proyecto completo y responder preguntas pequeñas. Por eso se conserva toda la referencia en disco, y se pide al grafo un resumen, después un grupo y luego archivos concretos. Exportar indiscriminadamente cada consulta no equivale a entender todas las causas.

Los motivos de una arista pueden incluir línea y texto de `include/use/import`; el motivo humano del diseño se busca en Git y documentos. El propio plan señala límites en evaluación de `#if`, actualización incremental y particionado del índice. La cobertura y tiempos publicados para Chromium son mediciones históricas del autor, no mediciones hechas ahora sobre esta carpeta vacía.

## Rust y Chromium

El núcleo de Carpetas usa `core` y `alloc`, incluyendo serde sin características predeterminadas. `no_std` no significa ausencia de asignaciones: el anfitrión debe proporcionar asignador si usa `alloc`. No aporta por sí solo archivos, red, hilos, GPU ni sandbox.

Chromium mantiene integración de Rust con C++, GN y sus herramientas de compilación. La propuesta es mantener el motor existente y definir módulos Rust propios con contratos pequeños. Cargo sirve para probar esos módulos independientemente; no sustituye GN/Ninja para compilar Chromium.

Referencias oficiales consultadas el 31-10-2026:

- [Rust en Chromium](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/rust/README.md).
- [Interoperabilidad Rust/C++](https://www.chromium.org/Home/chromium-security/memory-safety/rust-and-c-interoperability/).
- [Biblioteca alloc de Rust](https://doc.rust-lang.org/stable/alloc/).

## Evidencia y límites actuales

`evidencias/estado-qemu.json` del origen describe un estado temprano del taller, mientras documentos posteriores registran arranques y entregas completas. Deben leerse con su fecha y commit; el JSON antiguo no es el estado actual global.

La comprobación actual alcanza copia Git, historial, integridad y hashes. QEMU está presente y ejecutándose, pero SSH no respondió correctamente. No se han ejecutado compiladores dentro del invitado, consultas nuevas, benchmarks ni pruebas de Chromium. La RTX física y consumo/FPS necesitan mediciones independientes de QEMU con gráficos software.

El bundle conserva el historial disponible y las referencias locales. El manifiesto inventaría la referencia por ruta, objeto Git y SHA-256 de los archivos materializados. No es una copia del disco completo ni contiene los archivos ignorados.
