# quimera-regex: expresiones regulares

`motor-js/regex` (nivel 1, `no_std`, sin dependencias). Expresiones regulares de JavaScript con la misma arquitectura que irregexp (SpiderMonkey y V8): **patrón → árbol → programa → máquina con vuelta atrás**.

| Archivo | Qué hace |
|---|---|
| `patron.rs` | Analiza el patrón con la sintaxis de JavaScript, incluido el anexo B de los navegadores (`{` suelto es un carácter, `\8` es un `8`) |
| `clases.rs` | Clases (`[a-z]`, `\d`, `\w`, `\s`): rangos ordenados más un mapa de bits para ASCII |
| `programa.rs` | Árbol → operaciones (`Caracter`, `Clase`, `Dividir`, `Guardar`, `Mirar`...) |
| `maquina.rs` | Ejecuta el programa con vuelta atrás |
| `lib.rs` | `Regex::nueva(patrón, banderas)` y `buscar(texto, desde, &mut Memoria)` |

## Decisiones

- **Sin recursión de Rust.** Las alternativas pendientes van en una pila propia (`Memoria`), que se reutiliza entre búsquedas. Un texto de 200.000 caracteres con `(a|b)*` no desborda nada. Solo `(?=...)` recursa, y lo limita el anidamiento del patrón (como mucho 256 niveles).
- **UTF-8 y posiciones en bytes.** Las cadenas de QUIMERA son UTF-8. La VM convierte a índices UTF-16 (`index`, `lastIndex`) solo cuando el texto no es ASCII.
- **Atajos de búsqueda.** Un patrón que es solo texto (`/zbmvyyn/`) se busca con `str::find`, sin máquina. Si el patrón empieza siempre por el mismo texto, la búsqueda salta de aparición en aparición. Con `^` y sin `m`, solo se prueba la posición 0.
- **Bucles vacíos.** Un cuerpo que puede no consumir nada (`(a*)*`) lleva una marca de posición, así que la repetición no gira para siempre.
- **Capturas por vuelta.** Cada vuelta de una repetición borra las capturas de la anterior, como manda la especificación.
- **Un programa por patrón.** En la VM, cada par (patrón, banderas) se compila una sola vez. Un literal dentro de un bucle crea un objeto `RegExp` nuevo en cada vuelta, pero todos comparten el programa.
- **Errores tempranos.** El frontend valida cada literal `/.../` al analizar, como los navegadores.

## Qué soporta

Clases y rangos, `\d \D \w \W \s \S \b \B`, `.`, `^ $`, grupos que capturan, `(?:...)`, grupos con nombre (capturan; el nombre aún no se usa), `(?=...)` y `(?!...)`, referencias `\1`, cuantificadores `* + ? {n} {n,} {n,m}` codiciosos y perezosos, escapes `\t \n \x41 é \u{1F600}` y `\cX`, y las banderas `dgimsuy`.

En la VM: `RegExp` (constructor y literales), `exec`, `test`, `toString`, `lastIndex`, y en cadenas `replace` (con `$1`, `$&`, `` $` ``, `$'` o una función), `match`, `search` y `split`.

**Propiedades Unicode** (`\p{...}` y `\P{...}`, con la bandera `u`; `propiedades.rs`): las categorías `L`, `Lu`, `Ll`, `N`, `P`, `S`, `M`, `Z`, `Cc` (también por su nombre largo y con `General_Category=`), `Alphabetic`, `White_Space`, `ID_Start`, `ID_Continue`, `Emoji`, `Extended_Pictographic`, `ASCII`, `Any` y los alfabetos más usados (`Script=Latin`, `Greek`, `Cyrillic`, `Han`, `Arabic`...). Las letras y los números salen de la biblioteca estándar de Rust; la puntuación, los símbolos, los emojis y los alfabetos son tablas de bloques, aproximadas en los bordes. Un nombre desconocido es un error, como en los navegadores. Probado contra Chromium.

**Falta**: mirar hacia atrás (`(?<=...)`), las tablas completas de Unicode para `\p{...}`, `source`/`flags`/`global` como propiedades, y que `replace`/`split` llamen al `exec` del usuario como pide la especificación.

## Medida

RegExp de V8 (Octane): **12,5× la puntuación de Boa 0.22** (382 frente a 31). La corrección se comprueba aparte, porque el benchmark no se comprueba a sí mismo. `bancos/vm` espía `exec`, `test`, `replace`, `split` y `match` en los dos motores y compara una huella de sus 125.585 resultados: tienen que ser idénticos. Para ver la primera diferencia, si un día la hay:

```bash
cargo test --release -p quimera-banco-vm primera_diferencia -- --ignored --nocapture
```
