# Imágenes

```bash
cargo run --release -p quimera-ventana -- https://en.wikipedia.org/wiki/Cat
cargo run --release -p quimera-banco-imagen
```

`quimera-imagen` (nivel 2, capa `grafico`, `no_std`, sin dependencias): bytes → RGBA de 8 bits.

| Parte | Qué hace |
|---|---|
| `inflar.rs` | DEFLATE y zlib: bloques almacenados, Huffman fijo y dinámico; tabla rápida de 11 bits y lectura de 8 bytes de una vez |
| `png.rs` | PNG: 1 a 16 bits, gris, RGB, paleta, alfa, `tRNS`, entrelazado Adam7; el filtro se decide una vez por línea y hay caminos rápidos para 8 bits |
| `gif.rs` | GIF: el primer fotograma, LZW, transparencia y entrelazado |
| `jpeg.rs`, `jpeg_bloques.rs` | JPEG secuencial y **progresivo** (DC y AC, primera pasada y refinado, series de EOB), reinicios, 4:4:4 / 4:2:2 / 4:2:0, gris, CMYK/YCCK de Adobe; la IDCT entera de stb_image y el remuestreo «fancy» de libjpeg |

WebP y SVG se reconocen y devuelven `NoSoportado`: la página se maqueta igual (con el tamaño de los atributos) y la imagen no se dibuja. Hay un límite de 100 megapíxeles por imagen.

## Comprobado

`quimera-banco-imagen` decodifica las mismas imágenes con QUIMERA y con las bibliotecas de referencia (`png` 0.17, `jpeg-decoder` 0.3 en su modo sin SIMD, `gif` 0.13). Usa imágenes reales del sistema (fondos de Windows, JPEG secuenciales y progresivos; las PNG de la documentación de Rust) y otras generadas con codificadores de referencia (cada tipo de PNG, cada submuestreo de JPEG, progresivo, reinicios, gris, GIF con transparencia):

| | Resultado |
|---|---|
| Píxeles | **115 de 115 idénticas** (incluido JPEG, al bit) |
| Velocidad | 0,75× las referencias (JPEG 0,8×, PNG 0,5×) |

Las referencias usan varios hilos y `fdeflate`; acelerar PNG es trabajo pendiente. Las pruebas unitarias incluyen un PNG con cada byte cambiado y cortado en cada posición: error o imagen, nunca un pánico.

## En la página

1. **Estilo** — `width` y `height` de `<img>` (y de `video`, `canvas`, `iframe`, `table`, `td`...) se vuelven las declaraciones más débiles de la página (*presentational hints*): cualquier regla de CSS les gana.
2. **Maquetación** (`reemplazado.rs`), como CSS 2.1 §10.3.2 y §10.6.2:
   - con ancho y alto, esos; con uno solo, el otro guarda la proporción; sin ninguno, el tamaño propio;
   - `max-width` la reduce sin deformarla si el alto es automático (el `max-width:100%; height:auto` de casi todas las páginas);
   - si la imagen aún no llegó, la proporción sale de los atributos `width`/`height`, como en los navegadores actuales: la página no salta al llegar;
   - en línea, como bloque (`display:block`, centrada con `margin:auto`) y como elemento flexible.
3. **Red** — la página se muestra en cuanto llega; sus imágenes se descargan después, en 4 hilos, y se decodifican en el mismo hilo. Las que llegan entre dos cuadros se juntan en **una sola maquetación**. Las de una página anterior se descartan.
4. **GPU** — una textura por imagen (alfa premultiplicado, filtro lineal; las mayores que el límite de la GPU se reducen antes). Los cuadros se dibujan en su orden y solo se cambia de textura cuando cambia la imagen. Los píxeles se sueltan de la memoria en cuanto se suben.

`--captura` espera a todas las imágenes antes de dibujar.

## Pendiente

`srcset` y `<picture>`, `background-image`, WebP, SVG, animación de GIF, `object-fit`, mipmaps (una foto grande dibujada pequeña se ve con dientes), y una caché para no volver a descargar al recargar.
