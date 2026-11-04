# Vídeo

Objetivo: ver YouTube en QUIMERA.

El YouTube actual no trae enlaces de vídeo en la página. Su reproductor (JavaScript) pide los trozos con su propio protocolo (SABR, `serverAbrStreamingUrl`) y los va metiendo en un `<video>` a través de **MediaSource**. Para ver un vídeo hacen falta tres cosas:

1. **Decodificar** H.264 (y AAC para el sonido): `quimera-video`.
2. **MediaSource y `<video>`** en el DOM: el reproductor añade segmentos MP4 fragmentados con `appendBuffer`, y el elemento lleva el reloj (`currentTime`, `play`, `timeupdate`...).
3. **Pintar** cada fotograma en la caja del `<video>`.

## Decisión: el códec, del sistema

QUIMERA lo hace casi todo por sí mismo, pero un decodificador de H.264 propio son meses de trabajo y, sin la GPU, mucho más gasto de CPU. Por eso solo el códec se delega en **Media Foundation** de Windows (`CLSID_MSH264DecoderMFT`, que usa la GPU cuando puede). Todo lo que lo rodea es propio. Más adelante se puede cambiar por uno nuestro sin tocar lo demás.

## quimera-video (nivel 2, `grafico/video`)

| Archivo | Qué hace |
|---|---|
| `mp4.rs` | ISO BMFF propio: pistas (`avc1`/`avcC`, `mp4a`/`esds`) y muestras, de un archivo entero (`stbl`: `stsz`, `stco`/`co64`, `stsc`, `stts`, `ctts`, `stss`) o fragmentado (`moof`: `tfhd`, `tfdt`, `trun` con todas sus banderas, y los valores por defecto de `trex`). `Demuxer::empujar` lee a trozos, como llega por MediaSource |
| `h264.rs` | De `avcC` y NAL con su longitud delante a Annex B, con SPS y PPS antes de cada fotograma clave |
| `color.rs` | NV12 → RGBA con BT.709 en rango limitado, recortado a la zona visible |
| `mf.rs` | El decodificador de Windows: tipos de entrada y salida (NV12), muestras en unidades de 100 ns, cambios de formato a mitad de vídeo y vaciado al final. Es el único sitio con `unsafe` (`permite_unsafe = true`), con un `// SAFETY:` en cada bloque |

Medida: el vídeo de prueba de MDN (`flower.mp4`, 960 × 540, H.264) da 150 de 150 fotogramas en 438 ms, color incluido (unos 3 ms por fotograma). La prueba `tests/archivo.rs` lo decodifica si `QUIMERA_VIDEO_PRUEBA` apunta a un MP4; los vídeos no se guardan en el repositorio.

## Lo que ya se probó con YouTube

Con un MediaSource provisional que solo contaba bytes, y respondiendo que sí a H.264 y AAC:

- El reproductor se pone en marcha (sale el botón de pausa), crea un `SourceBuffer` de `audio/mp4; codecs="mp4a.40.2"` y otro de `video/mp4; codecs="avc1.4D401F"`, y pide el vídeo por SABR a servidores reales de googlevideo. Llegan 3,4 MB de vídeo de verdad.
- Para detectar imitaciones, el reproductor pregunta `isTypeSupported` con valores absurdos (`width=99999`, `channels=99`, `eotf=catavision`): el MediaSource de verdad tendrá que decir que no a esos, como Chrome.
- **El freno está en el motor de JavaScript.** Al procesar esos megas, el reproductor trocea el búfer con `subarray` y `DataView`, y en QUIMERA un `subarray` es una copia hecha elemento a elemento en JavaScript (`binarios.js`). El coste se vuelve cuadrático y el reproductor no avanza.

## Falta, en orden

1. ✅ **`ArrayBuffer` y arreglos tipados nativos en la VM** (ver [VM.md](VM.md)). Con ellos, el mismo experimento llega mucho más lejos: el reproductor mete en sus `SourceBuffer` el segmento de inicio (741 bytes) y trozos de vídeo de unos 350 KB y de audio de unos 160 KB. Son 1,8 MB de vídeo y 1,1 MB de audio en una prueba de 6 s de página.

2. ✅ **`MediaSource`, `SourceBuffer` y `<video>`** en el DOM (`web/dom/src/medios.rs`):
   - **Qué tiempos hay.** Lo que se mete con `appendBuffer` va al `mp4::Demuxer` de `quimera-video`, que dice qué tiempos tienen datos (`buffered`). Las muestras de vídeo esperan a que el anfitrión las tome con `Dom::tomar_videos`.
   - **El reloj** de cada `<video>` vive en Rust, sobre la hora de la página: corre al reproducir y se para al final de los datos. Va igual en la ventana y en una captura.
   - **La API y sus eventos**, en JavaScript: `sourceopen`, `updateend`, `loadedmetadata`, `loadeddata`, `canplay`, `play`, `playing`, `timeupdate`, `waiting`, `pause`, `seeking`, `seeked` y `ended`. También `URL.createObjectURL`.
   - **Qué se acepta.** `isTypeSupported` dice que sí solo a H.264 y AAC en MP4 con `codecs` (Chrome tampoco acepta `video/mp4` sin ellos), así que YouTube elige esos.
3. ✅ **Pintar los fotogramas.** `<video>` es un elemento reemplazado en la maquetación (300 × 150 si no tiene imagen, como en los navegadores) y se pinta con la imagen `quimera-video:N`. La ventana (`navegador/ventana/src/video.rs`) decodifica sus muestras con Media Foundation. Para no llenar la RAM, solo decodifica hasta el tiempo actual: las demás esperan comprimidas.

   **Primer resultado:** en una captura de la página de vídeo con 14 s de página, el `<video>` de YouTube va por el segundo 14 sin pausa, con `readyState` 4 y datos hasta el segundo 64. YouTube eligió 480p (854 × 480; en otra prueba, 720p) y QUIMERA pinta el fotograma real de ese momento.
4. ✅ **Que se mueva en la ventana: a 30 fotogramas por segundo.** El vídeo lo pinta el **compositor**, un hilo aparte dueño de la GPU (ver [VENTANA.md](VENTANA.md#el-compositor)):
   - **Desde la página**: le llegan las muestras comprimidas y el reloj de cada `<video>` (tiempo, si corre, velocidad).
   - **En el compositor**: lleva el tiempo, decodifica y sube cada fotograma a su textura, sin esperar al JavaScript ni a la maquetación. Los fotogramas nunca cruzan de hilo.
   - **Medido con `QUIMERA_VIDEO=1`**: 30 fotogramas por segundo estables y a tiempo real (un segundo de vídeo por segundo de reloj), con bajones puntuales cuando la GPU coincide con una escena nueva grande.

   Antes de esto (todo en un hilo), el vídeo iba a 1–4 fotogramas por segundo:
   - **Cómo late**: mientras un vídeo avanza, la ventana late a 30 por segundo, decodifica lo que toca y sube el fotograma a su misma textura (solo cambian los píxeles).
   - **Cómo late**: mientras un vídeo avanza, la ventana late a 30 por segundo, decodifica lo que toca y sube el fotograma a su misma textura (solo cambian los píxeles).
   - **Cuánto da**: 1 a 3 fotogramas por segundo, con la cola de muestras creciendo.
   - **Por qué**: con `QUIMERA_VIDEO=1` se ve cada segundo lo pintado y en qué se va cada latido (JavaScript, medidor, maquetación, vídeo). El vídeo cuesta unos 10 ms por latido; el JavaScript de la página, entre 0,1 y 1,5 s (a veces más). Todo va en un solo hilo, y mientras ese JavaScript trabaja no se pinta.
   - **Primer freno, ya quitado: el medidor.** El reproductor cambiaba el árbol y leía medidas decenas de veces por tanda, y cada lectura maquetaba la página entera (unos 55 ms). Ahora, mientras corren los temporizadores, se reutilizan las medidas de la tanda: cada tanda empieza con medidas frescas, y un elemento sin caja siempre se vuelve a medir. Pasó de 37 a 67 maquetaciones por latido a 2 a 4.
   - **Segundo freno**: la maquetación en seco exponencial y el estilo (ver ESTILO-MAQUETACION.md, «Velocidad con una página de verdad»). Con eso, unos 4 por segundo; el resto lo dio el compositor.
5. **Sonido**: AAC con Media Foundation y salida con WASAPI.
