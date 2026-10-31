#!/usr/bin/bash
# Se ejecuta como ultra (UID 1000), dentro de la VM ULTRA, con su disco montado.
set -euo pipefail
[[ $(id -u) == 1000 ]] || exit 1
base=/home/ultra/Chromium
servidor=http://10.0.2.2:17865
cd "$base"
curl -fsS "$servidor/analizar-chromium-mcp.py" -o analizar-chromium-mcp.py
esperar() {
    local marca=$1
    until curl -fsS --connect-timeout 3 --max-time 8 "$servidor/$marca" -o /dev/null 2>/dev/null; do
        sleep 10
    done
}
cargar() {
    local nombre=$1 ronda=$2
    curl -fsS --connect-timeout 5 --max-time 30 "$servidor/$nombre.sha256" -o "$nombre.sha256"
    curl -f --retry 3 --connect-timeout 5 "$servidor/$nombre" -o "$nombre.part"
    # El archivo de hash se refiere al nombre final, no al temporal.
    mv "$nombre.part" "$nombre"
    sha256sum -c "$nombre.sha256"
    tar --zstd --no-same-owner -xf "$nombre"
    mkdir -p evidencias
    git -C src rev-parse HEAD > "evidencias/$ronda-commit.txt"
    git -C src ls-files | wc -l > "evidencias/$ronda-archivos-main.txt"
    python3 analizar-chromium-mcp.py /usr/local/bin/ultra-carpetas "$base/src" "$base/evidencias/$ronda"
    # La ventana ya abierta se actualiza con F5 desde el controlador QEMU.
    printf '%s\n' "$ronda terminado"
}
esperar main-transferible
cargar chromium-main.tar.zst main
touch evidencias/main-analizado
esperar fuentes-linux-transferibles
cargar chromium-linux-fuentes.tar.zst linux-con-dependencias
touch evidencias/linux-con-dependencias-analizado
