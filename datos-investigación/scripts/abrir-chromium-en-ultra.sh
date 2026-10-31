#!/bin/sh
# Ejecutar en la consola exterior de ULTRA, como root, despues de arrancar la VM.
# No formatea nada. Solo acepta el disco exclusivo creado para este trabajo.
set -eu
disco=
for entrada in /sys/block/vd*; do
    [ "$(cat "$entrada/serial" 2>/dev/null)" = CHROMIUM-SOURCE ] || continue
    [ -z "$disco" ] || { echo 'Mas de un disco CHROMIUM-SOURCE' >&2; exit 1; }
    disco=/dev/${entrada##*/}
done
[ -n "$disco" ] || { echo 'Falta disco CHROMIUM-SOURCE' >&2; exit 1; }
mkdir -p /arch/home/ultra/Chromium
if ! grep -q ' /arch/home/ultra/Chromium ' /proc/mounts; then
    mount -t ext4 "$disco" /arch/home/ultra/Chromium
fi
[ -d /arch/home/ultra/Chromium/src ] || { echo 'Fuentes pendientes' >&2; exit 1; }
ultra-escritorio --hyprland --software > /tmp/hyprland-chromium.log 2>&1 &
echo 'Escritorio iniciado. Abrir ULTRA Carpetas en /home/ultra/Chromium/src y elegir Ciudad.'
