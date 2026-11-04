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

## Falta

- `MediaSource`, `SourceBuffer` y `<video>` en el DOM, con su reloj y sus eventos.
- Pintar los fotogramas en la caja del `<video>`.
- Sonido: AAC con Media Foundation y salida con WASAPI.
- Que el reproductor de YouTube elija H.264 + AAC: `MediaSource.isTypeSupported` solo dirá que sí a esos.
