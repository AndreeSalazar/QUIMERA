#!/bin/sh
# Perfila Boa con perf sobre benchmarks V8: donde se va el tiempo, por funcion y por crate.
# Uso: sh perfilar-boa.sh BINARIO-BOA CARPETA-BENCHES SALIDA-NUEVA [benchmark ...]
# Necesita Boa compilado con simbolos y frame pointers (RUSTFLAGS=-Cforce-frame-pointers=yes).
set -eu
# PERF="sudo -n perf" si el kernel no deja perfilar al usuario (perf_event_paranoid).
PERF=${PERF:-perf}
[ "$#" -ge 3 ] || { echo 'Uso: perfilar-boa.sh BINARIO CARPETA-BENCHES SALIDA [benchmark ...]' >&2; exit 2; }
boa=$(realpath "$1"); benches=$(realpath "$2"); mkdir "$3"; salida=$(realpath "$3"); shift 3
[ "$#" -gt 0 ] || set -- crypto earley-boyer richards regexp raytrace splay
for b in "$@"; do
    js="$salida/$b.js"
    printf '%s\n' "if (typeof print === 'undefined') { globalThis.print = (...a) => console.log(...a); }" > "$js"
    cat "$benches/v8-benches/$b.js" >> "$js"
    printf '\nmain();\n' >> "$js"
    $PERF record -q -e cpu-clock -F 997 --call-graph fp -o "$salida/$b.data" "$boa" "$js" > "$salida/$b.salida.txt" 2>&1
    # Tiempo propio de cada funcion (sin contar lo que llama).
    $PERF report -q -i "$salida/$b.data" --stdio --no-children --sort symbol --percent-limit 0.7 2>/dev/null \
        | grep -E '^ +[0-9]' > "$salida/$b.funciones.txt" || true
    # Tiempo por crate/modulo: agrupa los simbolos por su ruta Rust (boa_engine::vm::opcode::...).
    $PERF report -q -i "$salida/$b.data" --stdio --no-children --sort symbol 2>/dev/null \
        | awk '/^ +[0-9]/ { p=$1; sub("%","",p); $1=""; s=$0; gsub(/^ +\[\.\] +/,"",s);
                 n=split(s, parte, "::"); clave=(n>=3 ? parte[1]"::"parte[2]"::"parte[3] : s);
                 suma[clave]+=p } END { for (k in suma) printf "%6.2f%% %s\n", suma[k], k }' \
        | sort -rn | head -40 > "$salida/$b.modulos.txt"
    rm -f "$salida/$b.data" 2>/dev/null || sudo -n rm -f "$salida/$b.data"
    echo "$b listo"
done
