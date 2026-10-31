#!/bin/sh
# Continuacion del checkout publico iniciado en el taller QEMU.
set -eu
cd /var/tmp/ultra-main-work/chromium-personal
export DEPOT_TOOLS_METRICS=0 DEPOT_TOOLS_UPDATE=0
if [ "${1:-}" != --archivo-validado ]; then
while kill -0 640 2>/dev/null; do sleep 5; done
git -C src rev-parse --verify HEAD > revision-main.txt
git -C src status --porcelain > estado-main.txt
test ! -s estado-main.txt
git -C src ls-files | wc -l > archivos-main.txt
git -C src rev-parse --is-shallow-repository > historial-superficial.txt
echo 'MAIN_LISTO'
# Archivo completo, incluido el Git superficial, para conservar revision y rutas.
tar -I 'zstd -T2 -1' -cf chromium-main.tar.zst src
fi
sha256sum chromium-main.tar.zst > chromium-main.tar.zst.sha256
touch main-transferible
echo 'MAIN_TRANSFERIBLE'
cat > .gclient <<'EOF'
solutions = [{
  'name': 'src',
  'url': 'https://github.com/chromium/chromium.git',
  'managed': False,
  'custom_deps': {},
  'custom_vars': {},
}]
target_os = ['linux']
EOF
# Solo dependencias Git: se omiten paquetes CIPD/GCS y hooks de instalacion.
# El resultado es un arbol para estudiar codigo, no un entorno compilable aprobado.
set +e
python3 depot_tools/gclient.py sync --nohooks --noprehooks --no-history --shallow --no-bootstrap --ignore-dep-type=cipd --ignore-dep-type=gcs -j 4 --output-json=dependencias-sync.json > dependencias-sync.log 2>&1
codigo=$?
set -e
printf '%s\n' "$codigo" > dependencias-sync.exit
if [ "$codigo" -ne 0 ]; then
  echo 'DEPENDENCIAS_INCOMPLETAS: consultar log'
  exit "$codigo"
fi
python3 depot_tools/gclient.py revinfo --actual > revisiones-dependencias.txt
# El principal ya se transfirio: empaquetar solo las dependencias evita repetir
# mas de medio millon de archivos. El archivo se extrae sobre el mismo src/.
python3 - <<'PY'
import json
from pathlib import Path
import subprocess
doc = json.loads(Path('dependencias-sync.json').read_text())
rutas = []
for nombre in sorted(doc['solutions'], key=lambda p: (p.count('/'), p)):
    ruta = nombre.rstrip('/')
    if ruta == 'src' or not ruta.startswith('src/') or '..' in Path(ruta).parts:
        continue
    if not Path(ruta).is_dir() or any(ruta.startswith(p + '/') for p in rutas):
        continue
    rutas.append(ruta)
Path('dependencias-paquete.nul').write_bytes(b''.join(p.encode() + b'\0' for p in rutas))
subprocess.run(['tar', '-I', 'zstd -T2 -1', '-cf', 'chromium-linux-fuentes.tar.zst', '--null', '-T', 'dependencias-paquete.nul'], check=True)
PY
sha256sum chromium-linux-fuentes.tar.zst > chromium-linux-fuentes.tar.zst.sha256
touch fuentes-linux-transferibles
echo 'FUENTES_LINUX_TRANSFERIBLES'
