#!/usr/bin/env python3
"""Archiva la jerarquia completa de Chromium como datos para construirlo pieza a pieza.

Uso: python3 -I archivar-jerarquia-chromium.py CHECKOUT SALIDA-NUEVA
  CHECKOUT: carpeta que contiene src/ y dependencias-sync.json (gclient).

Escribe (sin tocar el checkout):
  archivos.tsv.gz         cada archivo: ruta, bytes, extension, lenguaje, componente
  carpetas.tsv.gz         cada carpeta: totales, lenguajes, BUILD.gn/DEPS/OWNERS, origen gclient
  componentes.json        cada componente: tamano, lenguajes, objetivos GN, licencia, origen
  gn-objetivos.tsv.gz     cada objetivo de cada BUILD.gn: etiqueta, plantilla, archivo
  gn-dependencias.tsv.gz  objetivo -> dependencia (deps y public_deps, sin evaluar condiciones)
  dependencias-gclient.tsv  ruta, url, revision
  aislamiento.json        grafo entre componentes, ciclos (piezas inseparables) y niveles
No compila ni ejecuta nada de Chromium. El grafo GN es lexico: incluye las ramas de
todos los `if`, asi que sobreestima las dependencias de una configuracion concreta.
"""
import collections
import gzip
import json
import os
from pathlib import Path
import re
import sys
import time

LENGUAJES = {
    'C': {'.c'}, 'C/C++ cabecera': {'.h'},
    'C++': {'.cc', '.cpp', '.cxx', '.hh', '.hpp', '.hxx', '.inc', '.inl', '.ipp'},
    'Objective-C': {'.m', '.mm'}, 'ASM': {'.s', '.asm', '.S'}, 'Rust': {'.rs'},
    'Python': {'.py'}, 'JavaScript': {'.js', '.mjs', '.cjs'}, 'TypeScript': {'.ts', '.tsx'},
    'HTML/CSS': {'.html', '.css', '.htm'}, 'Java': {'.java'}, 'Kotlin': {'.kt'}, 'Swift': {'.swift'},
    'Go': {'.go'}, 'Shell': {'.sh'}, 'GN': {'.gn', '.gni'}, 'Mojom': {'.mojom'}, 'IDL': {'.idl', '.webidl', '.pidl'},
    'Proto': {'.proto'}, 'JSON': {'.json', '.json5'}, 'XML': {'.xml', '.grd', '.grdp', '.xtb'},
    'WGSL/GLSL': {'.wgsl', '.glsl', '.vert', '.frag', '.comp', '.hlsl', '.metal'},
    'Texto': {'.md', '.txt', '.rst'}, 'Imagen': {'.png', '.svg', '.jpg', '.jpeg', '.gif', '.webp', '.ico', '.avif'},
}
POR_EXT = {e: l for l, es in LENGUAJES.items() for e in es}
CODIGO = {'C', 'C/C++ cabecera', 'C++', 'Objective-C', 'ASM', 'Rust', 'Python', 'JavaScript', 'TypeScript', 'Java', 'Kotlin', 'Swift', 'Go'}
# Carpetas paraguas: su componente es el segundo nivel (components/autofill, third_party/skia).
PARAGUAS = {'components', 'services', 'third_party', 'chrome', 'content', 'ui', 'ash', 'chromeos', 'extensions',
            'tools', 'android_webview', 'net', 'media', 'gpu', 'base', 'testing', 'build', 'mojo', 'device'}
# Capa de cada componente segun el analisis del SUPER GRAFO (de abajo arriba).
CAPAS = [
    ('0-cimientos', ('build', 'buildtools', 'base', 'testing', 'third_party/abseil-cpp', 'third_party/googletest', 'third_party/libc++',
                     'third_party/libc++abi', 'third_party/partition_alloc', 'third_party/rust', 'third_party/zlib', 'third_party/icu')),
    ('1-ipc', ('mojo', 'ipc')),
    ('2-red-servicios', ('net', 'url', 'services', 'third_party/boringssl', 'third_party/quiche', 'third_party/sqlite', 'storage', 'crypto', 'sql')),
    ('3-motores', ('v8', 'third_party/skia', 'third_party/angle', 'third_party/dawn', 'third_party/ffmpeg', 'third_party/harfbuzz-ng',
                   'third_party/freetype', 'media', 'gpu', 'skia')),
    ('4-composicion', ('cc', 'components/viz', 'ui')),
    ('5-motor-web', ('third_party/blink',)),
    ('6-multiproceso', ('content', 'sandbox')),
    ('7-producto', ('chrome', 'components', 'extensions', 'ash', 'chromeos', 'android_webview', 'headless', 'fuchsia_web', 'ios', 'weblayer')),
]
TIPOS_GN = ('component', 'source_set', 'static_library', 'shared_library', 'executable', 'group', 'test', 'loadable_module',
            'rust_static_library', 'rust_executable', 'rust_shared_library', 'rust_proc_macro', 'cargo_crate', 'rust_bindgen',
            'mojom', 'mojom_component', 'jumbo_component', 'jumbo_source_set', 'jumbo_static_library', 'android_library',
            'java_library', 'proto_library', 'fuzzer_test', 'action', 'action_foreach', 'copy', 'generate_jni', 'idl_compiler',
            'protobuf_library', 'cxx_library', 'blink_platform_sources', 'blink_core_sources', 'blink_modules_sources',
            'v8_source_set', 'v8_component', 'v8_static_library', 'v8_executable', 'skia_component', 'angle_static_library',
            'angle_source_set', 'dawn_component', 'dawn_source_set', 'webrtc_source_set', 'rtc_library', 'rtc_source_set',
            'grit', 'mojo_bindings', 'pdfium_source_set', 'chrome_strings', 'test_support')
RE_OBJETIVO = re.compile(r'^[ \t]*([A-Za-z_][A-Za-z0-9_]*)\(\s*"([^"$]+)"\s*\)\s*\{', re.M)
RE_DEPS = re.compile(r'\b(public_deps|deps)\s*(\+?=)\s*\[', re.M)
RE_CADENA = re.compile(r'"([^"]*)"')


def componente(ruta):
    if ruta.startswith('/'):
        return '(raiz)'
    p = ruta.split('/')
    if len(p) > 1 and p[0] in PARAGUAS and len(p) > 2:
        return p[0] + '/' + p[1]
    return p[0] if len(p) > 1 else '(raiz)'


def capa(comp):
    for nombre, prefijos in CAPAS:
        if any(comp == x or comp.startswith(x + '/') for x in prefijos):
            return nombre
    return '7-producto' if comp.split('/')[0] in ('chrome', 'components') else 'otros'


def lenguaje(nombre):
    _, ext = os.path.splitext(nombre)
    return POR_EXT.get(ext, POR_EXT.get(ext.lower(), 'otros'))


def cierre(texto, abre):
    """Indice del ']' o '}' que cierra el que abre en `abre`, saltando cadenas y comentarios."""
    nivel, i, n = 0, abre, len(texto)
    while i < n:
        c = texto[i]
        if c == '"':
            i += 1
            while i < n and texto[i] != '"':
                i += 2 if texto[i] == '\\' else 1
        elif c == '#':
            while i < n and texto[i] != '\n':
                i += 1
        elif c in '[{(':
            nivel += 1
        elif c in ']})':
            nivel -= 1
            if nivel == 0:
                return i
        i += 1
    return n


def etiqueta(dep, dir_gn):
    dep = dep.split('(')[0].strip()
    if not dep or '$' in dep:
        return None
    if dep.startswith(':'):
        return f'//{dir_gn}{dep}'
    if dep.startswith('//'):
        return dep if ':' in dep else f'{dep}:{dep.rstrip("/").rsplit("/", 1)[-1]}'
    # Relativa a la carpeta del BUILD.gn ("sub:x" o "sub").
    base = f'//{dir_gn}/{dep}' if dir_gn else f'//{dep}'
    return base if ':' in dep else f'{base}:{dep.rstrip("/").rsplit("/", 1)[-1]}'


def readme_chromium(carpeta):
    f = carpeta / 'README.chromium'
    if not f.is_file():
        return None
    datos = {}
    for linea in f.read_text(encoding='utf-8', errors='replace').splitlines()[:40]:
        if ':' in linea:
            k, v = linea.split(':', 1)
            if k.strip() in ('Name', 'Short Name', 'URL', 'Version', 'Revision', 'License', 'Security Critical', 'Shipped', 'Update Mechanism'):
                datos[k.strip()] = v.strip()
    return datos or None


def tarjan(nodos, aristas):
    """Componentes fuertemente conexos (iterativo)."""
    indice, bajo, en_pila, pila, sccs, contador = {}, {}, set(), [], [], [0]
    for raiz in nodos:
        if raiz in indice:
            continue
        trabajo = [(raiz, iter(sorted(aristas.get(raiz, ()))))]
        indice[raiz] = bajo[raiz] = contador[0]; contador[0] += 1
        pila.append(raiz); en_pila.add(raiz)
        while trabajo:
            v, hijos = trabajo[-1]
            avanzo = False
            for w in hijos:
                if w not in indice:
                    indice[w] = bajo[w] = contador[0]; contador[0] += 1
                    pila.append(w); en_pila.add(w)
                    trabajo.append((w, iter(sorted(aristas.get(w, ())))))
                    avanzo = True
                    break
                if w in en_pila:
                    bajo[v] = min(bajo[v], indice[w])
            if avanzo:
                continue
            trabajo.pop()
            if trabajo:
                bajo[trabajo[-1][0]] = min(bajo[trabajo[-1][0]], bajo[v])
            if bajo[v] == indice[v]:
                scc = []
                while True:
                    w = pila.pop(); en_pila.discard(w); scc.append(w)
                    if w == v:
                        break
                sccs.append(sorted(scc))
    return sccs


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    checkout = Path(sys.argv[1]).resolve(strict=True)
    src = checkout / 'src'
    salida = Path(sys.argv[2]).resolve()
    salida.mkdir(parents=True, exist_ok=False)
    t0 = time.time()

    # --- gclient: que carpetas son repositorios aparte, con su url y revision ---
    gclient = {}
    sync = checkout / 'dependencias-sync.json'
    if sync.is_file():
        for nombre, d in json.loads(sync.read_text()).get('solutions', {}).items():
            ruta = nombre.rstrip('/')
            if ruta == 'src' or not ruta.startswith('src/'):
                continue
            url = d.get('url') or ''
            repo, _, rev = url.partition('@')
            gclient[ruta[4:]] = {'url': repo, 'revision': rev or d.get('revision'), 'presente': (src / ruta[4:]).is_dir()}
    with open(salida / 'dependencias-gclient.tsv', 'w', encoding='utf-8') as f:
        f.write('ruta\turl\trevision\tpresente\n')
        for r in sorted(gclient):
            g = gclient[r]
            f.write(f"{r}\t{g['url']}\t{g['revision'] or ''}\t{int(g['presente'])}\n")
    raices_gclient = sorted(gclient, key=len, reverse=True)

    def origen(ruta):
        for r in raices_gclient:
            if ruta == r or ruta.startswith(r + '/'):
                return r
        return ''

    # --- recorrido completo ---
    carpetas = {}   # ruta -> dict
    comp = collections.defaultdict(lambda: {'archivos': 0, 'bytes': 0, 'lenguajes': collections.Counter(), 'build_gn': 0})
    gn_archivos = []
    n_archivos = 0
    with gzip.open(salida / 'archivos.tsv.gz', 'wt', encoding='utf-8', compresslevel=6) as fa:
        fa.write('ruta\tbytes\textension\tlenguaje\tcomponente\n')
        for raiz, dirs, archivos in os.walk(src):
            dirs[:] = sorted(d for d in dirs if d != '.git')
            rel = os.path.relpath(raiz, src).replace(os.sep, '/')
            rel = '' if rel == '.' else rel
            c = {'archivos': 0, 'bytes': 0, 'lenguajes': collections.Counter(), 'subcarpetas': len(dirs),
                 'build_gn': 'BUILD.gn' in archivos, 'deps': 'DEPS' in archivos, 'owners': 'OWNERS' in archivos,
                 'readme_chromium': 'README.chromium' in archivos}
            for a in sorted(archivos):
                p = os.path.join(raiz, a)
                try:
                    st = os.lstat(p)
                except OSError:
                    continue
                if not os.path.isfile(p) or os.path.islink(p):
                    continue
                ruta = f'{rel}/{a}' if rel else a
                lg = lenguaje(a)
                cp = componente(ruta)
                fa.write(f'{ruta}\t{st.st_size}\t{os.path.splitext(a)[1]}\t{lg}\t{cp}\n')
                c['archivos'] += 1; c['bytes'] += st.st_size; c['lenguajes'][lg] += 1
                k = comp[cp]; k['archivos'] += 1; k['bytes'] += st.st_size; k['lenguajes'][lg] += 1
                if a == 'BUILD.gn':
                    gn_archivos.append(rel); k['build_gn'] += 1
                n_archivos += 1
            carpetas[rel] = c
    print(f'{n_archivos} archivos, {len(carpetas)} carpetas en {time.time() - t0:.0f}s', flush=True)

    # Totales acumulados de abajo arriba.
    total = {r: [c['archivos'], c['bytes'], collections.Counter(c['lenguajes'])] for r, c in carpetas.items()}
    for r in sorted(carpetas, key=lambda x: x.count('/'), reverse=True):
        if not r:
            continue
        padre = r.rsplit('/', 1)[0] if '/' in r else ''
        if padre in total:
            t, tp = total[r], total[padre]
            tp[0] += t[0]; tp[1] += t[1]; tp[2].update(t[2])
    cols = ['C', 'C/C++ cabecera', 'C++', 'Rust', 'Python', 'JavaScript', 'TypeScript', 'Java', 'Objective-C', 'ASM', 'GN', 'Mojom', 'IDL', 'Proto']
    with gzip.open(salida / 'carpetas.tsv.gz', 'wt', encoding='utf-8', compresslevel=6) as fc:
        fc.write('ruta\tprofundidad\tcomponente\tcapa\torigen_gclient\tsubcarpetas\tarchivos_directos\tarchivos_total\tbytes_total\t'
                 'lenguaje_principal\tbuild_gn\tdeps\towners\treadme_chromium\t' + '\t'.join(cols) + '\n')
        for r in sorted(carpetas):
            c, t = carpetas[r], total[r]
            cod = {k: v for k, v in t[2].items() if k in CODIGO}
            principal = max(cod, key=cod.get) if cod else (max(t[2], key=t[2].get) if t[2] else '')
            cp = componente(r + '/x') if r else '(raiz)'
            fc.write('\t'.join(map(str, [r or '.', r.count('/') + 1 if r else 0, cp, capa(cp), origen(r), c['subcarpetas'],
                                        c['archivos'], t[0], t[1], principal, int(c['build_gn']), int(c['deps']), int(c['owners']),
                                        int(c['readme_chromium'])] + [t[2].get(k, 0) for k in cols])) + '\n')

    # --- objetivos y dependencias GN ---
    objetivos, aristas_gn = [], []
    for rel in gn_archivos:
        texto = (src / rel / 'BUILD.gn').read_text(encoding='utf-8', errors='replace')
        for m in RE_OBJETIVO.finditer(texto):
            plantilla, nombre = m.group(1), m.group(2)
            if plantilla in ('if', 'foreach', 'template', 'declare_args', 'import', 'assert', 'config', 'forward_variables_from'):
                continue
            etq = f'//{rel}:{nombre}'
            objetivos.append((etq, plantilla, f'{rel}/BUILD.gn' if rel else 'BUILD.gn'))
            fin = cierre(texto, m.end() - 1)
            cuerpo = texto[m.end():fin]
            for d in RE_DEPS.finditer(cuerpo):
                lista = cuerpo[d.end() - 1:cierre(cuerpo, d.end() - 1)]
                for s in RE_CADENA.findall(lista):
                    e = etiqueta(s, rel)
                    if e:
                        aristas_gn.append((etq, e, d.group(1)))
    with gzip.open(salida / 'gn-objetivos.tsv.gz', 'wt', encoding='utf-8') as f:
        f.write('etiqueta\tplantilla\tarchivo\tcomponente\n')
        for etq, pl, ar in objetivos:
            f.write(f'{etq}\t{pl}\t{ar}\t{componente(etq[2:].split(":")[0] + "/x")}\n')
    with gzip.open(salida / 'gn-dependencias.tsv.gz', 'wt', encoding='utf-8') as f:
        f.write('objetivo\tdependencia\ttipo\n')
        for a, b, t in aristas_gn:
            f.write(f'{a}\t{b}\t{t}\n')
    print(f'{len(objetivos)} objetivos GN, {len(aristas_gn)} dependencias GN', flush=True)

    # --- componentes ---
    plantillas = collections.defaultdict(collections.Counter)
    for etq, pl, _ in objetivos:
        plantillas[componente(etq[2:].split(':')[0] + '/x')][pl] += 1
    comp_de = lambda etq: componente(etq[2:].split(':')[0] + '/x')
    entre = collections.Counter()
    for a, b, _ in aristas_gn:
        ca, cb = comp_de(a), comp_de(b)
        if ca != cb:
            entre[(ca, cb)] += 1
    usa = collections.defaultdict(dict); usado = collections.defaultdict(dict)
    for (a, b), n in entre.items():
        usa[a][b] = n; usado[b][a] = n
    componentes = {}
    for cp in sorted(comp):
        k = comp[cp]
        cod = {l: n for l, n in k['lenguajes'].items() if l in CODIGO}
        componentes[cp] = {
            'capa': capa(cp), 'archivos': k['archivos'], 'bytes': k['bytes'], 'build_gn': k['build_gn'],
            'lenguajes': dict(k['lenguajes'].most_common()), 'lenguaje_principal': max(cod, key=cod.get) if cod else None,
            'rust': k['lenguajes'].get('Rust', 0), 'objetivos_gn': dict(plantillas[cp].most_common()),
            'origen_gclient': gclient.get(cp) or next(({'ruta': r, **gclient[r]} for r in raices_gclient if cp.startswith(r + '/')), None),
            'readme_chromium': readme_chromium(src / cp) if (src / cp).is_dir() else None,
            'usa': dict(sorted(usa[cp].items(), key=lambda x: -x[1])), 'lo_usan': dict(sorted(usado[cp].items(), key=lambda x: -x[1])),
        }
    (salida / 'componentes.json').write_text(json.dumps(componentes, indent=1, ensure_ascii=False), encoding='utf-8')

    # --- aislamiento: piezas inseparables (ciclos) y niveles de construccion ---
    nodos = sorted(componentes)
    aristas = {a: set(b for b in usa[a] if b in componentes) for a in nodos}
    sccs = tarjan(nodos, aristas)
    pieza = {c: i for i, s in enumerate(sccs) for c in s}
    # Tarjan devuelve las piezas en orden topologico inverso: las dependencias salen antes.
    nivel = {}
    for i, s in enumerate(sccs):
        deps = {pieza[b] for c in s for b in aristas[c] if pieza[b] != i}
        nivel[i] = 1 + max((nivel[d] for d in deps), default=-1)
    piezas = [{'nivel': nivel[i], 'componentes': s, 'archivos': sum(componentes[c]['archivos'] for c in s),
               'depende_de': sorted({'/'.join(sccs[pieza[b]][:1]) for c in s for b in aristas[c] if pieza[b] != i})}
              for i, s in enumerate(sccs)]
    piezas.sort(key=lambda p: (p['nivel'], -p['archivos']))
    ciclos = [p for p in piezas if len(p['componentes']) > 1]
    aisl = {
        'nota': 'Grafo entre componentes segun deps/public_deps de los BUILD.gn (todas las ramas de los if). '
                'Una pieza con varios componentes es un ciclo: no se puede construir ni sustituir uno sin los otros. '
                'nivel 0 no depende de nadie; cada nivel solo usa niveles menores.',
        'componentes': len(nodos), 'aristas_entre_componentes': sum(len(v) for v in aristas.values()),
        'piezas': len(piezas), 'ciclos': len(ciclos), 'niveles': 1 + max(nivel.values(), default=-1),
        'mayor_ciclo': max((len(p['componentes']) for p in ciclos), default=0),
        'lista': piezas,
    }
    (salida / 'aislamiento.json').write_text(json.dumps(aisl, indent=1, ensure_ascii=False), encoding='utf-8')
    resumen = {'archivos': n_archivos, 'carpetas': len(carpetas), 'componentes': len(nodos), 'build_gn': len(gn_archivos),
               'objetivos_gn': len(objetivos), 'dependencias_gn': len(aristas_gn), 'dependencias_gclient': len(gclient),
               'piezas': len(piezas), 'ciclos': len(ciclos), 'mayor_ciclo': aisl['mayor_ciclo'], 'niveles': aisl['niveles'],
               'segundos': round(time.time() - t0, 1)}
    (salida / 'resumen.json').write_text(json.dumps(resumen, indent=1), encoding='utf-8')
    print(json.dumps(resumen), flush=True)


if __name__ == '__main__':
    main()
