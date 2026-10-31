#!/usr/bin/env python3
"""Mide motores de JavaScript con los mismos scripts: tiempo, memoria maxima y puntuacion V8.

Uso: python3 -I medir-motores-js.py CARPETA-SCRIPTS SALIDA.json motor=comando [motor=comando ...]
  comando: binario y opciones separadas por espacios; el script se anade al final.
  Ej.: boa=/ruta/boa  sm=js140  sm-interp="js140 --no-blinterp --no-baseline --no-ion"

Cada script de la carpeta define main(); aqui se le anade la llamada y un `print` comun.
Repite 3 veces (1 en v8-benches) y guarda la mediana del tiempo de pared y el RSS maximo.
"""
import json
import os
from pathlib import Path
import shlex
import statistics
import subprocess
import sys
import tempfile
import time

PREAMBULO = "if (typeof print === 'undefined') { globalThis.print = (...a) => console.log(...a); }\n"
LIMITE_S = 900


def ejecutar(cmd, script):
    t0 = time.perf_counter()
    p = subprocess.Popen(cmd + [script], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        salida, _ = p.communicate(timeout=LIMITE_S)
    except subprocess.TimeoutExpired:
        p.kill()
        salida, _ = p.communicate()
        return {'ok': False, 'segundos': LIMITE_S, 'error': 'limite de tiempo'}
    seg = time.perf_counter() - t0
    return {'ok': p.returncode == 0, 'segundos': seg, 'codigo': p.returncode, 'salida': salida[-2000:]}


def rss_kib(cmd, script):
    """Memoria maxima (KiB) de una corrida aparte: wait4 da ru_maxrss del proceso exacto."""
    pid = os.fork()
    if pid == 0:
        fd = os.open(os.devnull, os.O_WRONLY)
        os.dup2(fd, 1); os.dup2(fd, 2)
        try:
            os.execvp(cmd[0], cmd + [script])
        finally:
            os._exit(127)
    _, estado, uso = os.wait4(pid, 0)
    return uso.ru_maxrss


def main():
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    carpeta, salida = Path(sys.argv[1]), Path(sys.argv[2])
    motores = {}
    for a in sys.argv[3:]:
        k, v = a.split('=', 1)
        motores[k] = shlex.split(v)
    scripts = sorted(p for p in carpeta.rglob('*.js'))
    resultados = {'motores': {k: ' '.join(v) for k, v in motores.items()}, 'scripts': {}}
    with tempfile.TemporaryDirectory() as tmp:
        for s in scripts:
            nombre = str(s.relative_to(carpeta))
            envuelto = Path(tmp) / (nombre.replace('/', '__'))
            envuelto.write_text(PREAMBULO + s.read_text(encoding='utf-8') + '\nmain();\n', encoding='utf-8')
            veces = 1 if 'v8-benches' in nombre else 3
            fila = {}
            for m, cmd in motores.items():
                corridas = [ejecutar(cmd, str(envuelto)) for _ in range(veces)]
                ok = all(c['ok'] for c in corridas)
                r = {'ok': ok, 'segundos': round(statistics.median(c['segundos'] for c in corridas), 4)}
                if ok:
                    r['rss_kib'] = rss_kib(cmd, str(envuelto)) if veces > 1 or r['segundos'] < 120 else None
                    puntos = [l.split()[-1] for l in corridas[-1]['salida'].splitlines() if l.startswith('SCORE')]
                    if puntos:
                        r['puntuacion'] = puntos[-1]
                else:
                    r['error'] = (corridas[-1].get('error') or corridas[-1].get('salida', ''))[-400:]
                fila[m] = r
                print(f"{nombre:42} {m:12} {'ok ' if ok else 'ERR'} {r['segundos']:9.3f}s {r.get('rss_kib') or '':>8} {r.get('puntuacion', '')}", flush=True)
            resultados['scripts'][nombre] = fila
            salida.write_text(json.dumps(resultados, indent=1, ensure_ascii=False), encoding='utf-8')


if __name__ == '__main__':
    main()
