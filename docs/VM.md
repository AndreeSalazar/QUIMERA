# La máquina virtual de QUIMERA

Dos crates ejecutan el bytecode del frontend:

| Crate | Nivel | Qué hace |
|---|---:|---|
| `motor-js/objetos` (`quimera-objetos`) | 3 | Formas (*shapes*), cadenas (únicas y liberables), objetos, arreglos densos, funciones, nativas y entornos, sobre `quimera-gc` |
| `motor-js/vm` (`quimera-vm`) | 4 | El intérprete: registros, llamadas, excepciones, cachés en línea y funciones nativas |

Los dos son `no_std` y sin dependencias de fuera.

```rust
let mut motor = quimera_vm::Motor::default();
motor.ejecutar("function main() { return 6 * 7 }")?;
let v = motor.llamar_global("main", &[])?;
assert_eq!(motor.texto(v), "42");
```

## Las decisiones de diseño

1. **Los registros viven en la pila de raíces del GC.** Cada llamada abre un marco `[cierre, this, entorno, objeto nuevo, r0, r1...]` en esa pila. Si el GC mueve un objeto, los registros que lo apuntan se actualizan solos: un registro nunca guarda una dirección vieja. Las funciones que asignan memoria meten antes sus argumentos en la pila y los vuelven a leer después.
2. **Sin recursión de Rust.** Una llamada de JavaScript apila un `Marco` en un `Vec` y el bucle sigue. Una recursión de 20.000 niveles de JavaScript no toca la pila nativa (hay una prueba).
3. **Cachés en línea en dos niveles.** Cada `LeerProp`, `EscribirProp` y `LeerGlobal` recuerda la forma y el hueco de la última vez: si el objeto tiene la misma forma, leer `o.x` es comparar un número y leer un hueco, sin buscar siquiera el nombre de la clave. Si no basta, el segundo nivel son **stubs al estilo CacheIR** (ver abajo). Es la respuesta al 20-38 % que se llevaba la búsqueda de propiedades en el perfil de Boa.
4. **Enteros rápidos.** Sumar, restar, multiplicar y comparar dos enteros no pasa por `f64`. Solo si desborda `i32` se pasa a coma flotante.
5. **Superinstrucciones.**
   - `BinariaInm` lleva el entero dentro de la instrucción (`i + 1`, `x -= 2`).
   - `SaltarSiNoCompara` compara y salta en una sola (`i < n`).
   - Un bucle `for (…; i < n; i++)` pasa de 6 instrucciones por vuelta a 3.
   - `++x` se compila como `x - (-1)`: la resta convierte a número, así que `"5"` da `6` y no `"51"`.
6. **Objetos que crecen sin moverse.** Cada objeto apunta a una celda aparte con sus propiedades. Si se llena, se crea otra más grande y se cambia el puntero, como los *slots* de SpiderMonkey.

## Medida

`cargo run --release -p quimera-banco-vm`: los benchmarks de Boa, `main()` medido, el mejor de tres, misma máquina. Los dos motores devuelven lo mismo en todos.

| Script | QUIMERA | Boa 0.22 | QUIMERA / Boa |
|---|---:|---:|---:|
| basic-call-loop | 6,7 ms | 16,4 ms | 2,44× |
| basic-nested-loop | 18,8 ms | 20,2 ms | 1,07× |
| basic-closure | 839 ms | 2.390 ms | 2,85× |
| closures-create | 13,4 ms | 86,6 ms | 6,48× |
| closures-invoke | 30,0 ms | 95,4 ms | 3,18× |
| properties-access | 24,0 ms | 75,3 ms | 3,14× |
| prototypes-chain | 24,8 ms | 99,2 ms | **4,00×** (antes 3,18×) |
| **Media geométrica** | | | **2,94×** (antes 2,81×) |

### V8 (Octane): los 8 benchmarks

Con su arnés completo: cada benchmark se mide solo durante al menos un segundo y escribe su `SCORE` (más es mejor). QUIMERA usa el reloj que le da el anfitrión (`Motor::con_reloj`). A Boa se le añaden `console.log` y `print`, que `boa_engine` no trae. Boa necesita una pila nativa grande (recursa en Rust): la medida corre en un hilo con 512 MB; QUIMERA no la necesita.

La mediana de tres corridas seguidas (sus medias geométricas: 3,81×, 3,78× y 3,81×), con los dos motores compilados con LTO. Las columnas de la derecha son la ronda anterior (cachés al estilo CacheIR, sin LTO) y el punto de partida:

| Benchmark | QUIMERA | Boa 0.22 | QUIMERA / Boa | Con CacheIR | Al empezar |
|---|---:|---:|---:|---:|---:|
| Richards | 653 | 206 | **3,17×** | 2,91× | 2,47× |
| DeltaBlue | 634 | 183 | **3,46×** | 3,36× | 3,03× |
| Crypto | 611 | 203 | **3,01×** | 3,24× | 2,28× |
| RayTrace | 1.326 | 352 | **3,78×** | 3,94× | 3,60× |
| EarleyBoyer | 1.791 | 398 | **4,49×** | 4,74× | 4,95× |
| NavierStokes | 1.012 | 454 | **2,23×** | 2,30× | 1,73× |
| Splay | 1.796 | 615 | **2,92×** | 2,75× | 2,94× |
| RegExp | 601 | 50 | **11,96×** | 12,53× | 12,69× |
| **Media geométrica** | | | **3,81×** | 3,82× | 3,44× |

Las puntuaciones varían bastante entre corridas (±10 %; RegExp, que no cambió, va de 10,8× a 13,2×), porque el arnés mide tiempo real y la máquina no está quieta: por eso la mediana. EarleyBoyer, Splay y RegExp están dentro de ese ruido. RegExp usa `quimera-regex` ([REGEX.md](REGEX.md)) y no se comprueba a sí mismo, así que el banco compara una huella de sus 125.585 resultados con la de Boa: tienen que ser idénticas.

Para correr NavierStokes y Splay no hizo falta nada nuevo. Para los demás: `arguments` (RayTrace), crear al empezar las globales declaradas con `var` aunque no tengan valor (Crypto), y `parseInt` (EarleyBoyer). Para probar scripts sueltos: `cargo run --release -p quimera-banco-vm -- a.js b.js`.

Las puntuaciones varían entre corridas porque el arnés mide tiempo real. Los demás benchmarks se comprueban a sí mismos: lanzan un `Error` si las cuentas no salen.

Cómo se llegó aquí, en dos sorpresas:
1. **NaN.** El arnés promedia objetos `BenchmarkResult` con su propio `valueOf`. Faltaba `ToPrimitive`: convertir un objeto llamando a sus métodos, lo que obliga a ejecutar JavaScript desde dentro de una operación (`conversion.rs`).
2. **Richards a 0,64×.** Un experimento midió cada patrón por separado: todos ganaban menos `x != null` con `x` objeto. El `ToPrimitive` recién añadido llamaba a `valueOf`/`toString` en `obj == null`, cuando la especificación dice que con `null`/`undefined` nunca se convierte. Además de lento, era incorrecto. Arreglado y con prueba: de 97 a 369 puntos.

Cómo se llegó a los 7 benchmarks de Boa:
- **Primera versión: 2,24×.** `nested-loop` iba a 0,60×, Boa ganaba.
- Un experimento separó el coste: leer el `const n` global y, sobre todo, el número de instrucciones por vuelta.
- **Con las superinstrucciones: 2,81×.** `nested-loop` pasa a ganar.

## Cachés al estilo CacheIR

`motor-js/vm/src/ic.rs`. La investigación (`datos-investigación/docs/PERFIL-BOA-Y-MOTOR-PROPIO.md`) ponía como meta siguiente el **Baseline Interpreter** de SpiderMonkey, 7,3× Boa, gracias a CacheIR: un lenguaje pequeño de cachés que aprenden cada caso.

- **Cada sitio** (`o.x`, `o.x = v`) aprende hasta 4 casos como *stubs*. Cada stub es una lista de **guardas** de forma (la del receptor y la de cada prototipo de la cadena) y una **acción**:
  - leer un hueco, del receptor o de un prototipo (los métodos);
  - la longitud de un arreglo;
  - escribir un hueco;
  - **añadir** la propiedad con la transición de forma ya calculada (`this.x = …` en un constructor);
  - o saber que la propiedad **no existe** en toda la cadena.
- **Sin punteros.** Los stubs guardan solo números de forma, así que el GC puede mover objetos sin invalidar nada, y la cadena se recorre de verdad en cada uso. Cambiar un método, taparlo con una propiedad propia, borrarla o añadir la propiedad más arriba cambia alguna forma y la guarda falla: hay una prueba para cada caso.
- **Megamórfico.** Con 4 stubs, el sitio deja de aprender (como SpiderMonkey) y va por el camino lento.
- Las formas de la cadena de cada stub van en una tabla compartida: la profundidad no tiene límite y un stub corto no ocupa lo de uno largo.

**Lo que mostró el perfil** (`cargo run --release -p quimera-banco-vm --features perfil -- script.js`, que cuenta y cronometra cada tipo de instrucción, descontando lo que se ejecuta dentro de otra): en QUIMERA, buscar propiedades **ya no era lo que más pesaba**, al revés que en Boa. Los stubs solos subieron la media de 3,44× a 3,51×. Lo demás salió del perfil:

1. **Aritmética rápida.** Dos números se operan sin comprobar si hay objetos (`valueOf`) o textos (`+` que concatena). Además, `*`, `&`, `|`, `^`, `<<`, `>>` y `>>>` llevan el entero dentro de la instrucción, no solo `+` y `-`. Crypto: de 2,28× a 3,24×.
2. **`this` en un registro.** El análisis de ámbitos marca qué funciones usan `this`. Esas lo cargan una vez al entrar y cada `this.x` usa ese registro: desaparece una instrucción de cada cinco en Richards.
3. **`apply` sin recursión de Rust.** Abre el marco de la función como una llamada más; el marco suelta también los argumentos que `apply` puso en la pila. 20.000 niveles de `apply` no tocan la pila nativa (hay una prueba).
4. **Lo que no sirvió:** que los objetos nacieran con el sitio de sus propiedades (por su literal o por el último `new` de su función). Casi todos caben en la primera celda de 4 huecos, así que reservaban lo mismo. Se deshizo.

**Dónde queda:** 3,82×, la mitad del camino hacia el 7,3× del Baseline Interpreter. El perfil dice que ahora pesa el **coste de cada instrucción** (unos 3,4 ns de media), no la búsqueda de propiedades: llamar y volver, leer y escribir registros en la pila de raíces, y despachar. Eso es lo siguiente para acercarse al Baseline.

## Acelerar el intérprete

Después de CacheIR, el perfil decía que pesaba el coste de cada instrucción. Un experimento con el banco (`--tiempo script.js`) lo midió por separado, con 9 millones de vueltas:

| Qué | Antes | Después |
|---|---:|---:|
| Vuelta de bucle con 3 instrucciones simples (`n` local) | 13,7 ns | **9,9 ns** |
| Leer una global, de más | +9,4 ns | +6,6 ns |
| Leer `o.n` con la caché acertando, de más | +12 ns | +7,0 ns |
| Llamar a una función y volver (la vuelta entera) | 71 ns | **52 ns** |

Lo que se hizo, de más a menos efecto:

1. **LTO** (`lto = "fat"`, `codegen-units = 1` en `release`). Con 16 unidades y sin LTO, funciones diminutas de otros crates (leer un registro, un campo del montón, el tipo de un valor) quedaban como llamadas de verdad en cada instrucción. Es lo que más aceleró: −23 % en los benchmarks de Boa y entre +14 % y +48 % en V8. **Pero el banco compila a los dos motores igual, así que Boa también se aceleró**: la proporción entre los dos casi no cambia. Compilar en `release` pasa a tardar unos minutos.
2. **La zona principal del GC, seguida** (`quimera-gc`): los campos de los objetos viejos van todos en un bloque, y cada objeto es (inicio, largo). Antes cada uno era un `Vec` propio: un `malloc` al ascenderlo desde la guardería, un salto más en cada lectura y unos 32 bytes de cabecera por objeto. Lo liberado vuelve a una lista de huecos por tamaño. Splay, el benchmark que más objetos conserva, pasó de 2,75× a 2,92× Boa.
3. **Una llamada lee los datos de la función una vez** (antes, dos, y cada una buscaba el objeto cuatro veces), y la lectura con la caché acertando busca el objeto una vez en vez de cuatro.
4. **El paso de cada instrucción y los accesos a registros, en línea** (`#[inline(always)]`).
5. **Bucles rotados**: la condición va al final y salta atrás si se cumple (`SaltarSiCompara`), así que cada vuelta tiene una instrucción menos. No cambió el tiempo de los bucles medidos: el coste estaba en lo que hacen algunas instrucciones, no en cuántas hay.
6. **Lo que no sirvió:** guardar en cada marco una copia compartida (`Rc`) de su código para no buscar la función en cada instrucción. Copiarla en cada llamada costaba más que la búsqueda, que el compilador ya resolvía bien. Se deshizo.

Para medir así: `--rapido` (solo QUIMERA con los benchmarks de Boa, en segundos), `--tiempo a.js` (lo que tarda `main()`) y `--bytecode a.js` (lo que genera el compilador).

**Dónde queda:** cada instrucción simple cuesta unos 3,3 ns (unos 13 ciclos) y una llamada unos 22 ns. Lo siguiente sería tocar la forma del intérprete: los registros como un trozo de memoria propio del marco en vez de la pila de raíces compartida, y el despacho sin `Result` por instrucción.

## `finally`, `for-in`/`for-of` y `delete`

- **`finally` sin saltos indirectos.** Cada `try` abre una *región* mientras se compila. Un `return`, `break` o `continue` que sale de ella cierra su manejador (`FinIntentar`) y ejecuta una copia del `finally` en ese mismo punto (`compilador/control.rs`). Esto arregló también un fallo latente: un `break` que salía de un `try` dejaba su manejador activo.
- **`for-in`/`for-of`.** `IterarClaves`/`IterarValores` dan un arreglo nuevo que el bucle recorre por índice. Las claves que pone la biblioteca (`push`, `toString`...) y el `constructor` de `F.prototype` no son enumerables (`iteracion.rs`).
- **`delete` sin mover el objeto.** `Mundo::borrar` rehace la forma sin esa clave y corre los huecos de detrás un sitio, en su celda: no asigna memoria. Las cachés en línea se invalidan solas porque la forma cambia.

## Cadenas que se liberan

Cada texto vivo tiene un número único (`quimera-objetos/src/cadenas.rs`): `===` entre cadenas y buscar una propiedad siguen siendo comparar números. Hay dos clases:

- **Átomos**: claves de propiedades y textos del código. Las formas y el bytecode guardan su número, así que no se liberan. Una cadena dinámica se convierte en átomo solo al **escribirla** como clave (`o[k] = v`). Para leer basta su número: como el texto es único, una dinámica nunca coincide con una clave de una forma.
- **Dinámicas**: lo que crea el programa (`a + b`, `replace`...). La recolección mayor anota qué cadenas ve al marcar, y las que no vio se liberan; su número se reutiliza.

Las cadenas viven fuera del montón, así que crearlas no llenaba la guardería y un bucle de solo texto nunca provocaba una recolección. Ahora cuentan como **presión externa**: al pasar de 8 MB (o de lo vivo tras la última limpieza), la siguiente asignación hace una mayor. También la hace un salto atrás del intérprete, que es un punto seguro porque todo lo vivo está en los registros. Nunca se recoge dentro de `cadena()`, donde el código de Rust puede tener objetos sin proteger.

Prueba: un millón de textos distintos en un bucle libera más de 900.000. Para buscar cadenas usadas sin proteger, la suite entera (los 8 de V8, y RegExp comparado con Boa) pasó con el umbral a 1 KB, es decir, con una limpieza casi en cada bucle.

## Objetos y funciones del anfitrión

Para el DOM ([DOM.md](DOM.md)), la VM deja que quien la usa ponga sus propias funciones y objetos (`anfitrion.rs`):

- **Funciones del anfitrión:** nativas con un número del anfitrión (desde 2^24). La máquina le pasa ese número, `this` y los argumentos, leídos de la pila de raíces, así que siguen valiendo aunque se asigne memoria.
- **Objetos del anfitrión:** una clase de celda más (`Clase::Anfitrion`) con un número dentro, el del nodo. Leer o escribir una propiedad suya pregunta primero al anfitrión. Las cachés en línea los excluyen en sus dos comprobaciones rápidas (una comparación más sobre el tipo, que ya se leía) y no los aprenden nunca. V8 pasó de 3,81× a 3,80× Boa, dentro del ruido.
- **Raíz del anfitrión:** una raíz fija más, un objeto donde el anfitrión guarda lo que debe durar (los objetos de los nodos, prototipos, listeners, temporizadores). Como el montón mueve objetos, el anfitrión no guarda `Valor`es de objetos en Rust entre llamadas.

## Qué falta

- **Leer una global cuesta unos 10 ns.** Es lo que queda de `nested-loop`. Inyectar el valor de un `const` rompería la TDZ (leerlo antes de su declaración debe fallar), así que necesita una caché con guardas, no un atajo.
- **Biblioteca estándar, todavía parcial.** Hay `console.log`/`print`; `Object` (`create`, `keys`, `prototype.hasOwnProperty`); `Array` (`isArray`, `push`, `pop`, `shift`, `unshift`, `slice`, `concat`, `reverse`, `join`, `indexOf`, `includes`, `sort`, `forEach`, `map`, `filter`, `reduce`, `some`, `every`, `find`, `findIndex`); `Function.prototype` (`call`, `apply`); `Math` (`floor`, `ceil`, `round`, `abs`, `max`, `min`, `sqrt`, `random`, `log`, `exp`, `pow`, `PI`, `E`); números (`Number`, `parseInt`, `parseFloat`, `isNaN`, `isFinite`, `toFixed`, `toPrecision`, `toString(base)`); `String` (`fromCharCode`, `charAt`, `charCodeAt`, `indexOf`, `slice`, `substring`, `split`, `toUpperCase`, `toLowerCase`, `trim`, `includes`, `startsWith`, `endsWith`); `Date` (`now`, `getTime`) y `Error`. Lo demás da `undefined`.
- **`sort` es inserción binaria.** Estable y sin memoria extra, con pocas comparaciones (n log n), pero mueve O(n²) elementos: para arreglos grandes hará falta un *merge sort* con raíces.
- **`ToPrimitive` usa una llamada interna** (recursión de Rust); `call` y `apply` dejan el marco en la máquina.
- **`this` primitivo sin envolver.** En modo no estricto, `f.call(5)` debería dar un `this` objeto (`Number`); QUIMERA da el número tal cual.
- **El coste por instrucción.** Unos 3,3 ns cada instrucción simple y 22 ns una llamada: el despacho, los registros en la pila de raíces y el marco de cada llamada. Es lo que separa a QUIMERA del Baseline Interpreter de SpiderMonkey (7,3× Boa).
- **Matemáticas sin `std`.** `ln` y `exp` están hechas a mano (reducción de rango, ln 2 partido al estilo Cody-Waite de fdlibm), con precisión de pocos bits. Si un día se necesita la exactitud de una libm, conviene portar fdlibm entera.
- **Errores de la máquina.** Los que lanza la máquina son cadenas (`"TypeError: ..."`); `new Error()` desde JavaScript sí crea objetos.
- **Cadenas en UTF-8.** JavaScript cuenta en UTF-16; se convierte donde hace falta (`charAt`, índices de `RegExp`). `quimera-cadenas` (UTF-16/Latin-1 compartidas) lo evitará.
- **`for-of` solo con arreglos y cadenas.** Sin `Symbol.iterator` todavía; `for-in` y `for-of` reciben una copia de claves o valores al empezar.
- **RegExp, casi completa.** Falta lo que lista [REGEX.md](REGEX.md); entre otras cosas, `replace` y `split` no llaman al `exec` del usuario. Los escapes `\u` en identificadores tampoco están todavía.
- **`arguments` es un arreglo.** Se crea solo en las funciones que lo usan (las demás no pagan nada), pero es una copia: no está enlazado a los parámetros como en el modo no estricto.
