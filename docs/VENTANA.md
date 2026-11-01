# La ventana: QUIMERA en pantalla con wgpu

```bash
cargo run --release -p quimera-ventana                      # la página de ejemplo
cargo run --release -p quimera-ventana -- pagina.html       # cualquier página local
cargo run --release -p quimera-ventana -- https://example.com/   # por red (docs/RED.md)
cargo run --release -p quimera-ventana -- pagina.html --captura salida.png 1024 768
```

La rueda del ratón, las flechas, Re Pág/Av Pág, espacio, Inicio y Fin desplazan la página. Al redimensionar la ventana se vuelve a maquetar con el ancho nuevo (y se reevalúan los `@media`), y F5 vuelve a cargar la página (del disco o de la red). El título de la ventana es el `<title>` de la página y su sitio.

`--captura` dibuja con la misma GPU en una imagen y la guarda como PNG. Sirve para comprobar el pintado sin mirar la pantalla, y en pruebas automáticas.

## Las piezas

| Crate | Nivel | Qué hace | Crates de fuera |
|---|---:|---|---|
| `quimera-texto` | 2 | Busca las fuentes del sistema (Windows: Georgia/Times, Segoe UI/Arial, Consolas/Courier; Linux: DejaVu, Liberation, Noto), mide con interletraje, da ascenso y descenso, y rasteriza letras. Si a una cara le falta un carácter (`•`, `→`, emoji), lo busca en las de reserva | `ab_glyph` 0.2.29 (Rust puro) |
| `quimera-gpu` | 2 | Una sola tubería wgpu que dibuja **cuadros**: rectángulos de color y letras recortadas de un atlas de un canal, todo en una pasada y con alfa premultiplicado. Desplazar solo cambia un uniforme; no se vuelve a subir nada | `wgpu` 24.0.5 |
| `quimera-pintar` | 10 | Lista de dibujo → cuadros. Rasteriza cada letra una vez por tamaño en el atlas y sube a la GPU solo las filas nuevas; si el atlas se llena, se vacía y se vuelve a llenar. Da a la maquetación las **mismas medidas** con que luego dibuja | — |
| `quimera-ventana` | 12 | La ventana: eventos, redimensionado, desplazamiento, captura PNG (codificador propio, sin dependencias) | `winit` 0.30.12, `pollster` 0.4 |

`quimera-pintar` no toca la GPU: produce cuadros y píxeles del atlas, así sus pruebas no necesitan tarjeta gráfica.

## Decisiones

- **Colores en sRGB, como los navegadores.** La superficie usa un formato sin sRGB: los colores de CSS se mezclan tal cual, sin pasarlos a lineal.
- **Letras nítidas.** Las letras se colocan en píxeles enteros y se muestrean sin filtrar. Con pantallas de alta densidad, la maquetación va en píxeles CSS y el pintado multiplica por la escala de la ventana (las letras se rasterizan a su tamaño real).
- **Una sola llamada de dibujo** para toda la página: la página de ejemplo son 810 cuadros, y la de `Vec` de la documentación de Rust, unos 100.000.
- **Sin `unsafe`.** wgpu crea la superficie a partir de un `Arc<Window>` con su API segura.

## Medida

La página de ejemplo, en una ventana de 1024×768: leer, analizar, estilo, maquetación con fuentes de verdad y cuadros en **0,6–0,8 ms**. Al desplazar solo se dibuja (un uniforme y una llamada).

## Falta

- Las familias con nombre (`font-family: "Fira Sans"`) aún caen en su genérica; hace falta buscar fuentes por nombre.
- Imágenes (hoy un recuadro gris), enlaces que se puedan pulsar, selección de texto, barra de desplazamiento.
- Pestañas, historial y barra de direcciones (`quimera-navegador`), y la interfaz con VERX. La red ya está (`quimera-red`, ver [RED.md](RED.md)).
- Texto complejo: ligaduras, árabe, índico (*shaping* con harfrust).
