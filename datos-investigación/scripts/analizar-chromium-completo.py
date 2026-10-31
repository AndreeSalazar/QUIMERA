#!/usr/bin/env python3
"""Segunda pasada del SUPER GRAFO sobre Chromium con dependencias (gclient) incluidas.

Uso: python3 analizar-chromium-completo.py BINARIO FUENTES SALIDA-NUEVA
Lee la arquitectura (peso, archivo, camino, combinar), la seguridad (senales) y las
piezas Rust. No compila ni ejecuta Chromium. Una consulta que falla se guarda y se sigue.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

# Piezas centrales de cada capa: base -> mojo -> content -> blink/v8 -> cc/viz/gpu -> chrome.
CLAVES = [
    'base/memory/raw_ptr.h',
    'base/functional/callback.h',
    'base/task/sequenced_task_runner.h',
    'base/logging.h',
    'build/build_config.h',
    'mojo/public/cpp/bindings/remote.h',
    'ipc/ipc_message.h',
    'content/public/browser/web_contents.h',
    'content/public/browser/render_process_host.h',
    'content/public/renderer/render_frame.h',
    'third_party/blink/renderer/core/dom/document.h',
    'third_party/blink/renderer/core/frame/local_frame.h',
    'third_party/blink/renderer/platform/heap/garbage_collected.h',
    'third_party/blink/renderer/bindings/core/v8/script_value.h',
    'v8/include/v8-isolate.h',
    'v8/include/v8.h',
    'cc/trees/layer_tree_host.h',
    'components/viz/common/frame_sinks/begin_frame_source.h',
    'gpu/command_buffer/client/gles2_interface.h',
    'third_party/skia/include/core/SkCanvas.h',
    'net/url_request/url_request.h',
    'net/base/net_errors.h',
    'services/network/public/cpp/resource_request.h',
    'sandbox/linux/seccomp-bpf/sandbox_bpf.h',
    'sandbox/policy/sandbox.h',
    'ui/gfx/geometry/rect.h',
    'chrome/browser/profiles/profile.h',
    'chrome/browser/ui/browser.h',
    'url/gurl.h',
]

CAMINOS = [
    ('chrome/browser/ui/browser.h', 'content/public/browser/web_contents.h'),
    ('content/public/browser/web_contents.h', 'third_party/blink/renderer/core/dom/document.h'),
    ('third_party/blink/renderer/core/dom/document.h', 'v8/include/v8-isolate.h'),
    ('third_party/blink/renderer/core/frame/local_frame.h', 'cc/trees/layer_tree_host.h'),
    ('cc/trees/layer_tree_host.h', 'third_party/skia/include/core/SkCanvas.h'),
    ('content/public/browser/render_process_host.h', 'mojo/public/cpp/bindings/remote.h'),
    ('services/network/public/cpp/resource_request.h', 'net/url_request/url_request.h'),
    ('sandbox/policy/sandbox.h', 'base/memory/raw_ptr.h'),
]

CAPAS = ['base', 'mojo', 'ipc', 'content', 'third_party/blink', 'v8', 'cc', 'components/viz',
         'gpu', 'third_party/skia', 'net', 'services', 'sandbox', 'ui', 'chrome', 'url']

SEGURIDAD = ['sandbox', 'mojo/core', 'ipc', 'net/http', 'net/quic', 'third_party/blink/renderer/bindings',
             'v8/src/objects', 'v8/src/compiler', 'content/browser/renderer_host',
             'third_party/boringssl/src', 'third_party/ffmpeg', 'third_party/libpng',
             'third_party/libxml', 'third_party/sqlite']

RUST = ['build/rust', 'mojo/public/rust', 'third_party/rust', 'third_party/crabbyavif',
        'third_party/openh264rs', 'components/cbor', 'third_party/oak', 'chrome/utility',
        'components/web_package', 'ui/android', 'tools/crates']


def main():
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    binary, root = (Path(p).resolve(strict=True) for p in sys.argv[1:3])
    output = Path(sys.argv[3]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    # Sin cache: cada pasada mide con los detectores del binario que se le pasa.
    env = dict(os.environ, ULTRA_PRESUPUESTO_GB='32', ULTRA_PRESUPUESTO_S='3600', ULTRA_SIN_CACHE='1')
    identity = {
        'root': str(root), 'binary': str(binary), 'uid': os.getuid(),
        'binary_sha256': hashlib.file_digest(binary.open('rb'), 'sha256').hexdigest(),
        'started_unix': time.time(),
        'scope': 'Principal + dependencias Git de gclient (sin CIPD/GCS ni hooks)',
    }
    git = subprocess.run(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True, capture_output=True)
    identity['git_head'] = git.stdout.strip() if git.returncode == 0 else None
    (output / 'identidad.json').write_text(json.dumps(identity, indent=2), encoding='utf-8')
    errors = (output / 'mcp-stderr.log').open('w', encoding='utf-8')
    process = subprocess.Popen([str(binary), '--mcp', str(root)], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=errors, text=True, env=env)
    sequence = 0
    fallidas = []

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

    def tool(name, args, label):
        t0 = time.time()
        try:
            result = rpc('tools/call', {'name': name, 'arguments': args})
        except RuntimeError as exc:
            if process.poll() is not None:
                raise
            result = {'isError': True, 'error': str(exc)}
        (output / (label + '.json')).write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
        if result.get('isError'):
            fallidas.append(label)
        print(f'{label} {time.time() - t0:.1f}s {"ERROR" if result.get("isError") else "ok"}', flush=True)
        return result

    try:
        init = rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {},
                                 'clientInfo': {'name': 'Chromium personal completo', 'version': '2'}})
        (output / 'initialize.json').write_text(json.dumps(init, indent=2), encoding='utf-8')
        process.stdin.write(json.dumps({'jsonrpc': '2.0', 'method': 'notifications/initialized'}) + '\n')
        process.stdin.flush()
        tool('resumen', {}, 'resumen')
        tool('ciclos', {'limite': 50}, 'ciclos')
        tool('fallos', {'limite': 50}, 'fallos')
        for ruta in CLAVES:
            etiqueta = ruta.replace('/', '__')
            tool('peso', {'ruta': ruta, 'limite': 20}, 'peso--' + etiqueta)
            tool('archivo', {'ruta': ruta, 'limite': 30}, 'archivo--' + etiqueta)
        for i, (a, b) in enumerate(CAMINOS):
            tool('camino', {'a': a, 'b': b}, f'camino-{i:02d}')
        tool('combinar', {'archivos': CAPAS, 'limite': 50}, 'combinar-capas')
        for area in SEGURIDAD:
            tool('senales', {'archivos': [area], 'limite': 30}, 'senales--' + area.replace('/', '__'))
        for area in RUST:
            tool('senales', {'archivos': [area], 'limite': 20}, 'rust-senales--' + area.replace('/', '__'))
            tool('buscar', {'texto': area, 'limite': 20}, 'rust-buscar--' + area.replace('/', '__'))
        tool('cronologia', {'limite': 10}, 'cronologia')
        tool('exportar_sarif', {'salida': 'super-grafo-chromium-completo.sarif'}, 'exportar_sarif')
        (output / 'completado.json').write_text(json.dumps(
            {'finished_unix': time.time(), 'rpc_count': sequence, 'fallidas': fallidas}, indent=2), encoding='utf-8')
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
