# Chromium dentro del SUPER GRAFO: ejecución real

31-10-2026. Se corrigió el objetivo de la primera entrega: cargar las fuentes de Chromium en la aplicación del autor, analizar la raíz y mostrar su Ciudad 3D.

## Fuentes

- Origen público: https://github.com/chromium/chromium.git, espejo del proyecto Chromium.
- Revisión: `e9b6e64b6fa3072793adc5d651051a2b8dd054ed`.
- Checkout principal: 514.576 entradas seguidas por Git: 514.304 archivos y 272 gitlinks a componentes externos.
- Historial superficial: solo la revisión descargada; no es la historia completa de Chromium.
- Taller: `/var/tmp/ultra-main-work/chromium-personal/src`.
- Destino ULTRA: `/home/ultra/Chromium/src`, en el disco exclusivo `vm/chromium-fuentes.qcow2`.

## Primera pasada real sobre toda la raíz principal

Se ejecutó el binario existente de ULTRA Carpetas en modo MCP, como UID 1000, dentro del taller QEMU. El cliente consultó su catálogo real y guardó las respuestas, sin usar capturas para deducir las cifras.

| Resultado informado por SUPER GRAFO | Valor |
|---|---:|
| Entradas indexadas, incluidos directorios | 558.218 |
| Archivos de código leídos por el motor | 160.231 |
| Código leído, MiB aproximados | 1.398,3 |
| Dependencias detectadas | 1.271.361 |
| Tiempo de análisis | 51.508 ms |
| Análisis recortado | false |
| Ciclos detectados | 30 |
| Ciclos dependientes de un `#if` | 6 |
| Señales de Seguridad | 352 |
| Señales de Rendimiento | 6.620 |
| Señales de Elegancia | 18.649 |
| Resultados exportados a SARIF | 25.621 |

Las cifras originales están en `evidencias/chromium-real/resultados-main-taller/resumen.json`. La identidad y SHA-256 del ejecutable están en `identidad.json`. El SARIF completo se conserva en `evidencias/chromium-real/chromium-main.sarif`.

Las señales son sospechas y patrones para investigar, no vulnerabilidades confirmadas. No se ejecutó Chromium ni se midieron FPS. `compile_commands` es null en esta pasada: las dependencias son el mapa léxico del código presente, sin validar todavía toda la configuración Clang y las cabeceras generadas.

## Qué significa analizar toda la raíz

Se pasó la raíz completa al motor, sin seleccionar únicamente `base/` ni un ejemplo. El índice y la Ciudad abarcan archivos presentes; el análisis de referencias reconoce los lenguajes que implementa actualmente el programa: familias C/C++/ASM, Rust y Python. Los archivos de JavaScript, TypeScript, GN y otros formatos pueden ser visibles en la Ciudad sin que eso implique un analizador semántico para ellos.

El motor omite enlaces, archivos ocultos, cachés, directorios de construcción marcados y archivos mayores de 2 MiB para su lectura textual. La Ciudad admite hasta 600.000 edificios y puede agrupar archivos en bloques al superar su límite. `recortado: false` se refiere a los presupuestos de esta pasada; no demuestra cobertura semántica universal.

## Dependencias externas y vista 3D

El repositorio principal incluye Blink, pero V8 y otras bibliotecas llegan por `DEPS`. Se preparó una sincronización oficial con gclient para las dependencias Git de Linux, sin hooks ni paquetes binarios CIPD/GCS. La sincronización tiene un log y código de salida propios; no se dará por completa por el solo hecho de tener descargado el árbol principal.

La segunda VM arranca el kernel `7.3.0-rc7-pc-g32c52ec87d62` y la imagen ULTRA del autor en modo snapshot. Se comprobó la ventana real `ultra-carpetas` dentro de Hyprland. El cargador copia las fuentes verificadas al disco exclusivo y ejecuta el motor instalado de esa imagen. La evidencia de Ciudad debe proceder de esa ejecución, una vez terminada la transferencia.

## Consultar el porqué de cada parte

Usar el servidor MCP sobre la raíz completa y pedir `archivo`, `camino`, `peso`, `combinar` o `senales` por grupo. El índice conserva el proyecto completo y cada respuesta tiene tope. Para motivos humanos anteriores a la revisión descargada se necesitará ampliar Git por intervalos o consultar commits upstream; el clon superficial no puede explicar una historia que no contiene.
