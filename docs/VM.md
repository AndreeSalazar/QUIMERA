# La máquina virtual de QUIMERA

Dos crates ejecutan el bytecode del frontend:

| Crate | Nivel | Qué hace |
|---|---:|---|
| `motor-js/objetos` (`quimera-objetos`) | 3 | Formas (*shapes*), cadenas internadas, objetos, arreglos densos, funciones, nativas y entornos, sobre `quimera-gc` |
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
3. **Cachés en línea.** Cada `LeerProp`, `EscribirProp` y `LeerGlobal` recuerda la forma y el hueco de la última vez. Si el objeto tiene la misma forma, leer `o.x` es comparar un número y leer un hueco. Es la respuesta al 20-38 % que se llevaba la búsqueda de propiedades en el perfil de Boa.
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
| basic-call-loop | 5,9 ms | 15,9 ms | 2,67× |
| basic-nested-loop | 18,1 ms | 20,4 ms | 1,13× |
| basic-closure | 834 ms | 2.257 ms | 2,71× |
| closures-create | 15,1 ms | 84,3 ms | 5,60× |
| closures-invoke | 31,4 ms | 95,9 ms | 3,06× |
| properties-access | 23,5 ms | 74,3 ms | 3,16× |
| prototypes-chain | 30,7 ms | 97,7 ms | 3,18× |
| **Media geométrica** | | | **2,81×** |

### Richards y DeltaBlue (V8, Octane)

Con su arnés completo: cada benchmark se mide solo durante al menos un segundo y escribe su `SCORE` (más es mejor). QUIMERA usa el reloj que le da el anfitrión (`Motor::con_reloj`). A Boa se le añaden `console.log` y `print`, que `boa_engine` no trae.

| Benchmark | QUIMERA | Boa 0.22 | QUIMERA / Boa |
|---|---:|---:|---:|
| Richards | 356–369 | 127–152 | **2,5–2,8×** |
| DeltaBlue | 379–388 | 131–134 | **2,9–3,0×** |

Las puntuaciones varían un poco entre corridas porque el arnés mide tiempo real. Los dos benchmarks se comprueban a sí mismos: lanzan un `Error` si las cuentas no salen.

Cómo se llegó aquí, en dos sorpresas:
1. **NaN.** El arnés promedia objetos `BenchmarkResult` con su propio `valueOf`. Faltaba `ToPrimitive`: convertir un objeto llamando a sus métodos, lo que obliga a ejecutar JavaScript desde dentro de una operación (`conversion.rs`).
2. **Richards a 0,64×.** Un experimento midió cada patrón por separado: todos ganaban menos `x != null` con `x` objeto. El `ToPrimitive` recién añadido llamaba a `valueOf`/`toString` en `obj == null`, cuando la especificación dice que con `null`/`undefined` nunca se convierte. Además de lento, era incorrecto. Arreglado y con prueba: de 97 a 369 puntos.

Cómo se llegó a los 7 benchmarks de Boa:
- **Primera versión: 2,24×.** `nested-loop` iba a 0,60×, Boa ganaba.
- Un experimento separó el coste: leer el `const n` global y, sobre todo, el número de instrucciones por vuelta.
- **Con las superinstrucciones: 2,81×.** `nested-loop` pasa a ganar.

## `finally`, `for-in`/`for-of` y `delete`

- **`finally` sin saltos indirectos.** Cada `try` abre una *región* mientras se compila. Un `return`, `break` o `continue` que sale de ella cierra su manejador (`FinIntentar`) y ejecuta una copia del `finally` en ese mismo punto (`compilador/control.rs`). Esto arregló también un fallo latente: un `break` que salía de un `try` dejaba su manejador activo.
- **`for-in`/`for-of`.** `IterarClaves`/`IterarValores` dan un arreglo nuevo que el bucle recorre por índice. Las claves que pone la biblioteca (`push`, `toString`...) y el `constructor` de `F.prototype` no son enumerables (`iteracion.rs`).
- **`delete` sin mover el objeto.** `Mundo::borrar` rehace la forma sin esa clave y corre los huecos de detrás un sitio, en su celda: no asigna memoria. Las cachés en línea se invalidan solas porque la forma cambia.

## Qué falta

- **Leer una global cuesta unos 10 ns.** Es lo que queda de `nested-loop`. Inyectar el valor de un `const` rompería la TDZ (leerlo antes de su declaración debe fallar), así que necesita una caché con guardas, no un atajo.
- **Biblioteca estándar, todavía parcial.** Hay `console.log`/`print`; `Object` (`create`, `keys`, `prototype.hasOwnProperty`); `Array` (`isArray`, `push`, `pop`, `shift`, `unshift`, `slice`, `concat`, `reverse`, `join`, `indexOf`, `includes`, `sort`, `forEach`, `map`, `filter`, `reduce`, `some`, `every`, `find`, `findIndex`); `Function.prototype` (`call`, `apply`); `Math` (`floor`, `ceil`, `round`, `abs`, `max`, `min`, `sqrt`, `random`, `log`, `exp`, `pow`, `PI`, `E`); números (`toFixed`, `toPrecision`, `toString`); `String` (`fromCharCode`, `charAt`, `charCodeAt`, `indexOf`, `slice`, `substring`, `split`, `toUpperCase`, `toLowerCase`, `trim`, `includes`, `startsWith`, `endsWith`); `Date` (`now`, `getTime`) y `Error`. Lo demás da `undefined`.
- **`sort` es inserción binaria.** Estable y sin memoria extra, con pocas comparaciones (n log n), pero mueve O(n²) elementos: para arreglos grandes hará falta un *merge sort* con raíces.
- **`apply` usa una llamada interna.** Recursión de Rust (como `ToPrimitive`); `call` en cambio deja el marco en la máquina.
- **Matemáticas sin `std`.** `ln` y `exp` están hechas a mano (reducción de rango, ln 2 partido al estilo Cody-Waite de fdlibm), con precisión de pocos bits. Si un día se necesita la exactitud de una libm, conviene portar fdlibm entera.
- **Errores de la máquina.** Los que lanza la máquina son cadenas (`"TypeError: ..."`); `new Error()` desde JavaScript sí crea objetos.
- **Cadenas.** Viven fuera del GC y no se liberan; las cadenas UTF-16 de `quimera-cadenas` lo arreglarán.
- **`for-of` solo con arreglos y cadenas.** Sin `Symbol.iterator` todavía; `for-in` y `for-of` reciben una copia de claves o valores al empezar.
- **Más benchmarks de V8.** RayTrace, NavierStokes, Crypto, EarleyBoyer, RegExp y Splay necesitan más lenguaje y más biblioteca (expresiones regulares, clases, plantillas...).
