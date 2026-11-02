# El frontend de QUIMERA

`motor-js/frontend` (`quimera-frontend`, nivel 2, `no_std`, sin dependencias) convierte JavaScript en bytecode de registros, en tres pasos:

```text
texto ──léxico──▶ tokens ──parser──▶ árbol (AST) ──compilador──▶ Stencil (bytecode)
```

| Módulo | Qué hace | Líneas |
|---|---|---:|
| `lexico.rs` | Tokens: identificadores Unicode, números con `_`, hex/oct/bin, cadenas con escapes y pares sustitutos, signos, comentarios, saltos de línea | 401 |
| `parser/` | Descenso recursivo + precedencia por niveles, `;` automático, flechas, objetos | 3 archivos |
| `ambitos.rs` | Qué variables captura cada función interna | 228 |
| `compilador/` | Árbol → bytecode, con registros y entornos | 3 archivos |
| `bytecode.rs` | Las instrucciones y el desensamblador | 312 |

## Por qué propio y no el parser de Boa

El parser de Boa (unas 49.000 líneas con su AST e interner) depende de su GC, de ICU y de `std`. Adaptarlo habría roto tres reglas de QUIMERA: `no_std`, cero dependencias y los límites de los guardianes. Este se escribió desde cero siguiendo la gramática de ECMAScript, con la estructura del de Boa como referencia.

## Stencil: bytecode sin GC y de registros

Dos ideas de SpiderMonkey:

1. **Stencil**: lo compilado no contiene objetos del GC, solo números, índices y la tabla de átomos. Se puede guardar en caché o compilar en otro hilo.
2. **Variables en registros, capturadas en el entorno**: una variable que ninguna función interna usa vive en un registro. Si alguna la captura, va a un hueco del entorno (los *aliased bindings* de SpiderMonkey). El perfil de Boa midió que copiar valores entre pila y registros le costaba el 8-13 %. Aquí `x += 1` es una sola instrucción sobre el registro de `x`.

Ejemplo real, `basic-closure.js` de Boa:

```text
función 3 inner: 0 parámetros, 3 registros, entorno 0
  0000  r1 = entorno[1][0]        ← x, dos funciones hacia fuera (outer)
  0001  r2 = entorno[0][0]        ← y, de middle
  0002  r0 = r1 Suma r2
  0003  return r0
función 4 main: 0 parámetros, 4 registros, entorno 0
  0000  r0 = 0                    ← n vive en r0
  0001  r2 = 10000000
  0002  r1 = r0 Menor r2
  0003  si no r1 saltar 0010
  ...
  0007  r1 = 1
  0008  r0 = r0 Suma r1           ← n++ sin copias
  0009  saltar 0001
```

## Qué entiende hoy

**Analiza y compila**: `var`/`let`/`const`, funciones (declaradas, expresiones, flechas, métodos, parámetros por defecto, nombre propio), cierres anidados, objetos (abreviados, calculados, métodos), arreglos con huecos, todos los operadores (también `**`, `??`, `&&=`, `>>>=`...), `new`, `this`, `typeof`/`delete`/`void`, `if`, `while`, `do`, `for`, `break`/`continue`, `for-in`/`for-of`, `switch`, `try`/`catch`/`finally` (también con `return`/`break` dentro), `throw`, y el `;` automático.

**Expresiones regulares**: un `/` es una expresión regular si el token anterior no puede terminar una expresión (`x / y` divide; `= /re/` no). Cada literal se valida al analizar con `quimera-regex`, como hacen los navegadores.

**Sintaxis moderna, convertida al analizar** (`parser/azucar.rs`, `parser/clases.rs`): se reescribe en construcciones que el compilador ya entiende, como hace Babel, sin tocar el compilador ni la máquina.

| Escrito | Se convierte en |
|---|---|
| `` `a${x}b` `` (en el léxico) | `("a" + (x) + "b")`; con etiqueta, `f(["a", "b"], (x))` (sin `raw`) |
| `f(...a)`, `o.m(...a)`, `new F(...a)` | `f.apply(undefined, lista)`, `__quimera_metodo(o, "m", lista)`, `__quimera_nuevo(F, lista)` |
| `[a, ...b]`, `{...o, x}` | `[a].concat(__quimera_lista(b))`, `Object.assign({}, o, {x})` |
| `let {a, b: [c] = []} = o`, `[x, y] = [y, x]` | temporales y asignaciones en orden; los valores por defecto con `=== undefined` |
| `function f({a}, ...r)` | un parámetro temporal y un prólogo (`r` sale de `arguments`; en una flecha, función normal con `.bind(this)`) |
| `for (const [k, v] of m)` | la desestructuración dentro del cuerpo |
| `a?.b`, `a?.[k]`, `o.m?.()` | `(t = a) == null ? void 0 : t.b`, con toda la cadena dentro |
| `class B extends A { x = 1; #p; m() { super.m() } static s() {} }` | función constructora y prototipo; `super(...)` es `__quimera_super`, los campos van en el constructor, los métodos no se enumeran |

Los temporales se llaman `\u{1}t3`: no se pueden escribir desde JavaScript y no salen en `for-in`. Los campos `#privados` son propiedades normales con ese nombre (no se enumeran, pero no son privados de verdad).

**Ámbitos de bloque** (`bloques.rs`): el compilador reparte las variables por función, así que un `let`/`const`/`catch (e)` de bloque que tapa a otra variable con el mismo nombre se renombra antes de compilar (`n` → `n\u{1}1`), con sus usos dentro del bloque. Antes, `let n = 1; { const n = 2; } n` daba 2: el código minificado lo hace a menudo (highlight.js reutiliza `n` así). Falta una variable nueva por vuelta en `for (let i...)` cuando la capturan funciones.

**`get`/`set`** en objetos y clases: `Object.defineProperty` tras crear el objeto o la clase. **`{__proto__: p}`**: `Object.setPrototypeOf`.

**Generadores y `async`**: `function*`, `*m()`, `yield`, `yield*`; `async function`, flechas y métodos `async`, `await`. Las `async` llegan al compilador como un generador envuelto (ver [VM.md](VM.md#generadores-y-asyncawait)).

**El crate**: el léxico y el parser son `quimera-parser` (nivel 2) y el análisis de ámbitos y el compilador, `quimera-frontend` (nivel 3). Se partió al pasar de 5.000 líneas.

**Etiquetas** (`fuera: for ... break fuera`, también en bloques y `switch`) y **módulos** (`import`/`export`, ver [DOM.md](DOM.md#módulos)): `analizar_modulo` y `compilar_modulo`.

**Avisa con un error claro**: bloques `static {}` de las clases, escapes `\u` en identificadores, `...rest` en una asignación con patrón (`[a, ...b] = x`; en declaraciones sí vale), `async`/`await`, etiquetas en `break`/`continue`, bloques `static {}` y BigInt.

Corpus de prueba (`motor-js/frontend/tests/datos/`): los 7 scripts pequeños de Boa y los 8 de V8 (Richards, DeltaBlue, Crypto, RayTrace, EarleyBoyer, NavierStokes, Splay, RegExp). Los 15 se analizan, compilan y ejecutan.

## Medida

`cargo run --release -p quimera-banco-frontend`: el corpus repetido 16 veces (8,2 MB, con los 8 benchmarks de V8), cada copia en su función, el mejor de cinco intentos. Windows, misma máquina para los dos:

| | Tiempo | Velocidad |
|---|---:|---:|
| Boa 0.22: texto → árbol | 1.906 ms | 4,3 MB/s |
| **QUIMERA: texto → árbol** | **483 ms** | **16,9 MB/s** |

Con la sintaxis moderna, el análisis de este corpus (que no la usa) se hizo un 4 % más lento: 327 → 340 ms medidos seguidos en la misma máquina (frente a 1.270 ms de Boa, 3,7× en vez de 3,9×). Se probó a leer los argumentos directamente en su lista final (ganó 2/3 de lo perdido) y a apartar los caminos raros como fríos (no se notó).
| QUIMERA: texto → bytecode (todo el frontend) | 702 ms | 11,7 MB/s |

QUIMERA analiza **3,9× más rápido** (en una máquina cargada; antes de añadir RegExp eran 4,2×) (con el corpus anterior de 1,05 MB eran 3,3–3,4×: EarleyBoyer, con su código Scheme muy anidado, castiga más al parser recursivo de Boa, que además necesita una pila nativa grande para no desbordarse). Con todo el camino hasta el bytecode es 2,7× más rápido que Boa produciendo solo su árbol.

**Para ser justos**: el parser de Boa hace más por el camino. Cubre todo ECMAScript (las partes que aquí dan error), comprueba los errores tempranos de la especificación y guarda posiciones en cada nodo. La comparación es honesta para este corpus, pero parte de la diferencia se irá cerrando a medida que QUIMERA cubra más del lenguaje. La meta es mantener la ventaja: léxico en un solo paso, sin interner global y sin GC en el frontend.

## Siguiente

- `quimera-objetos` y `quimera-vm`: ejecutar este bytecode y medir contra Boa y SpiderMonkey (`js140`) en los mismos benchmarks.
- Completar el lenguaje: accesores (`get`/`set`), generadores y `async`/`await` (necesitan que la máquina pueda suspender un marco), etiquetas.
- Posiciones en el bytecode, para los mensajes de error en ejecución.
