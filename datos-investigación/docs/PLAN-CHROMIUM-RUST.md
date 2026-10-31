# Plan de Chromium personal con núcleo Rust no_std

## Decisión de arquitectura propuesta

Objetivo inicial: navegador basado en Chromium que funcione en ULTRA-LinuX. La lógica propia que no necesita servicios del sistema se implementa en crates `no_std`; adaptadores explícitos conectan esa lógica con Chromium y Linux. Esta propuesta permite resultados utilizables durante la construcción.

Si la exigencia final es que TODO el navegador sea Rust `no_std`, abrir una línea de motor nuevo: HTML, CSS, diseño, JavaScript, red/TLS, fuentes, multimedia, accesibilidad, GPU y aislamiento. El código de Chromium serviría de referencia, pero no sería un fork que compila tras una conversión automática. Ese alcance aún requiere una decisión del autor.

## Etapas con salida comprobable

| Etapa | Trabajo | Criterio para avanzar |
|---|---|---|
| 0. Referencia | Conservar ULTRA, historia, licencias, mapa y límites | Bundle verificado, copia íntegra e inventario. Hecho en esta entrega |
| 1. Taller vivo | Recuperar acceso al QEMU existente, identificar discos/copia y versiones | SSH funciona; registrar commit, compiladores y recursos reales. Pendiente |
| 2. Chromium de referencia | Fijar revisión upstream, licencias y dependencias; preparar disco Linux y herramientas oficiales | Compilar y arrancar Chromium sin cambios en el entorno elegido. Pendiente |
| 3. Mapa del navegador | Abrir el checkout completo en SUPER GRAFO; generar base de compilación con rutas reales | Resumen, módulos, dependencias contrastadas con Clang y cronología acotada. Pendiente |
| 4. Primer módulo Rust | Elegir una función propia pequeña y pura tras analizar el mapa | Contrato de entradas/salidas, pruebas de comportamiento y compilación no_std. Pendiente |
| 5. Integración | Adaptador C++/Rust usando mecanismo admitido por esa revisión de Chromium y objetivos GN | Pruebas de integración, errores y propiedad de memoria definidos. Pendiente |
| 6. Navegador personal | Controles propios, configuración e identidad visual; integrar con ULTRA | Navegación local y HTTPS, pestañas, descargas, permisos y recuperación. Pendiente |
| 7. Validación | Ejecutar en ULTRA/QEMU y después comparar en PC físico | Resultados reproducibles de funcionalidad, sandbox, recursos y compatibilidad. Pendiente |

No descargar Chromium ni sus dependencias antes de conocer espacio libre en el disco Linux, memoria, versión elegida y coste de compilación. La carpeta actual solo contiene la referencia de ULTRA.

## Diseño de carpetas futuro

```text
Chromiun personal/
  referencia/ULTRA-LinuX/       # referencia independiente con Git
  referencia/ultra-linux.bundle
  docs/                       # decisiones y planes
  evidencias/                 # identidad, inventario, resultados
  scripts/                    # consulta y verificación
  chromium/                   # futuro checkout upstream, todavía ausente
  crates/navegador-nucleo/     # futura lógica no_std, todavía ausente
  adaptadores/                # futura FFI y plataforma, todavía ausente
```

Primer candidato: estado de pestañas y comandos propios, sin interpretar contenido web. Usar acciones que producen efectos, como Carpetas; Chromium seguirá ejecutando navegación, renderizado y servicios. Separar decisiones de APIs de plataforma hace posible probar el núcleo sin pantalla. No reutilizar automáticamente las estructuras de Carpetas para un dominio distinto.

## Investigación mediante el grafo

1. `resumen` de toda la referencia para establecer alcance real.
2. `cronologia` para la evolución; luego `archivo` en `lib.rs`, `consultas.rs` y los adaptadores.
3. `combinar` y `camino` para explicar cruces; `peso` para priorizar cambios de menor impacto.
4. Con Chromium disponible, repetir por grupos: `base`, `content`, `chrome`, `components`, `net`, Blink, V8, GPU y sandbox. Estas son áreas de investigación propuestas, no grupos ya medidos aquí.
5. Contrastar includes con el compilador y cabeceras generadas reales. Una consulta lexical aislada no valida todas las ramas del preprocesador.
6. Registrar por decisión: pregunta, ruta/línea, commit, evidencia, límite, alternativa y siguiente prueba.

## Pruebas que deben aportar evidencia

- Núcleo: transiciones reales, comandos inválidos y límites de recursos; `cargo check --target x86_64-unknown-none` si ese destino y sus dependencias están instalados. Esto comprueba una biblioteca, no un navegador arrancable.
- FFI: propiedad, vida útil, codificación, límites de búfer, propagación de errores y política de panic; probar los contratos del adaptador.
- Navegador: documento local primero, después servidor de prueba y HTTPS; conservar aislamiento de procesos y sandbox.
- QEMU: arranque, Wayland, entrada, persistencia, fallos y recuperación. Registrar si la GPU es software.
- PC: rendimiento y energía con la misma carga, versiones y configuración. No extrapolar FPS ni vatios de QEMU.

## Licencias y mantenimiento

La referencia ULTRA y su núcleo declaran GPL-2.0-only; una reutilización de código requiere revisar las obligaciones aplicables a la integración y distribución elegida. Conservar licencias de Chromium y cada dependencia. La organización inicial no decide una licencia global del futuro navegador.

Cada entrega debe fijar revisión Chromium, toolchain hermético, parches y estado de actualizaciones. El historial del grafo explica relaciones y cambios; las decisiones de seguridad y mantenimiento siguen requiriendo revisión y pruebas.
