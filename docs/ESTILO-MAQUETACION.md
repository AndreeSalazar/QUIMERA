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

**Falta**: `float`, `position` (absoluta, fija, relativa), `grid`, tablas de verdad (hoy cada celda es un bloque), `inline-block` con su caja (hoy se maqueta como `inline`), fondos y bordes de los elementos en línea que ocupan varias líneas, `overflow`, `text-align: justify`, partir palabras largas, texto bidireccional y las imágenes de verdad (hoy un recuadro gris).

## Flexbox

```bash
cargo run --release -p quimera-ventana -- docs/ejemplos/flex.html
```

[`docs/ejemplos/flex.html`](ejemplos/flex.html) es una página típica de hoy: menú con el logo a la izquierda y los enlaces empujados a la derecha (`margin-right: auto`), portada en columna centrada, botones con `gap`, tres tarjetas que se reparten el ancho 1 : 2 : 1 con el mismo alto (`stretch`), y pie con `space-between`. QUIMERA la dibuja **igual que Chromium**: misma disposición y mismas proporciones (las tarjetas miden unos 121 px de alto en Chromium y 120 en QUIMERA). Solo cambia dónde se parte alguna línea, porque las dos miden la fuente con métricas algo distintas.

**Estilo** (`quimera-estilo/src/flex.rs`): `display: flex`, `flex-direction`, `flex-wrap`, `justify-content`, `align-items`, `align-self`, `flex-grow`, `flex-shrink`, `flex-basis`, `row-gap`/`column-gap`, `order`, y los atajos `flex` (`none`, `auto`, `initial`, `1`, `1 2`, `1 2 30%`, `100px`), `flex-flow` y `gap`.

**Maquetación** (`quimera-maquetacion/src/flex.rs`), el algoritmo de la especificación (§9), compacto:
1. Cada hijo es un elemento flexible (los textos sueltos, uno anónimo), ordenado por `order`. Su tamaño base sale de `flex-basis`, o de su `width`/`height`, o de su contenido (`max-content` en fila; en columna, su alto maquetado).
2. Las líneas: una, o con `wrap` tantas como hagan falta.
3. El espacio libre se reparte con `flex-grow` (si los factores suman menos de 1, solo esa parte). Lo que sobra se quita con `flex-shrink`, en proporción a tamaño × factor, sin bajar del **mínimo del contenido** (la palabra más larga); los que llegan al mínimo se congelan y se reparte otra vez.
4. Los márgenes `auto` se quedan el espacio sobrante; si no hay, `justify-content` (`space-*` sin espacio caen a inicio o centro, como manda la especificación). Las direcciones `-reverse` se colocan al revés.
5. El alto de cada línea es el de su elemento más alto (o el del contenedor, si es una sola línea y el contenedor tiene alto). `align-items`/`align-self` colocan, y `stretch` estira lo que no tiene tamaño cruzado.

Los elementos se maquetan con el mismo `bloque()` de siempre: primero «en seco» para medirlos (sus piezas se descartan) y después en su sitio, con el ancho que les toca y, si se estiran, con su alto impuesto.

**Tamaños intrínsecos** (`intrinseco.rs`): el ancho mínimo (`min-content`: la palabra o imagen más ancha) y máximo (`max-content`: todo en una línea) de cualquier elemento, guardados por nodo. Los usan flexbox y las tablas, y servirán para `float` e `inline-block`.

Por el camino: el texto de un bloque ya no pinta un fondo propio debajo de cada línea (en los navegadores solo lo pintan los elementos en línea, como `<span>`). Y las fuentes de reserva incluyen chino, japonés y coreano (Microsoft YaHei, Yu Gothic, Malgun; Noto CJK en Linux), así que `日本語` o `简体中文` ya no salen como cajas.

**Falta en flex**: `align-content`, la alineación por línea base, `min-width`/`max-width` como límites del reparto, `inline-flex` como caja en línea y el mínimo automático en columnas.

## Tablas

```bash
cargo run --release -p quimera-ventana -- https://news.ycombinator.com/
```

Hacker News (hecho entero con tablas) se ve **igual que en Chromium**: la tabla al 85 % centrada por `<center>`, la barra naranja, las filas numeradas y los títulos a la izquierda. En Wikipedia, la ficha de cada artículo arma su rejilla de fotos.

**Comprobado contra Chromium** con [`docs/ejemplos/tablas.html`](ejemplos/tablas.html): diez casos con cajas de tamaño fijo (sin texto, así las fuentes no influyen). Las **39 cajas** quedan donde las pone Chromium, al 1/64 de píxel con que redondea: espaciado, bordes que colapsan, ancho fijo, `colspan`, `rowspan`, `vertical-align`, porcentajes, atributos antiguos, `width: 100%` y `<thead>`/`<tfoot>` desordenados (`web/maquetacion/tests/tablas.rs`).

**Estilo** (`quimera-estilo/src/tabla.rs`):
- `display: table`, `table-row-group`, `table-header-group`, `table-footer-group`, `table-row`, `table-cell`, `table-caption`, `table-column`.
- `border-collapse` y `border-spacing` (se heredan) y `vertical-align`. La hoja del navegador hace que las celdas lo tomen de su fila (`inherit`), así que por defecto van centradas en vertical, como en los navegadores.
- **Atributos antiguos**, como pistas de presentación: `border`, `cellspacing`, `cellpadding` (que cada celda toma de su tabla), `bgcolor`, `width`, `height`, `align`, `valign` y `nowrap`.
- **`<center>` y `align=center`** centran también los bloques de dentro (`-webkit-center` en Chromium), no solo el texto.
- **Modo de compatibilidad:** sin `<!doctype html>` se suma la hoja `compatibilidad.css`, como la `quirks.css` de Chromium. En ese modo las tablas no heredan la letra ni la alineación de fuera, así que en las páginas antiguas `<center><table>` centra la tabla pero no su texto.

**Maquetación** (`quimera-maquetacion/src/tabla.rs`), el algoritmo automático (CSS 2.1 §17):
1. **Rejilla:** primero las filas de `<thead>`, luego las de `<tbody>` (y las sueltas) y al final las de `<tfoot>`. Cada celda ocupa la primera casilla libre con su `colspan × rowspan` (`rowspan=0` llega al final).
2. **Columnas:** el mínimo y el máximo de cada una salen de sus celdas (los tamaños intrínsecos de siempre). Una celda con `width` fija su columna, y una con `%` la reparte. Las de varias columnas reparten lo que les falte en proporción.
3. **Ancho:** el pedido (en una tabla, `width` incluye borde y relleno), o el del contenedor sin pasar del máximo de las columnas, y nunca menos que el mínimo. Primero los porcentajes; lo que hay entre el mínimo y el máximo se reparte en proporción, y lo que sobra va a las columnas automáticas según su máximo.
4. **Filas:** cada una tan alta como su celda más alta; una celda de varias filas alarga la última, y `height` en la tabla se reparte entre las filas.
5. Cada celda se maqueta con el mismo `bloque()` de siempre: primero en seco para medirla, después en su sitio con el alto de sus filas. Su contenido se mueve arriba, en medio o abajo. Debajo van el fondo de su grupo y el de su fila.
6. **Bordes que colapsan**, como Chromium: cada borde entre dos celdas es el más ancho de los dos (y en los de fuera, también el de la tabla). Cada celda ocupa la mitad de sus bordes, la tabla la mitad de los de fuera, y los bordes se dibujan centrados en las líneas, encima de todas las celdas.

**Falta en tablas:** `table-layout: fixed`, el ancho de `<col>` y `<colgroup>`, `caption-side: bottom`, la alineación de celdas por línea base (cuenta como arriba), `visibility: collapse` y meter en una tabla anónima las celdas sueltas fuera de una tabla (`display: table-cell` en un `<div>` se maqueta como bloque).

## Siguiente

1. **Velocidad del estilo** con páginas grandes (filtro de Bloom de antepasados, estilos compartidos).
2. **Fuentes de verdad** (`quimera-texto`).
3. **Ventana con wgpu** (`quimera-gpu` + `quimera-pintar`): la misma lista de dibujo, en pantalla.
4. Imágenes, `float` y `position`, lo que más falta ahora en páginas reales.
