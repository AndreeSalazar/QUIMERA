# HTML y CSS: la primera capa del navegador

Dos crates de nivel 2, `no_std` y sin dependencias, en `web/`:

| Crate | Qué hace | Inspiración |
|---|---|---|
| `quimera-html` | HTML → árbol del documento, con la recuperación de errores de los navegadores | html5ever (Servo) y el estándar de HTML |
| `quimera-css` | CSS → reglas (selectores + declaraciones), `@media`, especificidad, colores y longitudes | Stylo y cssparser (Servo), CSS Syntax Level 3 |

No dependen uno del otro. Los unirá `quimera-estilo` (la cascada), y después vendrán la maquetación y el pintado con wgpu.

## quimera-html

| Archivo | Qué hace |
|---|---|
| `tokenizador.rs` | Etiquetas, atributos con o sin comillas, comentarios, doctype, contenido crudo de `script`/`style`/`noscript`, y `title`/`textarea` con referencias |
| `entidades.rs` | `&amp;`, `&#233;`, `&#x1F600;`, el remapeo Windows-1252 (`&#150;` es `–`) y las referencias antiguas sin `;` |
| `constructor.rs` | Tokens → árbol (versión compacta de los modos de inserción del estándar) |
| `extranjero.rs` | SVG y MathML dentro de HTML: `viewBox`, `linearGradient`, `<path/>` |
| `arbol.rs` | El árbol: nodos en un arreglo enlazados por número, sin `Rc` |

El constructor hace lo que hacen los navegadores con el HTML real:
- crea `<html>`, `<head>` y `<body>` aunque falten, y lleva a `<head>` lo que va ahí;
- cierra solos `<p>`, `<li>`, `<dt>`/`<dd>`, `<option>`, los títulos y las celdas de tabla, y crea los `<tbody>`/`<tr>` implícitos;
- ignora los cierres que no casan con nada, y crea un `<p>` vacío con un `</p>` suelto;
- junta en `<html>`/`<body>` los atributos de una segunda etiqueta `<html>` o `<body>`;
- quita el salto de línea inicial de `<pre>` y `<textarea>`;
- con SVG/MathML, recupera las mayúsculas, respeta `<path/>` y sale del SVG ante un `<p>` o un `<div>`.

**Falta**: el «algoritmo de adopción» (reabrir `<b>`/`<i>` mal anidados), el *foster parenting* (texto suelto dentro de `<table>` que los navegadores sacan fuera), `<template>` como fragmento aparte (hoy su contenido son sus hijos), `<frameset>`, la detección de codificación (se espera UTF-8) y la tabla completa de 2.231 referencias con nombre (hay unas 140, las de uso real).

### Comparado con html5ever

`bancos/web` analiza páginas reales con QUIMERA y con html5ever 0.39 y compara los árboles línea a línea. Usa la documentación de Rust que instala rustup: más de 62.000 páginas (libro, referencia, std, rustdoc, mdBook).

```bash
cargo run --release -p quimera-banco-web -- "" 5000
```

| | Resultado |
|---|---|
| **Mismo árbol que html5ever** | **5.000 de 5.000 páginas** (45 MB) |
| QUIMERA | 650 ms, 70 MB/s |
| html5ever | 1.422 ms, 32 MB/s |
| **QUIMERA / html5ever** | **2,2× más rápido** |

Las diferencias que fue encontrando, ya arregladas: los atributos de `<html lang>` se perdían; los blancos entre `<meta>` y `<link>` van dentro de `<head>`; `<noscript>` es texto con JavaScript activo; SVG necesita sus mayúsculas y respetar `<path/>`. Para ver una página concreta: `cargo run --release -p quimera-banco-web -- pagina.html` muestra los dos árboles alrededor de la primera diferencia.

**Para ser justos**: todas son páginas generadas por rustdoc y mdBook, HTML bien escrito y de pocas formas distintas. Páginas de otros sitios (con tablas mal anidadas o `<b><i></b></i>`) encontrarán lo que falta. html5ever además es *streaming* (analiza mientras llega el texto) y guarda los nombres internados.

**Registro de cambios** (`registro.rs`): encendido (solo mientras algún `MutationObserver` observa), cada cambio de hijos, de atributo o de texto deja un `Cambio`; `tomar_cambios` los entrega y `sin_anotar` copia sin anotar (lo que se copia dentro de nodos nuevos, como en `innerHTML`, no lo puede estar observando nadie).

## quimera-css

| Archivo | Qué hace |
|---|---|
| `tokenizador.rs` | CSS Syntax Level 3: identificadores con escapes, números, `%`, dimensiones, cadenas, `url()`, comentarios |
| `hoja.rs` | Reglas, `@media` (la condición queda como texto para quien sepa el tamaño de la pantalla), `!important`, el atributo `style` |
| `selectores.rs` | Etiqueta, `*`, `#id`, `.clase`, los siete tipos de `[atributo]`, `:first-child`, `:last-child`, `:only-child`, `:root`, `:empty`, `:link`, `:not()`, `:is()`, `:where()` (sin especificidad), `:nth-child()`, `:nth-last-child()`, `:nth-of-type()`, `:nth-last-of-type()`, `:first/last/only-of-type`, `::before`/`::after`, combinadores (espacio, `>`, `+`, `~`), especificidad |
| `valores.rs` | Colores (148 nombres, `#rgb(a)`, `#rrggbb(aa)`, `rgb()`, `hsl()`, con comas o espacios y `/ alfa`) y longitudes (`px`, `em`, `rem`, `%`, `vw`, `vh`; `pt`, `in`, `cm`... pasan a píxeles) |

Recuperación de errores como en los navegadores: una declaración mala (o vacía) se salta hasta el `;`; un selector malo descarta su regla entera; `@font-face`, `@keyframes`, `@import` se saltan sin romper lo de alrededor.

El emparejamiento de selectores no conoce el árbol: pregunta a través del rasgo `ElementoCss` (nombre, atributo, padre, hermanos). Así sirve para el árbol de `quimera-html` y para el DOM futuro.

Con las hojas de estilo de la documentación de Rust: 22 hojas distintas (160 KB), 1.029 reglas y 3.183 declaraciones, sin reglas perdidas (los ~20 bloques de diferencia con un recuento a mano son los pasos de `@keyframes`).

`:not()` faltaba y era grave: YouTube define todas sus variables globales en `html:not(.style-scope)`; sin ellas, el botón de menú de cada vídeo medía 1280 px y aplastaba el título. Las comas de dentro de `:not(.a, .b)` ya no parten la lista de selectores.

**`:has()`** (`tiene.rs`): `:has(a)`, `:has(> a)`, `:has(+ a)`, `:has(~ a)` y listas. Como en Chrome, no perdona (un selector que no se entiende descarta la regla) y no se anida. Comprobado contra Chromium con [ejemplos/has.html](ejemplos/has.html). Mirar los descendientes cuesta lo que mide el subárbol.

**Falta**: `:nth-child(2n of S)` (no casa), `:hover` y demás estados (no casan: el estilo es estático por ahora), `@supports`, `@import` de verdad y CSS anidado.

## Siguiente

1. **`quimera-estilo`**: la cascada. Para cada elemento, sus reglas ordenadas por origen, `!important`, especificidad y orden, más la herencia y los valores por defecto del navegador (la hoja `html.css` de cada navegador).
2. **`quimera-maquetacion`**: cajas de bloque y en línea, y el texto partido en líneas.
3. **`quimera-gpu` + `quimera-pintar`**: rectángulos y texto en una ventana con wgpu. La primera página dibujada por QUIMERA.
