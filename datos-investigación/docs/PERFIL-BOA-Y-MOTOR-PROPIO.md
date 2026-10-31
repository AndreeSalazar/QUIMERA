# Perfil de Boa y diseño de QUIMERA, el motor JavaScript propio

31-10-2026. Boa 0.22 (`329e8974`) se compiló con símbolos y *frame pointers*, y se perfiló con `perf` 7.3.1 (`cpu-clock`, 997 Hz) en el taller QEMU sobre seis benchmarks V8. Datos: `datos/motores/boa/perfil/`. Script: `scripts/perfilar-boa.sh`.

## Dónde se va el tiempo de Boa

Porcentaje del tiempo propio de cada función (solo las funciones que pasan del 0,7 %), agrupado por categoría:

| Categoría | Crypto | EarleyBoyer | RayTrace | RegExp | Richards | Splay |
|---|---:|---:|---:|---:|---:|---:|
| **Búsqueda de propiedades** | **37,9** | **27,1** | **33,3** | 15,5 | **33,6** | 19,5 |
| Despacho de instrucciones (`Context::run`) | 13,0 | 8,6 | 8,0 | — | 16,2 | 7,2 |
| Copias de valores (`move`, `push`, `pop`) | 12,7 | 8,7 | 7,7 | — | 9,3 | 10,9 |
| Llamadas y retornos | — | 6,6 | 6,9 | 2,5 | 10,5 | 4,0 |
| GC: marcar y trazar | — | 7,3 | 0,9 | 0,7 | — | **27,1** |
| Expresiones regulares (`regress`) | — | — | — | **48,0** | — | — |
| Aritmética | 19,1 | — | — | — | — | 2,4 |
| `malloc` | — | 2,5 | 2,6 | 3,9 | — | 1,5 |

Funciones concretas que más pesan:

- `HashMap<PropertyKey, Slot>::get`: 5–7 % en EarleyBoyer, RayTrace y RegExp. La forma (*shape*) del objeto se consulta en una tabla hash cuando la caché en línea falla.
- `get_by_name` + `InlineCache::get`: 18 % en Richards.
- `handle_move`: 12 % en Crypto. Copiar un valor de un registro a otro.
- `PropertyMap::trace` + `mark_heap`: 17 % en Splay. Recorrer el montón entero en cada recolección.
- `regress::classicalbacktrack`: 46 % en RegExp. Un intérprete de expresiones regulares con *backtracking*.

## Por qué, visto en el código

1. **Copiar un valor no es gratis.** `JsValue` usa NaN-boxing, pero su `clone` suma un contador de referencias (`Gc::clone` → `inc_ref_count`) cuando lleva un objeto o una cadena, y al soltarlo lo resta. El GC usa ese contador para saber qué está vivo. Cada `move` entre registros paga una suma, una resta y un salto.
2. **Cada instrucción es una llamada indirecta.** `OPCODE_HANDLERS[opcode](context, pc)` es una tabla de 256 funciones. El estado de la máquina vive en `Context` y no en registros de la CPU, así que entre instrucciones se carga y se guarda.
3. **Las propiedades caen a menudo al camino lento.** La caché en línea existe (`vm/inline_cache`), pero al fallar se consulta una tabla hash de la *shape*. Los arreglos pasan por `array_exotic_define_own_property` incluso en accesos simples.
4. **El GC recorre todo.** `boa_gc` es *mark-sweep* sin generaciones: cada recolección traza el montón entero, aunque casi todo lo nuevo muere joven.
5. **Las expresiones regulares se interpretan** con *backtracking* clásico (`regress` 0.12).

## La respuesta de SpiderMonkey a cada problema

| Problema en Boa | Cómo lo resuelve SpiderMonkey | Dónde está en `js/src` |
|---|---|---|
| Copiar valores cuesta un contador | `JS::Value` son 64 bits que se copian sin más. El GC encuentra lo vivo con raíces explícitas (`Rooted`) y recorriendo la pila | `vm/`, `gc/RootMarking.cpp` |
| Llamada indirecta por instrucción | Intérprete con saltos directos entre instrucciones; después, el Baseline Interpreter generado a máquina | `vm/Interpreter.cpp`, `jit/BaselineCodeGen.cpp` |
| Propiedades por tabla hash | Shapes + **CacheIR**: cada acceso aprende el caso y genera un *stub* pequeño que comparten todas las capas | `jit/CacheIR.cpp`, `jit/BaselineIC.cpp` |
| Arreglos por el camino general | Elementos densos: un arreglo normal es un bloque contiguo con su longitud | `vm/NativeObject` |
| GC traza todo | GC generacional: los objetos nuevos van al *nursery* y solo se copia lo que sobrevive. Marcado incremental y paralelo | `gc/Nursery.cpp`, `gc/Tenuring.cpp` |
| RegExp interpretado | `irregexp` compila cada expresión a código máquina | `irregexp/` |
| Bytecode atado al GC | *Stencil*: el compilador produce bytecode sin objetos del GC, serializable y compartible entre hilos | `frontend/Stencil.cpp` |

## QUIMERA: Boa + SpiderMonkey

El nombre viene de la criatura mitológica hecha de varios animales: aquí una serpiente (Boa) con una araña y un mono (SpiderMonkey), siguiendo la tradición de Mozilla de dar nombre de animal a cada capa (TraceMonkey, IonMonkey, WarpMonkey).

**La receta**: el **frontend de Boa** (parser y AST en Rust, ya maduros: 49.917 líneas, licencia MIT/Unlicense) con el **diseño de ejecución de SpiderMonkey** (valores, GC, CacheIR, capas), escrito desde cero en Rust.

Crates propuestos (`quimera-*`), en orden de construcción. Cada uno es `no_std` + `alloc` cuando no necesita el sistema:

| Nivel | Crate | Viene de | Qué hace |
|---|---|---|---|
| 0 | `quimera-valor` | SpiderMonkey | Valor de 64 bits con NaN-boxing, `Copy`, sin contador |
| 0 | `quimera-cadenas` | Boa (`boa_string`) | Cadenas UTF-16/Latin-1 |
| 1 | `quimera-gc` | SpiderMonkey | *Nursery* con asignación por puntero + zona principal con marcado; raíces precisas |
| 2 | `quimera-frontend` | Boa (`boa_ast`, `boa_parser`, `boa_interner`) | Parser → bytecode tipo *Stencil*, sin objetos del GC |
| 3 | `quimera-objetos` | SpiderMonkey | Shapes, elementos densos, propiedades |
| 3 | `quimera-ic` | SpiderMonkey (CacheIR) | Lenguaje de cachés en línea que comparten intérprete y, más tarde, JIT |
| 4 | `quimera-vm` | ambos | Intérprete: un único bucle con `match` (para que LLVM genere la tabla de saltos y mantenga el estado en registros), registros en lugar de pila |
| 4 | `quimera-regex` | SpiderMonkey (irregexp) | Compilar cada expresión a un bytecode específico; autómata sin *backtracking* cuando se pueda |
| 5 | `quimera-builtins` | Boa | Array, Map, Promise...; se reutilizan los de Boa poco a poco |
| 6 | `quimera-embeber` | propio | La interfaz que usa tu navegador (DOM, eventos). Así el motor se puede cambiar |
| 7 | `jit` (opcional) | SpiderMonkey (Baseline) | JIT de plantilla con Cranelift, solo si hace falta |

**Metas medibles** (media geométrica V8, mismo taller):

| Paso | Meta | Referencia ya medida |
|---|---:|---|
| Boa hoy | 302 | — |
| Valores `Copy` + bucle único + elementos densos | ~1.000 | Intérprete C++ de SpiderMonkey |
| + CacheIR | ~2.200 | Baseline Interpreter de SpiderMonkey |
| + regex compiladas y *nursery* | RegExp ×10, Splay sin pausas largas | `irregexp`, `Nursery` |
| + JIT de plantilla (opcional) | 5.000–25.000 | Baseline JIT / Warp |

Las tres primeras filas no generan código máquina: el motor sigue siendo pequeño, seguro y portable a ULTRA.

## Licencias

- **Boa** es MIT o Unlicense: se puede copiar y modificar libremente, citando la licencia.
- **SpiderMonkey** es MPL-2.0. Copiar archivos obliga a mantenerlos bajo MPL. Implementar las mismas ideas en código propio no tiene esa obligación.
- **ULTRA** es GPL-2.0-only. MPL-2.0 es compatible con la GPL por su cláusula de licencias secundarias, salvo que un archivo diga lo contrario.

## Siguiente paso

Prototipo del nivel 0–1 (`quimera-valor` + `quimera-gc` con *nursery*). Con un benchmark de asignación al estilo Splay, comparar contra `boa_gc`: tiempo, memoria y pausas. Es la base de todo lo demás y la diferencia se puede medir enseguida.
