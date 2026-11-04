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

1. **`ArrayBuffer` y arreglos tipados nativos en la VM**: bytes de verdad, con vistas que compartan memoria (`subarray` sin copiar, `DataView` sobre los mismos bytes). Es lo que más pesa: sin ello el reproductor no puede leer su vídeo, y además acelera cualquier página que trabaje con datos binarios.

2. **`MediaSource`, `SourceBuffer` y `<video>`** en el DOM, con su reloj y sus eventos. Los segmentos van a `mp4::Demuxer` para saber qué tiempos hay (`buffered`), e `isTypeSupported` dice que sí solo a H.264 y AAC con parámetros razonables.
3. **Pintar los fotogramas** en la caja del `<video>`.
4. **Sonido**: AAC con Media Foundation y salida con WASAPI.
