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

**Avisa con un error claro**: plantillas `` ` ``, expresiones regulares, clases, `...spread`, desestructuración, `?.`, generadores, `async`/`await`, etiquetas y BigInt.

Corpus de prueba (`motor-js/frontend/tests/datos/`): los 7 scripts pequeños de Boa y 7 de V8 (Richards, DeltaBlue, Crypto, RayTrace, EarleyBoyer, NavierStokes, Splay). Los 14 se analizan, compilan y ejecutan.

## Medida

`cargo run --release -p quimera-banco-frontend`: el corpus repetido 16 veces (6,25 MB, con los 7 benchmarks de V8), cada copia en su función, el mejor de cinco intentos. Windows, misma máquina para los dos:

| | Tiempo | Velocidad |
|---|---:|---:|
| Boa 0.22: texto → árbol | 1.098 ms | 5,7 MB/s |
| **QUIMERA: texto → árbol** | **260 ms** | **24,0 MB/s** |
| QUIMERA: texto → bytecode (todo el frontend) | 375 ms | 16,7 MB/s |

QUIMERA analiza **4,2× más rápido** (con el corpus anterior de 1,05 MB eran 3,3–3,4×: EarleyBoyer, con su código Scheme muy anidado, castiga más al parser recursivo de Boa, que además necesita una pila nativa grande para no desbordarse). Con todo el camino hasta el bytecode es 2,9× más rápido que Boa produciendo solo su árbol.

**Para ser justos**: el parser de Boa hace más por el camino. Cubre todo ECMAScript (las partes que aquí dan error), comprueba los errores tempranos de la especificación y guarda posiciones en cada nodo. La comparación es honesta para este corpus, pero parte de la diferencia se irá cerrando a medida que QUIMERA cubra más del lenguaje. La meta es mantener la ventaja: léxico en un solo paso, sin interner global y sin GC en el frontend.

## Siguiente

- `quimera-objetos` y `quimera-vm`: ejecutar este bytecode y medir contra Boa y SpiderMonkey (`js140`) en los mismos benchmarks.
- Completar el lenguaje: plantillas, expresiones regulares (con `quimera-regex`), clases y desestructuración.
- Posiciones en el bytecode, para los mensajes de error en ejecución.
