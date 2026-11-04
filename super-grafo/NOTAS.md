# Notas de SUPER GRAFO

Qué es cada parte de QUIMERA y cómo se comporta, con medidas reales (4 de noviembre de 2026, Windows, Ryzen 5 5600X). Hay una sección por nodo, con su ruta como título. SUPER GRAFO enseña la nota al pasar el ratón por el nodo (grafo y Ciudad), y `MAPA.md` usa su primera frase.

Las medidas en vivo salen de `QUIMERA_VIVO` + `QUIMERA_TIEMPOS`, con unos 35 s de YouTube (portada) y de Wikipedia («Gato»). Se escriben como «total en la sesión · mediana por tramo · peor».

## bancos

Bancos de comparación: cada uno mide una parte de QUIMERA contra la referencia en Rust, con la misma entrada. No forman parte del navegador.

- Son el único sitio donde entran crates de fuera (Boa, html5ever, png/jpeg/gif), como juez: mismo resultado y velocidad.
- Se corren a mano (`cargo run --release -p quimera-banco-…`), no en cada commit.

## bancos/frontend

Banco del frontend de JavaScript contra el parser de Boa 0.22, con más de 1 MB de benchmarks (Richards, DeltaBlue…).

- Mide texto → árbol (los dos) y texto → bytecode (todo el frontend de QUIMERA), el mejor de cinco intentos.
- Junta 7 hallazgos revisados de los guardianes (unwrap en código de medida, aceptable aquí).

## bancos/gc

Banco de `quimera-gc` contra `boa_gc`, con una carga que imita a Splay (una tabla de larga vida y basura que muere joven).

## bancos/imagen

Banco de los decodificadores de imágenes contra `png`, `jpeg-decoder` y `gif`.

- Píxeles exactos en PNG y GIF; en JPEG, dice la diferencia máxima.
- Usa los fondos de Windows y casos generados (16 bits, progresivo, entrelazado).

## bancos/vm

Banco de la máquina virtual contra `boa_engine` 0.22, con los benchmarks de Boa: los dos tienen que devolver lo mismo.

- También corre scripts sueltos: `-- a.js b.js`.
- Es el nodo con más hallazgos de los guardianes (24), todos revisados: es código de medida.

## bancos/web

Banco de `quimera-html` contra html5ever (Servo) en páginas reales, por defecto la documentación de Rust (miles de páginas).

- Compara los árboles volcados y enseña la primera diferencia.

## cimientos

Los cimientos: lo que todo lo demás usa y que no usa nada de QUIMERA (capa 0).

## cimientos/comprimir

Descompresión sin dependencias: DEFLATE, zlib y gzip.

- La usan las imágenes PNG, las fuentes WOFF y las respuestas comprimidas de la red.
- Es `no_std`, sin crates de fuera.

## cimientos/valor

El valor de JavaScript en 64 bits (NaN-boxing, como `JS::Value` de SpiderMonkey): el ladrillo del motor de JavaScript.

- Copiar un valor es copiar un `u64`: no hay contadores de referencias. A Boa eso le costaba un 8–13 % del tiempo.
- El GC encuentra lo vivo por sus raíces.

## grafico

Gráficos: GPU, pintado, texto, imágenes, vectores y vídeo. Aquí la página acaba en cuadros de pantalla.

## grafico/gpu

La GPU de QUIMERA (wgpu): una sola tubería que dibuja cuadros (rectángulos de color y letras de un atlas) en una sola pasada.

- **Cómo se comporta:** los cuadros se suben una vez por maquetación. Desplazar solo cambia un uniforme. Hay un modo fijo para la capa `position: fixed` y una textura por imagen y por vídeo.
- **En vivo:** se usa a través del compositor (su propio hilo). Dibujar un cuadro cuesta 0,6 ms de mediana y 27 ms el peor (YouTube, 348 cuadros en 35 s).
- **Cuidado:** las capas no se guardan por separado. Cambiar una pieza vuelve a subir la escena entera (paso 2 del plan).

## grafico/imagen

Decodificadores propios de imágenes, `no_std`: PNG (todas las profundidades, Adam7), JPEG (secuencial y progresivo, la misma IDCT que stb_image) y GIF.

- **Cómo se comporta:** mira el formato por los bytes, no por la extensión. Decodifica en hilos aparte y solo lo que está en pantalla o cerca. Lo que queda a más de tres pantallas suelta su textura.
- **En vivo:** 19 imágenes en YouTube en 21 ms en total (0,1 ms de mediana, 5 ms la mayor).

## grafico/pintar

El pintado: la lista de dibujo de la maquetación → cuadros para la GPU. Cada letra se rasteriza una vez por tamaño en un atlas.

- **Cómo se comporta:** `Medidor` le da a la maquetación las mismas medidas con que se dibuja, así el texto cabe donde dijo la maquetación. Marca la capa fija.
- **En vivo:** es barato. 0,3 ms por escena en YouTube; 10 ms en Wikipedia, porque hay mucho texto y muchas letras.
- **Cuidado:** si el atlas se llena, se vacía y se vuelve a llenar.

## grafico/texto

Fuentes: busca las del sistema (Windows y Linux), mide texto y rasteriza letras con `ab_glyph`.

- Las fuentes se cargan cuando hacen falta, y hay fuentes de reserva para los caracteres que faltan (emoji, flechas).
- Las fuentes de la página (`@font-face`) se descargan después de la primera pintada, como `font-display: swap`.

## grafico/vector

SVG → imagen RGBA, propio y `no_std`: trazos, formas, rellenos, degradados, `use`.

- Es lo que usan los iconos y logotipos de YouTube.

## grafico/video

Vídeo: MP4 propio (también el fragmentado, el que usa MediaSource), H.264 a Annex B, color NV12 → RGBA y la decodificación de H.264 con Media Foundation de Windows.

- **Cómo se comporta:** los vídeos viven en el compositor. La página le pasa las muestras nuevas cada 100 ms como mucho; el compositor lleva el tiempo, decodifica hasta el momento actual y sube el fotograma.
- **Medido:** el vídeo de YouTube va a 30 fps aunque un script tarde segundos.
- **Falta:** el audio (AAC/WASAPI). En Linux y Mac aún no hay decodificador.

## herramientas

Herramientas para trabajar en QUIMERA. No forman parte del navegador.

## herramientas/guardianes

El semáforo: archivos de menos de 1000 líneas (aviso a las 700), la jerarquía de niveles, `unsafe` contado, funciones de menos de 80 líneas y crates de menos de 5000.

- **Cómo se comporta:** el hook de commit lo corre en modo estricto. Un amarillo sin revisar bloquea igual que un rojo.
- **Hoy:** 0 rojos y 40 amarillos, todos revisados.
- Con `--sarif guardianes.sarif`, SUPER GRAFO los pinta en la Ciudad.

## herramientas/ver

Ver una página sin ventana: HTML → estilo → maquetación → un SVG con la lista de dibujo. Fue la primera manera de «ver» QUIMERA.

## motor-js

El motor de JavaScript propio de QUIMERA: léxico, parser, bytecode, GC, objetos, regex y máquina virtual.

## motor-js/frontend

JavaScript → bytecode de registros (Stencil, como SpiderMonkey): el resultado no tiene objetos del GC, así que se puede guardar o compilar en otro hilo.

## motor-js/gc

Recolector de basura generacional (guardería más zona principal, como SpiderMonkey).

- Crear un objeto es copiarlo al final de un bloque, sin `malloc`. Solo se copia fuera lo que sobrevive.
- El GC de marcar y barrer de Boa se llevaba el 27 % del tiempo en Splay.
- Tiene 3 hallazgos revisados (el único `unsafe` permitido en el motor).

## motor-js/objetos

Los objetos sobre el GC: una cabecera fija y una celda de propiedades que crece sin mover el objeto (los *slots* de SpiderMonkey). Hay formas para las cachés en línea.

## motor-js/parser

Léxico y parser de JavaScript → árbol de sintaxis.

- La sintaxis moderna (clases, desestructuración, spread, `?.`, async) se convierte en lo que entiende el compilador, como hace Babel.
- Es de los más grandes (3347 líneas).

## motor-js/regex

Expresiones regulares de JavaScript, `no_std`, como irregexp: patrón → programa → una máquina con vuelta atrás y pila propia (un texto largo no la desborda).

- Tiene atajos para texto plano, prefijos fijos y `^`.

## motor-js/vm

La máquina virtual: ejecuta el bytecode de registros, con cachés en línea, sin recursión de Rust y con los registros en la pila de raíces del GC.

- **Cómo se comporta:** corre los scripts y temporizadores de la página. Incluye la biblioteca moderna (JSON, arreglos tipados, Promise…) y su propio perfilador (`QUIMERA_PERFIL`).
- **En vivo:** es lo que más pesa en YouTube. 12,6 s de 35 s, con un pico de 3 s al ejecutar los 9 scripts de la página. En Wikipedia, solo 0,3 s.
- **Cuidado:** es el crate más grande (8373 líneas; el guardián pide partirlo). La biblioteca debería irse a su propio crate (`quimera-builtins`). Los picos de 3 s son el siguiente objetivo de velocidad.

## navegador

El navegador que ve la gente: ventana, barra, historial, eventos e hilos.

## navegador/ventana

La ventana de QUIMERA: tres hilos (la ventana con la rueda, la página con JS, estilo y maquetación, y el compositor con la GPU), la barra, los enlaces, el historial, los formularios y el modo captura.

- **Cómo se comporta:**
  - la rueda desplaza desde el hilo de la ventana sin esperar al JavaScript (0,5 ms de mediana);
  - la página se pinta antes de ejecutar sus scripts;
  - cada cambio del DOM provoca una pasada, pero incremental: estilo y maquetación solo rehacen lo tocado (unos 14 ms en YouTube; antes 90).
- **En vivo:** orquesta. Las peticiones de red, la decodificación y el pintado salen a otros hilos. `QUIMERA_VIVO` escribe estas trazas.
- **Cuidado:**
  - lo usa todo (es la cima de las capas);
  - la pasada de maquetación ya va en su módulo (`navegador/pasada.rs`), y `navegador.rs` bajó de 669 a 624 líneas; sigue siendo de los más tocados;
  - el modo captura sigue en un solo hilo.

## red

La red.

## red/red

La red propia: URLs, HTTP/1.1, HTTPS con rustls, `file:` y `data:`, cookies, bloqueo de anuncios como Brave y una piscina de conexiones reutilizadas (6 por sitio, keep-alive).

- **Cómo se comporta:** las páginas y los recursos se descargan de 12 en 12 en hilos aparte. El HTML llega primero y las hojas y scripts vienen en paralelo.
- **En vivo:** una página de YouTube tarda 0,7–1,7 s de red (el total de la sesión, 24,7 s, suma descargas en paralelo). En Wikipedia, 0,4 s de mediana por recurso.
- **Cuidado:** aún no hay HTTP/2 ni HTML en streaming: espera al documento entero antes de analizarlo (paso 4 del plan).

## web

La web: HTML, CSS, estilo, DOM y maquetación.

## web/css

Analizador de CSS, `no_std`: hojas, `@media`, selectores con especificidad, colores, longitudes, `calc()` y variables.

## web/dom

El DOM visto desde JavaScript: un solo árbol y un solo GC.

- Los nodos son objetos del anfitrión en el montón de la VM.
- Lleva los temporizadores, los eventos, `fetch`, MediaSource y el medidor (las medidas de maquetación para los scripts).
- **En vivo:** el medidor costaba 92 s en YouTube; ahora cuesta 12 s en total y 117 ms al ejecutar los scripts.
- **Cuidado:** pasa de 5000 líneas (5771); el guardián pide partirlo.

## web/estilo

La cascada de estilos: para cada elemento, el valor de cada propiedad (origen, `!important`, especificidad, `@media`, herencia, variables, pseudoelementos).

- **Cómo se comporta:** **incremental** (`incremental.rs`). Tras un cambio de JavaScript recalcula solo lo que alcanzan los toques del documento: el elemento, sus hermanos y lo de dentro, más los `:has()` que solo se propagan si su estilo cambia. Los atributos que ninguna regla mira no recalculan nada. El índice de reglas se guarda entre cálculos, y el filtro Bloom de antepasados descarta reglas.
- **En vivo:** **4,3 ms por cambio en YouTube** (recalcula unos 12 de 2200 elementos; antes eran 47–53 ms); 5,5 ms en Wikipedia. Cuando cambian las hojas (un `<style>` nuevo), una pasada entera: unos 45 ms.
- **Comprobar:** `QUIMERA_ESTILO_COMPROBAR=1` compara cada cálculo con el entero (0 distintos en YouTube y Wikipedia).
- **Cuidado:** si una regla nueva mira algo que las raíces no cubren, el modo comprobar lo dirá. Lo siguiente es el CSS en paralelo (paso 8, como Stylo).

## web/html

Analizador de HTML5, `no_std`: cualquier texto da un árbol, y se recupera de los errores como un navegador.

- **En vivo:** analizar YouTube cuesta 3 ms y Wikipedia 5 ms. No es el cuello de botella; lo es la red.

## web/maquetacion

La maquetación: documento con sus estilos → lista de dibujo. Hace bloques, en línea, flex, grid, tablas, flotantes, posicionamiento (también la capa fija), texto e imágenes.

- **Cómo se comporta:** **incremental** (`incremental.rs`). Las medidas en seco (`alto_libre`: el 93 % de los bloques de cada pasada en YouTube) y los tamaños intrínsecos pasan de una maquetación a la siguiente. Solo se tiran los de los nodos sucios y sus antepasados (estilo cambiado, toques de JavaScript, imágenes que llegan, vídeos). Se tira todo con fuentes nuevas, otro tamaño de ventana, teclado o clic.
- **En vivo:** **2,8 ms por cambio en YouTube** (antes 31–38 ms); 13 ms en Wikipedia, donde casi todo es texto de verdad.
- **Comprobar:** `QUIMERA_MAQ_COMPROBAR=1` maqueta también entera y compara: 123 pasadas iguales en YouTube. Sacó un fallo de la entera: las anclas de dentro de un `inline-block` iban en (0, 0); ya está arreglado.
- **Cuidado:** la pasada entera tras un cambio ya va por unos 14 ms. Lo que más pesa ahora es copiar el árbol (2,7 ms) y los pseudoelementos (4,2 ms).

## datos-investigación

La investigación de antes y de durante QUIMERA:
- Chromium diseccionado (jerarquía, SARIF del árbol entero);
- Servo, Boa y SpiderMonkey medidos;
- evidencias y scripts.

Lo más pesado (el disco de la VM, las copias, los SARIF grandes) lo ignora git.

## docs

La documentación de QUIMERA, un documento por parte:
- VENTANA, VIDEO, RED, VM y ESTILO-MAQUETACION;
- los guardianes;
- la jerarquía;
- PLAN, con la hoja de ruta de SUPER QUIMERA.

## super-grafo

El save de SUPER GRAFO: `orden.json` (el grafo organizado por capas y las carpetas desplegadas), `MAPA.md` y `mapa.json` (QUIMERA en una página) y estas notas. Es lo primero que hay que leer para retomar el trabajo.
