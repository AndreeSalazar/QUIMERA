#!/usr/bin/env python3
"""Consulta el SUPER GRAFO real sobre una raiz completa y conserva evidencia.

Uso: python3 analizar-chromium-mcp.py BINARIO FUENTES SALIDA-NUEVA
No compila ni ejecuta programas de Chromium. Ejecutar como usuario del escritorio.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    binary, root = (Path(p).resolve(strict=True) for p in sys.argv[1:3])
    output = Path(sys.argv[3]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, ULTRA_PRESUPUESTO_GB='32', ULTRA_PRESUPUESTO_S='1800')
    identity = {
        'root': str(root), 'binary': str(binary), 'uid': os.getuid(),
        'binary_sha256': hashlib.file_digest(binary.open('rb'), 'sha256').hexdigest(),
        'started_unix': time.time(),
        'scope': 'Archivos presentes; excludes y limites del motor SUPER GRAFO aplican',
    }
    git = subprocess.run(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True, capture_output=True)
    identity['git_head'] = git.stdout.strip() if git.returncode == 0 else None
    (output / 'identidad.json').write_text(json.dumps(identity, indent=2), encoding='utf-8')
    errors = (output / 'mcp-stderr.log').open('w', encoding='utf-8')
    process = subprocess.Popen([str(binary), '--mcp', str(root)], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=errors, text=True, env=env)
    sequence = 0

    def rpc(method, params):
        nonlocal sequence
        sequence += 1
        process.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': sequence, 'method': method, 'params': params}) + '\n')
        process.stdin.flush()
        for line in process.stdout:
            message = json.loads(line)
            if message.get('id') == sequence:
                if 'error' in message:
                    raise RuntimeError(message['error'])
                return message['result']
        raise RuntimeError(f'El servidor termino: {process.poll()}')

    def tool(name, args, label=None):
        result = rpc('tools/call', {'name': name, 'arguments': args})
        (output / ((label or name) + '.json')).write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
        if result.get('isError'):
            raise RuntimeError(result)
        return result

    try:
        init = rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                 'clientInfo': {'name': 'Chromium personal', 'version': '1'}})
        (output / 'initialize.json').write_text(json.dumps(init, indent=2), encoding='utf-8')
        process.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}) + '\n')
        process.stdin.flush()
        catalog = rpc('tools/list', {})
        (output / 'herramientas.json').write_text(json.dumps(catalog, indent=2, ensure_ascii=False), encoding='utf-8')
        for name in ('resumen', 'ciclos', 'fallos', 'guardianes'):
            tool(name, {'limite': 20})
            print(name, 'guardado', flush=True)
        for area in ('base', 'content', 'chrome', 'components', 'net', 'third_party/blink', 'v8', 'gpu', 'sandbox'):
            tool('buscar', {'texto': area, 'limite': 10}, 'buscar-' + area.replace('/', '-'))
        tool('exportar_sarif', {'salida': 'super-grafo-chromium.sarif'})
        # Estado real del puente; no se afirma que haya ventana si no la hay.
        tool('ventana', {'limite': 10})
        (output / 'completado.json').write_text(json.dumps({'finished_unix': time.time(), 'rpc_count': sequence}), encoding='utf-8')
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        errors.close()


if __name__ == '__main__':
    main()
