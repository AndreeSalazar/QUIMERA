# QUIMERA

Navegador personal escrito en Rust, construido desde cero y por niveles, para ULTRA-LinuX. El objetivo es **consumir poco y tener control total**, no copiar Chromium entero.

Su motor JavaScript mezcla el frontend de **Boa** con el diseño de ejecución de **SpiderMonkey**. Por eso se llama QUIMERA: una criatura hecha de varios animales (una serpiente, una araña y un mono).

## Estado

| Crate | Nivel | Qué es | Estado |
|---|---:|---|---|
| `cimientos/valor` → `quimera-valor` | 0 | Valor JS de 64 bits, `Copy`, sin contador | ✅ 8 pruebas |
| `motor-js/gc` → `quimera-gc` | 1 | GC generacional: guardería en arena + zona principal, barrera de escritura | ✅ 7 pruebas |
| `bancos/gc` → `quimera-banco-gc` | 99 | Compara `quimera-gc` con `boa_gc` | ✅ |

Primera medida, con la misma carga estilo Splay (200.000 pasos, 5 millones de objetos, Windows, `--release`, tres corridas):

| | Total | Pasos de más de 1 ms | Peor pausa |
|---|---:|---:|---:|
| `quimera-gc` | 522–571 ms | 25–48 | 14–19 ms |
| `boa_gc` 0.22 | 667–729 ms | 79–81 | 10–22 ms |

QUIMERA es **1,26–1,36× más rápido** y tiene un tercio de los pasos lentos. La peor pausa aún depende de la recolección mayor, que marca todo de una vez; marcarla por partes (como SpiderMonkey) es la siguiente mejora del GC.

## Estructura

```text
QUIMERA (esta carpeta)
  Cargo.toml              workspace
  cimientos/              nivel 0: valor, cadenas, plataforma
  motor-js/               niveles 1-6: gc, frontend, objetos, ic, vm, regex, builtins, js
  web/                    html, css, dom, estilo, maquetación      (planeado)
  grafico/                gpu (wgpu), texto, pintar                (planeado)
  red/  proceso/          red, aislamiento                         (planeado)
  navegador/              navegador, ventana + interfaz VERX       (planeado)
  bancos/                 medidas contra otros motores
  herramientas/           comprobar-niveles.py (el guardián de la jerarquía)
  docs/JERARQUIA.md       todos los niveles, el orden y por qué wgpu
  datos-investigación/    la investigación archivada: Chromium, Servo, Boa, SpiderMonkey, SUPER GRAFO
```

## Comandos

```bash
cargo test
```

```bash
cargo run --release -p quimera-banco-gc
```

```bash
python -I herramientas/comprobar-niveles.py
```

## La regla

Un crate solo usa crates de niveles menores, y solo los crates de fuera que declara. Ver [docs/JERARQUIA.md](docs/JERARQUIA.md).

## Investigación

Todo lo anterior está en [datos-investigación/](datos-investigación/README.md):

- [cómo está construido Chromium](datos-investigación/docs/INVESTIGACION-CHROMIUM-COMPLETA.md) y [su jerarquía completa](datos-investigación/datos/chromium-jerarquia/LEEME.md);
- [Servo, Boa y SpiderMonkey medidos](datos-investigación/docs/INVESTIGACION-MOTORES-RUST.md);
- [el perfil de Boa y el diseño de QUIMERA](datos-investigación/docs/PERFIL-BOA-Y-MOTOR-PROPIO.md).
