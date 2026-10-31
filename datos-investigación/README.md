# Chromium personal: guía desde ULTRA-LinuX

Preparado el 31-10-2026 para Eddi Salazar. Objetivo corregido: cargar el codigo abierto de Chromium en la aplicacion SUPER GRAFO del Linux personal, analizar la raiz completa y mostrar su Ciudad 3D. La referencia y el plan de la primera entrega son material auxiliar.

El trabajo real se realiza en dos VM: el taller existente descarga las fuentes; una VM aparte arranca el kernel y sistema ULTRA del autor y contiene la aplicacion instalada. `vm/chromium-fuentes.qcow2` es el disco exclusivo de fuentes y evidencias, montado dentro de ULTRA en `/home/ultra/Chromium`. La imagen original de ULTRA se abre con `snapshot=on`.

Estado de esta ronda: se recupero el acceso SSH al taller usando su configuracion KEX, se inicio la descarga de Chromium, se arranco ULTRA y se comprobo la ventana de `ultra-carpetas` en Hyprland. El analisis de Chromium no se da por terminado hasta guardar resultados del motor y comprobar recortes y alcance de las dependencias. Consultar `evidencias/chromium-real/`.

- [QUIMERA: perfil de Boa y diseño del motor JavaScript propio (Boa + SpiderMonkey)](docs/PERFIL-BOA-Y-MOTOR-PROPIO.md).
- [Servo, Boa y SpiderMonkey con SUPER GRAFO: cómo están hechos y velocidad medida](docs/INVESTIGACION-MOTORES-RUST.md).
- [Plan: Chromium personal en Rust no_std, pieza a pieza y aislado](docs/PLAN-CHROMIUM-PERSONAL.md).
- [Datos de la jerarquía completa de Chromium y orden de construcción](datos/chromium-jerarquia/LEEME.md).
- [Chromium completo en SUPER GRAFO: capas, pesos, Rust y fiabilidad de señales](docs/INVESTIGACION-CHROMIUM-COMPLETA.md).
- [Análisis y mapa de piezas](docs/ANALISIS-ULTRA.md).
- [Plan de construcción y criterios de avance](docs/PLAN-CHROMIUM-RUST.md).
- `referencia/ULTRA-LinuX/`: copia independiente de los archivos versionados y su historial Git, lista para abrir en SUPER GRAFO.
- `referencia/ultra-linux.bundle`: todas las referencias exportables del Git local, para recuperar historia y ramas.
- `evidencias/`: inventario completo, historial, identidad y comprobaciones de esta extracción.
- `scripts/consultar-super-grafo.sh`: consultas acotadas para ejecutar en el taller Linux con el grafo compilado.

La referencia fija `4778c674a43511dbb5cd166f1e8859182bddc4a4`. Se conserva COPYING y la autoría de cada archivo. El proyecto original permanece separado.

“Todos los datos” aquí significa todos los archivos seguidos por Git y el historial local exportado. Los directorios ignorados `vm`, `build`, `upstream`, `herramientas` y `privado` no están en ese Git y no se han copiado. Tampoco se han extraído credenciales, perfiles del navegador ni datos de juegos. El grafo no explica automáticamente decisiones que nunca se documentaron: los motivos se investigan en commits, documentos, código y pruebas.

La dirección propuesta es Chromium sobre ULTRA-LinuX con módulos propios Rust `no_std` y adaptadores de plataforma. Un navegador entero escrito en Rust sin `std` sería un motor nuevo, con una escala distinta de trabajo. Esa alternativa queda descrita en el plan.

## Primera lectura

Abrir `referencia/ULTRA-LinuX/docs/SUPER-GRAFO.md`, después `src/apps/carpetas-nucleo/src/lib.rs`, `src/apps/ultra-carpetas/src/consultas.rs` y `docs/ESTADOS-Y-VIGENCIA.md`. El núcleo y la aplicación muestran la separación real entre decisiones puras y servicios Linux.

## QEMU

En la primera ronda, dos intentos SSH locales al puerto 12222 fallaron durante el intercambio de identificacion. En la segunda ronda se uso `KexAlgorithms=curve25519-sha256` y acceso a la clave local, y el taller respondio. Se ejecuto `resumen` del SUPER GRAFO sobre las fuentes de ULTRA y se arranco otra VM con kernel `7.3.0-rc7-pc-g32c52ec87d62`; estos hechos no son todavia resultados de Chromium. Las evidencias copiadas del Git conservan sus fechas originales.
