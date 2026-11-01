# Estilo y maquetación: del árbol a algo que dibujar

```
HTML ──quimera-html──▶ árbol ──quimera-estilo──▶ estilo de cada elemento ──quimera-maquetacion──▶ lista de dibujo
                                     ▲                                                                │
                              quimera-css (hojas)                                         quimera-ver (SVG) · después wgpu
```

| Crate | Nivel | Qué hace | Inspiración |
|---|---:|---|---|
| `quimera-estilo` | 8 | La cascada: qué valor tiene cada propiedad en cada elemento | Stylo (Servo) |
| `quimera-maquetacion` | 9 | Cajas y líneas → lista de dibujo (rectángulos, textos, imágenes) | Servo `layout`, CSS 2.1 |
| `quimera-ver` | 100 | Herramienta: una página → imagen SVG, para verla antes de tener ventana | — |

La maquetación usa el estilo, así que no pueden compartir nivel (el plan los tenía a los dos en el 8): maquetación pasa al 9 y lo de encima sube uno.

## La primera página

```bash
cargo run --release -p quimera-ver -- docs/ejemplos/primera.html docs/ejemplos/primera.svg
```

[`docs/ejemplos/primera.svg`](ejemplos/primera.svg) es la primera página dibujada por QUIMERA: todo el camino, del HTML al dibujo, es propio. Tarda menos de 1 ms (HTML 0,08 · estilo 0,46 · maquetación 0,09). Con páginas reales de la documentación de Rust, con su CSS de verdad cargado desde `<link>`:

| Página | Nodos | HTML | Estilo | Maquetación |
|---|---:|---:|---:|---:|
| Libro: «What Is Ownership?» | 1.289 | 0,6 ms | 3,9 ms | 1,2 ms |
| `std::vec::Vec` | 38.419 | 13 ms | 120 ms | 19 ms |

El estilo de `Vec` es lo más lento: 38.000 elementos contra las ~1.000 reglas de rustdoc. Stylo lo resuelve con filtros de Bloom de antepasados y compartiendo estilos entre hermanos iguales; aquí es el siguiente paso de velocidad.

## quimera-estilo

1. **Hojas**: la del navegador (`navegador.css`, basada en la del estándar de HTML), los `<style>` y los `<link rel="stylesheet">` en orden de documento (el texto lo da quien llama: el estilo no lee archivos ni red), y el atributo `style`.
2. **Índice**: cada selector se guarda por la parte derecha de su selector (id, primera clase o etiqueta), como en Stylo: cada elemento solo se prueba contra lo que puede casar.
3. **Orden**: navegador normal < página normal < página `!important` < navegador `!important`; dentro, especificidad y orden. Las declaraciones no válidas no entran (gana la anterior, como en CSS).
4. **Atajos**: `margin`, `padding`, `border` (y sus lados, anchos, estilos y colores), `background` (el color), `font` (con `tamaño/alto-de-línea` y familia), `list-style`.
5. **Valores calculados**: primero `font-size` (las demás longitudes en `em` dependen de ella) y `color` (`currentcolor`); `em`, `rem`, `vw`, `vh` y las unidades absolutas pasan a píxeles; los porcentajes quedan para la maquetación. Herencia de lo heredable, e `inherit`, `initial`, `unset`.
6. **`@media`**: `screen`/`print`, `not`, `only`, comas, `and`, `min/max-width/height` (en `px` o `em`), `orientation`, `prefers-color-scheme: light`.

Propiedades que calcula: `display`, `color`, `background-color`, `font-size`, `font-weight`, `font-style`, `font-family` (con su familia genérica), `text-align`, `line-height`, `white-space`, `text-decoration`, `text-transform`, márgenes, rellenos, bordes, `width`, `height`, `max-width`, `min-height`, `list-style-type`, `visibility`.

**Falta**: `var()` y `calc()`, `position`, `float`, `flex`, `grid` y `overflow`, `:hover` y demás estados, `::before`/`::after` con `content`, colores del sistema (`ThreeDFace`) y compartir estilos entre elementos iguales (velocidad).

## quimera-maquetacion

El flujo normal de CSS 2.1:
- **Bloques**: ocupan el ancho del contenedor, o el suyo con `width`, limitado por `max-width`, y `margin: auto` centra. Bordes y rellenos. Los márgenes verticales **colapsan**: entre hermanos cuenta el mayor; el del primer hijo sale por arriba del padre si este no tiene borde ni relleno (y el del último por abajo); los de la raíz no colapsan.
- **En línea**: los hijos en línea seguidos forman un bloque anónimo. El texto se convierte en palabras con las reglas de `white-space` (`normal` junta blancos, `pre` respeta todo, `pre-wrap`, `pre-line`, `nowrap`) y `text-transform`. Se parte en líneas de forma voraz; cada línea mide lo de su elemento más alto (el propio bloque cuenta como *strut*), con una línea base común y `text-align`.
- **Listas**: el marcador (`•`, `◦`, `▪`, `1.`, `a.`, `i.`) fuera del contenido, en la primera línea; respeta `<ol start>`.
- `<br>`, `<img>` con tamaño (por atributos o CSS), fondos y bordes debajo del contenido, subrayado y tachado, fondo del lienzo (el de `<html>` o, si no tiene, el de `<body>`).

El texto se mide a través del rasgo `MedirTexto`. Por ahora `Aproximado` da anchos por clase de carácter, parecidos a los de Arial, Times y Courier. Las fuentes de verdad (`quimera-texto`) entrarán ahí sin tocar la maquetación. `quimera-ver` pone a cada texto el ancho que midió QUIMERA (`textLength`), así la imagen respeta la maquetación aunque el visor use sus fuentes.

Las pruebas usan un medidor exacto (cada carácter mide medio `em`) y comprueban posiciones calculadas a mano con las reglas de CSS.

**Falta**: `float`, `position` (absoluta, fija, relativa), `flex`, `grid`, tablas de verdad (hoy cada celda es un bloque), `inline-block` con su caja (hoy se maqueta como `inline`), fondos y bordes de los elementos en línea que ocupan varias líneas, `overflow`, `text-align: justify`, partir palabras largas, texto bidireccional y las imágenes de verdad (hoy un recuadro gris).

## Siguiente

1. **Velocidad del estilo** con páginas grandes (filtro de Bloom de antepasados, estilos compartidos).
2. **Fuentes de verdad** (`quimera-texto`).
3. **Ventana con wgpu** (`quimera-gpu` + `quimera-pintar`): la misma lista de dibujo, en pantalla.
4. `float` y `flex`, lo que más falta en páginas reales.
