# Datos de la jerarquía completa de Chromium

Revisión `e9b6e64b6fa3072793adc5d651051a2b8dd054ed` (31-10-2026), con las 450 dependencias de gclient (448 carpetas presentes; 86 son repositorios internos de Google, que no se pueden clonar sin acceso). Generado en el taller QEMU por `scripts/archivar-jerarquia-chromium.py` y aquí por `scripts/aislar-componentes.py`. No se compiló ni ejecutó nada de Chromium.

| | |
|---|---:|
| Archivos | 987.266 |
| Carpetas | 82.910 |
| Componentes (carpetas de 1.º o 2.º nivel) | 1.291 |
| `BUILD.gn` | 8.924 |
| Objetivos GN | 35.682 |
| Dependencias GN | 278.731 |
| Objetivos de producción para Linux | 19.246 |
| Niveles de construcción (Linux, por objetivo) | 207 |
| Ciclos entre objetivos (Linux) | 6, el mayor de 11 |

## Archivos

| Archivo | Qué contiene |
|---|---|
| `archivos.tsv.gz` | Cada archivo: ruta, bytes, extensión, lenguaje, componente |
| `carpetas.tsv.gz` | Cada carpeta: profundidad, componente, capa, repositorio gclient de origen, archivos directos y totales, bytes, lenguaje principal, si tiene `BUILD.gn`/`DEPS`/`OWNERS`/`README.chromium`, y cuántos archivos hay de cada lenguaje |
| `componentes.json` | Cada componente: capa, tamaño, lenguajes, Rust, plantillas GN, repositorio y revisión gclient, licencia (`README.chromium`), a quién usa y quién lo usa |
| `gn-objetivos.tsv.gz` | Cada objetivo de cada `BUILD.gn`: etiqueta, plantilla (`source_set`, `component`, `rust_static_library`…), archivo |
| `gn-dependencias.tsv.gz` | Cada dependencia `deps`/`public_deps`: objetivo → dependencia |
| `dependencias-gclient.tsv` | Cada repositorio externo: ruta, URL, revisión, presente |
| `objetivos-orden.tsv.gz` | **Orden de construcción**: cada objetivo de producción Linux con su nivel (0 = no depende de nada) |
| `aislamiento-produccion.json` | Niveles, ciclos de objetivos, rango de niveles de cada componente y ciclos entre carpetas |
| `plan-componentes.tsv` | Una fila por componente: capa, nivel, rango de niveles de sus objetivos, tamaño, Rust, señales del SUPER GRAFO (Seguridad, errores, Elegancia, Rendimiento), licencia y origen |
| `aislamiento.json`, `*-todas.*` | Las mismas cuentas con pruebas incluidas o con todas las plataformas, como contraste |

Se abren con cualquier hoja de cálculo (los `.tsv.gz` se descomprimen primero) o con Python: `pandas.read_csv('carpetas.tsv.gz', sep='\t')`.

## Cómo leer el aislamiento

- **Por objetivo GN** es la medida buena. GN prohíbe ciclos entre objetivos, así que el orden es real: un objetivo de nivel N solo usa objetivos de niveles menores. Los 6 ciclos que aparecen salen de unir ramas `if` que nunca se activan juntas (X11 y Wayland en `ozone`/`gl`/`vulkan`, por ejemplo).
- **Por carpeta** casi todo forma un ciclo (417 componentes en Linux), porque las carpetas mezclan objetivos de niveles distintos. `cc` usa `components/viz/common` y `components/viz/client` usa `cc`. Por eso, para aislar una pieza, se elige por objetivos, no por carpeta.
- El análisis es léxico: lee todas las ramas de los `if` de GN y quita pruebas por nombre. Sobreestima las dependencias de una configuración concreta. Para el grafo exacto de una configuración, `gn desc`/`gn gen --ide=json` sobre un `out/` configurado.
