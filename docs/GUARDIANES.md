# Los guardianes de QUIMERA

Los guardianes son la base de todo lo demás. Su trabajo es que el proyecto **no se corrompa, no sorprenda y no pierda el camino**. El objetivo de QUIMERA es poder encontrar siempre la aguja en el pajar: si un archivo no puede crecer hasta ser un monolito, si cada pieza tiene su nivel, y si lo peligroso está contado y explicado, entonces nunca hay pajar.

```bash
cargo run -p quimera-guardianes
```

```bash
cargo run -p quimera-guardianes -- --estricto
```

```bash
cargo run -p quimera-guardianes -- --reglas
```

```bash
cargo run -p quimera-guardianes -- --sarif guardianes.sarif
```

Con `--sarif`, además del informe, los hallazgos se escriben en **SARIF 2.1.0**, el formato estándar de los analizadores. Un rojo es `error`, un amarillo sin revisar es `warning` y uno revisado es `note`, con su motivo de `guardianes.toml` como `suppressions`. Lo que es del crate entero apunta a su `Cargo.toml`. Así lo leen otras herramientas: [SUPER GRAFO](https://github.com/AndreeSalazar/ULTRA-LinuX) pinta el semáforo sobre sus archivos (2D y la Ciudad 3D) y se lo cuenta a una IA por MCP. El archivo no va al repositorio (`.gitignore`).

## El semáforo

Cada crate es un **carril** con su color, y debajo, cada hallazgo:

```text
🟢 nivel   0  cimientos     quimera-valor          1 archivo, 368 líneas, el mayor 368/1000
🟢 nivel   1  motor-js      quimera-gc             1 archivo, 525 líneas, el mayor 525/1000
      🟡 A-UNWRAP motor-js/gc/src/lib.rs:123 — expect() fuera de las pruebas  [revisado: ...]
```

| Color | Significa | Qué pasa |
|---|---|---|
| 🔴 Rojo | Algo que no puede quedarse así | **Bloquea** el commit. Hay que arreglarlo |
| 🟡 Amarillo | Algo que puede estar bien, pero alguien tiene que mirarlo | Hay que **revisarlo** y aceptarlo en `guardianes.toml` con un motivo. En modo estricto (el del commit), un amarillo sin revisar cuenta como rojo |
| 🟢 Verde | Limpio, o amarillo ya revisado | Pasa |

## Las reglas

| Regla | Color | Exige | Por qué |
|---|---|---|---|
| `R-TAMANO` | 🔴 | Ningún archivo `.rs` pasa de **1000 líneas** | Un archivo enorme es un monolito: nadie lo revisa entero y ahí se esconde la aguja |
| `R-NIVEL` | 🔴 | Un crate solo usa crates de **niveles menores** | La jerarquía permite probar y sustituir cada pieza sola |
| `R-EXTERNO` | 🔴 | Solo crates de fuera declarados en `externos` | Cada dependencia ajena es código que no controlamos |
| `R-NOSTD` | 🔴 | Si declara `no_std`, lo es de verdad | El núcleo tiene que vivir en ULTRA sin la biblioteca estándar |
| `R-UNSAFE` | 🔴 | Sin `unsafe` salvo `permite_unsafe = true` | `unsafe` apaga las garantías: se concentra en pocos sitios conocidos |
| `R-SAFETY` | 🔴 | Cada `unsafe {}` o `unsafe impl` lleva `// SAFETY:` justo antes | Sin el porqué nadie puede revisar la invariante |
| `R-META` | 🔴 | Cada crate declara nivel y capa, y su carpeta es su capa | Sin metadata no se puede vigilar la jerarquía |
| `R-REVISION` | 🔴 | Cada revisión de `guardianes.toml` corresponde a un amarillo actual y tiene motivo | Una revisión vieja taparía problemas futuros |
| `A-TAMANO` | 🟡 | Un archivo de más de 700 líneas | Partirlo antes de que sea urgente |
| `A-FUNCION` | 🟡 | Una función de más de 80 líneas | Hace varias cosas: cuesta leerla y probarla |
| `A-UNWRAP` | 🟡 | `unwrap`/`expect` fuera de las pruebas | Convierte un error esperable en un panic |
| `A-PANIC` | 🟡 | `panic!`, `todo!`, `unimplemented!`, `unreachable!` fuera de las pruebas | Un navegador no debe caerse por un dato raro |
| `A-ALLOW` | 🟡 | Un `#[allow(...)]` | Apagar un aviso es una decisión que debe quedar explicada |
| `A-PENDIENTE` | 🟡 | `TODO:`, `FIXME:` o `XXX:` en un comentario | Lo pendiente se olvida si no está a la vista |
| `A-SINPRUEBAS` | 🟡 | Un crate sin ninguna prueba | Un cambio podría romperlo sin que nadie lo sepa |
| `A-CRATE` | 🟡 | Un crate de más de 5000 líneas | Un crate grande suele esconder dos piezas: ULTRA modular |

Las pruebas (`#[cfg(test)]`, `#[test]`, la carpeta `tests/`, los archivos `pruebas.rs`/`tests.rs` o con `#![cfg(test)]`) pueden usar `unwrap` y `panic!` libremente.

## Aceptar un amarillo

```toml
# guardianes.toml
[[revisado]]
regla = "A-UNWRAP"
archivo = "motor-js/gc/src/lib.rs"   # o el nombre del crate, para reglas de crate entero
motivo = "por qué es correcto aquí"
```

La revisión vale para todos los amarillos de esa regla en ese archivo. Si el amarillo desaparece, la revisión se queda sin uso y los guardianes la marcan en 🔴 para que se borre.

## Dónde actúan

- **En cada commit**: `.githooks/pre-commit` pasa los guardianes en modo estricto, `cargo fmt --check`, `clippy -D warnings` y todas las pruebas. Se activa una vez por copia del repositorio con `git config core.hooksPath .githooks`.
- **En las pruebas**: `quimera_pasa_sus_propios_guardianes` falla si QUIMERA no pasa en modo estricto, así que `cargo test` también lo vigila.
- **Sobre sí mismos**: los guardianes son un crate más y cumplen sus propias reglas. Al escribirlos encontraron 4 cosas en el propio código: una función de 116 líneas, que se partió en cinco; un banco sin pruebas, que ahora tiene una; un `#[allow]` innecesario, que se quitó; y su propio comentario con un `TODO:`.

## Cómo está hecho

`herramientas/guardianes`, sin dependencias externas, siete archivos de menos de 300 líneas:

| Archivo | Qué hace |
|---|---|
| `main.rs` | Línea de órdenes e informe con carriles |
| `semaforo.rs` | Colores, reglas y hallazgos |
| `reglas.rs` | Cada regla, una función |
| `lexico.rs` | Lexer de Rust: separa código de cadenas y comentarios; sabe qué es prueba |
| `proyecto.rs` | Lee el workspace, la metadata y `guardianes.toml` |
| `toml.rs` | Lector mínimo de TOML |
| `pruebas.rs` | Proyectos de mentira para probar cada regla |
