#!/usr/bin/env python3
"""Pasa el SUPER GRAFO (ultra-carpetas --mcp) por cualquier proyecto y guarda cada respuesta.

Uso: python3 -I analizar-proyecto-super-grafo.py BINARIO RAIZ SALIDA-NUEVA [area ...]
  area: carpetas relativas a RAIZ para `senales` y `combinar` (por defecto, las de primer nivel).
Sin cache: cada pasada mide con los detectores del binario dado. No compila ni ejecuta el proyecto.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    binario, raiz = (Path(p).resolve(strict=True) for p in sys.argv[1:3])
    salida = Path(sys.argv[3]).resolve()
    salida.mkdir(parents=True, exist_ok=False)
    areas = sys.argv[4:] or sorted(p.name for p in raiz.iterdir() if p.is_dir() and not p.name.startswith(('.', 'target')))[:64]
    git = subprocess.run(['git', '-C', str(raiz), 'rev-parse', 'HEAD'], text=True, capture_output=True)
    (salida / 'identidad.json').write_text(json.dumps({
        'raiz': str(raiz), 'binario': str(binario), 'git_head': git.stdout.strip() or None,
        'binario_sha256': hashlib.file_digest(binario.open('rb'), 'sha256').hexdigest(),
        'inicio_unix': time.time(), 'areas': areas}, indent=2), encoding='utf-8')
    env = dict(os.environ, ULTRA_PRESUPUESTO_GB='32', ULTRA_PRESUPUESTO_S='3600', ULTRA_SIN_CACHE='1')
    errores = (salida / 'mcp-stderr.log').open('w', encoding='utf-8')
    proc = subprocess.Popen([str(binario), '--mcp', str(raiz)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=errores, text=True, env=env)
    n = 0
    fallidas = []

    def rpc(metodo, params):
        nonlocal n
        n += 1
        proc.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': n, 'method': metodo, 'params': params}) + '\n')
        proc.stdin.flush()
        for linea in proc.stdout:
            m = json.loads(linea)
            if m.get('id') == n:
                if 'error' in m:
                    raise RuntimeError(m['error'])
                return m['result']
        raise RuntimeError(f'el servidor termino: {proc.poll()}')

    def herramienta(nombre, args, etiqueta):
        t0 = time.time()
        try:
            r = rpc('tools/call', {'name': nombre, 'arguments': args})
        except RuntimeError as e:
            if proc.poll() is not None:
                raise
            r = {'isError': True, 'error': str(e)}
        (salida / f'{etiqueta}.json').write_text(json.dumps(r, indent=2, ensure_ascii=False), encoding='utf-8')
        if r.get('isError'):
            fallidas.append(etiqueta)
        print(f'{etiqueta} {time.time() - t0:.1f}s {"ERROR" if r.get("isError") else "ok"}', flush=True)
        return r

    try:
        rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {}, 'clientInfo': {'name': 'Chromium personal', 'version': '3'}})
        proc.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}) + '\n')
        proc.stdin.flush()
        herramienta('resumen', {}, 'resumen')
        herramienta('ciclos', {'limite': 30}, 'ciclos')
        herramienta('fallos', {'limite': 30}, 'fallos')
        herramienta('combinar', {'archivos': areas[:64], 'limite': 60}, 'combinar-areas')
        for a in areas[:64]:
            herramienta('senales', {'archivos': [a], 'limite': 25}, 'senales--' + a.replace('/', '__'))
        herramienta('cronologia', {'periodos': 12, 'limite': 15}, 'cronologia')
        herramienta('exportar_sarif', {'salida': 'super-grafo.sarif'}, 'exportar_sarif')
        (salida / 'completado.json').write_text(json.dumps({'fin_unix': time.time(), 'rpc': n, 'fallidas': fallidas}, indent=2), encoding='utf-8')
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
        errores.close()


if __name__ == '__main__':
    main()
