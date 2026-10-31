#!/bin/sh
# Ejecutar en Linux con una copia de este directorio y un binario SUPER GRAFO actual.
# Uso: sh scripts/consultar-super-grafo.sh /ruta/ultra-carpetas /ruta/referencia/ULTRA-LinuX /ruta/salida-nueva
set -eu
[ "$#" -eq 3 ] || { echo 'Uso: script BINARIO REFERENCIA SALIDA-NUEVA' >&2; exit 2; }
bin=$(realpath "$1")
raiz=$(realpath "$2")
[ -x "$bin" ] || { echo 'Binario no ejecutable' >&2; exit 2; }
git -C "$raiz" rev-parse --verify HEAD >/dev/null
# Una salida existente se rechaza para conservar rondas anteriores.
mkdir "$3"
salida=$(realpath "$3")
git -C "$raiz" rev-parse HEAD > "$salida/commit.txt"
git -C "$raiz" status --porcelain=v1 > "$salida/estado-git.txt"
sha256sum "$bin" > "$salida/binario.sha256"
consulta() {
    nombre=$1
    herramienta=$2
    argumentos=$3
    "$bin" --consulta "$herramienta" "$raiz" "$argumentos" > "$salida/$nombre.json.tmp"
    mv "$salida/$nombre.json.tmp" "$salida/$nombre.json"
}
consulta resumen resumen '{}'
consulta ciclos ciclos '{"limite":10}'
consulta fallos fallos '{"limite":10}'
consulta cronologia cronologia '{"periodos":10,"profundidad":2,"limite":10}'
consulta nucleo archivo '{"ruta":"src/apps/carpetas-nucleo/src/lib.rs","limite":10}'
consulta consultas archivo '{"ruta":"src/apps/ultra-carpetas/src/consultas.rs","limite":10}'
echo 'Consultas completadas. No equivalen a pruebas de ejecucion del navegador.'
