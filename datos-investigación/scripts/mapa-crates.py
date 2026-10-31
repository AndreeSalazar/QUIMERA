#!/usr/bin/env python3
"""Mapa de un proyecto Rust (o mixto): crates, quien usa a quien, tamano, unsafe y carpetas.

Uso: python3 -I mapa-crates.py RAIZ SALIDA-NUEVA

Escribe:
  crates.json        cada crate del proyecto: ruta, archivos .rs, lineas, unsafe, deps internas/externas, nivel
  crates-orden.tsv   crates por nivel (0 = no usa otros crates del proyecto)
  carpetas.tsv       cada carpeta hasta 4 niveles: archivos y lineas por lenguaje
  resumen.json
Lineas = lineas no vacias. `unsafe` se cuenta como palabra fuera de comentarios de linea (lexico).
"""
import collections
import json
import os
from pathlib import Path
import re
import sys
import tomllib

LENG = {'.rs': 'Rust', '.c': 'C', '.h': 'C/C++ cab', '.cc': 'C++', '.cpp': 'C++', '.cxx': 'C++', '.hpp': 'C++', '.py': 'Python',
        '.js': 'JS', '.mjs': 'JS', '.ts': 'TS', '.html': 'HTML', '.css': 'CSS', '.webidl': 'WebIDL', '.toml': 'TOML'}
FUERA = {'.git', 'target', 'node_modules', '__pycache__'}
RE_UNSAFE = re.compile(r'\bunsafe\b')


def lineas(p):
    try:
        with open(p, 'rb') as f:
            datos = f.read(8 << 20)
    except OSError:
        return 0, 0
    n = u = 0
    for l in datos.decode('utf-8', 'replace').splitlines():
        s = l.strip()
        if not s:
            continue
        n += 1
        if p.suffix == '.rs' and not s.startswith('//') and RE_UNSAFE.search(s.split('//')[0]):
            u += 1
    return n, u


def tarjan(nodos, aristas):
    idx, low, en, pila, out, c = {}, {}, set(), [], [], [0]
    def visita(v):
        trabajo = [(v, iter(sorted(aristas.get(v, ()))))]
        idx[v] = low[v] = c[0]; c[0] += 1; pila.append(v); en.add(v)
        while trabajo:
            x, it = trabajo[-1]
            for w in it:
                if w not in idx:
                    idx[w] = low[w] = c[0]; c[0] += 1; pila.append(w); en.add(w)
                    trabajo.append((w, iter(sorted(aristas.get(w, ())))))
                    break
                if w in en:
                    low[x] = min(low[x], idx[w])
            else:
                trabajo.pop()
                if trabajo:
                    low[trabajo[-1][0]] = min(low[trabajo[-1][0]], low[x])
                if low[x] == idx[x]:
                    s = []
                    while True:
                        w = pila.pop(); en.discard(w); s.append(w)
                        if w == x:
                            break
                    out.append(sorted(s))
    for v in nodos:
        if v not in idx:
            visita(v)
    return out


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    raiz = Path(sys.argv[1]).resolve(strict=True)
    salida = Path(sys.argv[2]).resolve()
    salida.mkdir(parents=True, exist_ok=False)

    # Carpetas y archivos.
    carpetas = collections.defaultdict(lambda: collections.defaultdict(lambda: [0, 0]))
    por_archivo = {}
    total = collections.defaultdict(lambda: [0, 0])
    for d, dirs, archivos in os.walk(raiz):
        dirs[:] = sorted(x for x in dirs if x not in FUERA)
        rel = Path(d).relative_to(raiz)
        for a in archivos:
            p = Path(d) / a
            lg = LENG.get(p.suffix.lower())
            if not lg or p.is_symlink():
                continue
            n, u = lineas(p) if lg != 'TOML' else (0, 0)
            por_archivo[p] = (n, u)
            total[lg][0] += 1; total[lg][1] += n
            partes = rel.parts
            for k in range(0, min(len(partes), 4) + 1):
                carpetas['/'.join(partes[:k]) or '.'][lg][0] += 1
                carpetas['/'.join(partes[:k]) or '.'][lg][1] += n
    lgs = sorted(total, key=lambda l: -total[l][1])
    with open(salida / 'carpetas.tsv', 'w', encoding='utf-8') as f:
        f.write('carpeta\tprofundidad\tarchivos\tlineas\t' + '\t'.join(f'{l}_lineas' for l in lgs) + '\n')
        for c in sorted(carpetas):
            v = carpetas[c]
            f.write(f"{c}\t{0 if c == '.' else c.count('/') + 1}\t{sum(x[0] for x in v.values())}\t{sum(x[1] for x in v.values())}\t"
                    + '\t'.join(str(v[l][1]) if l in v else '0' for l in lgs) + '\n')

    # Crates.
    crates = {}
    for d, dirs, archivos in os.walk(raiz):
        dirs[:] = sorted(x for x in dirs if x not in FUERA)
        if 'Cargo.toml' not in archivos:
            continue
        try:
            doc = tomllib.loads((Path(d) / 'Cargo.toml').read_text(encoding='utf-8'))
        except (tomllib.TOMLDecodeError, UnicodeDecodeError):
            continue
        paquete = doc.get('package') or {}
        nombre = paquete.get('name')
        if not nombre or isinstance(nombre, dict):
            continue
        deps = {}
        secciones = [doc.get('dependencies', {}), doc.get('build-dependencies', {})]
        for t in (doc.get('target') or {}).values():
            secciones += [t.get('dependencies', {}), t.get('build-dependencies', {})]
        for s in secciones:
            for k, v in s.items():
                real = v.get('package', k) if isinstance(v, dict) else k
                deps[real] = v.get('path') if isinstance(v, dict) else None
        rs = [p for p in por_archivo if p.suffix == '.rs' and Path(d) in p.parents]
        crates[nombre] = {'ruta': str(Path(d).relative_to(raiz)) or '.', 'deps': deps, 'no_std': None,
                          'version': paquete.get('version') if isinstance(paquete.get('version'), str) else None,
                          'descripcion': paquete.get('description') if isinstance(paquete.get('description'), str) else None}
        lib = Path(d) / 'src' / 'lib.rs'
        if lib.is_file():
            cab = lib.read_text(encoding='utf-8', errors='replace')[:6000]
            crates[nombre]['no_std'] = '#![no_std]' in cab or 'no_std)]' in cab
    # Archivos de cada crate: los de su carpeta que no son de un crate mas profundo.
    rutas = sorted(((raiz / c['ruta'], n) for n, c in crates.items()), key=lambda x: -len(str(x[0])))
    cuenta = collections.defaultdict(lambda: [0, 0, 0])
    for p, (n, u) in por_archivo.items():
        if p.suffix != '.rs':
            continue
        for r, nombre in rutas:
            if r == p.parent or r in p.parents:
                cuenta[nombre][0] += 1; cuenta[nombre][1] += n; cuenta[nombre][2] += u
                break
    internos = set(crates)
    aristas = {n: {d for d in c['deps'] if d in internos and d != n} for n, c in crates.items()}
    sccs = tarjan(sorted(crates), aristas)
    pieza = {c: i for i, s in enumerate(sccs) for c in s}
    nivel = {}
    for i, s in enumerate(sccs):
        deps = {pieza[b] for c in s for b in aristas[c] if pieza[b] != i}
        nivel[i] = 1 + max((nivel[x] for x in deps), default=-1)
    usado = collections.defaultdict(set)
    for a, bs in aristas.items():
        for b in bs:
            usado[b].add(a)
    for n, c in crates.items():
        c.update({'archivos_rs': cuenta[n][0], 'lineas_rs': cuenta[n][1], 'lineas_unsafe': cuenta[n][2],
                  'nivel': nivel[pieza[n]], 'usa_internos': sorted(aristas[n]), 'lo_usan': sorted(usado[n]),
                  'externos': sorted(d for d in c['deps'] if d not in internos)})
        del c['deps']
    (salida / 'crates.json').write_text(json.dumps(crates, indent=1, ensure_ascii=False), encoding='utf-8')
    with open(salida / 'crates-orden.tsv', 'w', encoding='utf-8') as f:
        f.write('nivel\tcrate\truta\tlineas_rs\tlineas_unsafe\tno_std\tusa\tlo_usan\texternos\n')
        for n, c in sorted(crates.items(), key=lambda x: (x[1]['nivel'], -x[1]['lineas_rs'])):
            f.write(f"{c['nivel']}\t{n}\t{c['ruta']}\t{c['lineas_rs']}\t{c['lineas_unsafe']}\t{c['no_std']}\t"
                    f"{len(c['usa_internos'])}\t{len(c['lo_usan'])}\t{len(c['externos'])}\n")
    externos = collections.Counter(e for c in crates.values() for e in c['externos'])
    resumen = {'lenguajes': {l: {'archivos': total[l][0], 'lineas': total[l][1]} for l in lgs},
               'crates': len(crates), 'niveles': 1 + max(nivel.values(), default=-1),
               'ciclos_entre_crates': sum(1 for s in sccs if len(s) > 1),
               'lineas_rs': sum(c['lineas_rs'] for c in crates.values()),
               'lineas_unsafe': sum(c['lineas_unsafe'] for c in crates.values()),
               'externos_distintos': len(externos), 'externos_mas_usados': externos.most_common(25)}
    (salida / 'resumen.json').write_text(json.dumps(resumen, indent=1, ensure_ascii=False), encoding='utf-8')
    print(json.dumps({k: v for k, v in resumen.items() if k != 'externos_mas_usados'}, ensure_ascii=False))


if __name__ == '__main__':
    main()
