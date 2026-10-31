#!/usr/bin/env python3
"""Grafo de produccion de Chromium: componentes que se pueden construir y sustituir aislados.

Uso: python -I scripts/aislar-componentes.py [--todas-plataformas] datos/chromium-jerarquia [SARIF-SUPER-GRAFO]

Por defecto mira solo Linux (el destino de ULTRA): quita carpetas y objetivos de Android,
iOS, Windows, macOS, Fuchsia y ChromeOS, porque GN leido sin evaluar une todas las ramas.

Lee gn-objetivos.tsv.gz, gn-dependencias.tsv.gz y componentes.json (de
archivar-jerarquia-chromium.py) y escribe en la misma carpeta:
  aislamiento-produccion.json  piezas (ciclos), niveles y orden de construccion sin pruebas
  plan-componentes.tsv         una fila por componente: capa, nivel, tamano, Rust, señales, dependencias
El grafo completo (aislamiento.json) une casi todo en un ciclo porque las pruebas y //:gn_all
enlazan con todo. Aqui se quitan los objetivos de prueba, las herramientas y la raiz.
"""
import collections
import gzip
import importlib.util
import json
from pathlib import Path
import re
import sys

PRUEBA = re.compile(r'(test|mock|fake|fuzz|bench|perf|support|example|demo|sample|golden|unittest|browsertest)', re.I)
PLATAFORMA = re.compile(r'(^|[/_])(android|androidx|ios|win|windows|mac|macos|cocoa|fuchsia|chromeos|ash|lacros|cast|apple|jni)([/_]|$)', re.I)
SOLO_LINUX = True
CARPETA_PRUEBA = re.compile(r'(^|/)(test|tests|testing|test_support|fuzzers?|benchmarks?|examples?|tools)(/|$)')


def cargar_tarjan():
    ruta = Path(__file__).with_name('archivar-jerarquia-chromium.py')
    spec = importlib.util.spec_from_file_location('archivar', ruta)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.tarjan, mod.componente


def es_produccion(etq):
    carpeta, _, nombre = etq[2:].partition(':')
    if SOLO_LINUX and (PLATAFORMA.search(carpeta) or PLATAFORMA.search(nombre)):
        return False
    return bool(carpeta) and not PRUEBA.search(nombre) and not CARPETA_PRUEBA.search(carpeta) \
        and not carpeta.startswith(('testing', 'tools'))


def main():
    global SOLO_LINUX
    args = sys.argv[1:]
    if args and args[0] == '--todas-plataformas':
        SOLO_LINUX = False
        args = args[1:]
    if len(args) not in (1, 2):
        raise SystemExit(__doc__)
    d = Path(args[0])
    sufijo = '' if SOLO_LINUX else '-todas'
    tarjan, componente = cargar_tarjan()
    comp_de = lambda etq: componente(etq[2:].split(':')[0] + '/x')
    componentes = json.loads((d / 'componentes.json').read_text(encoding='utf-8'))

    objetivos_prod = collections.Counter()
    with gzip.open(d / 'gn-objetivos.tsv.gz', 'rt', encoding='utf-8') as f:
        next(f)
        for linea in f:
            etq = linea.split('\t')[0]
            if es_produccion(etq):
                objetivos_prod[comp_de(etq)] += 1
    entre = collections.Counter()
    obj_aristas = collections.defaultdict(set)
    with gzip.open(d / 'gn-dependencias.tsv.gz', 'rt', encoding='utf-8') as f:
        next(f)
        for linea in f:
            a, b, _ = linea.rstrip('\n').split('\t')
            if not (es_produccion(a) and es_produccion(b)):
                continue
            if a != b:
                obj_aristas[a].add(b)
            ca, cb = comp_de(a), comp_de(b)
            if ca != cb and ca in componentes and cb in componentes:
                entre[(ca, cb)] += 1

    # Orden real de construccion: por objetivo GN (GN prohibe ciclos entre objetivos;
    # los que aparezcan aqui vienen de unir ramas de if que nunca coinciden).
    obj_nodos = sorted(set(obj_aristas) | {b for v in obj_aristas.values() for b in v})
    obj_sccs = tarjan(obj_nodos, obj_aristas)
    obj_pieza = {o: i for i, s in enumerate(obj_sccs) for o in s}
    obj_nivel = {}
    for i, s in enumerate(obj_sccs):
        deps = {obj_pieza[b] for o in s for b in obj_aristas.get(o, ()) if obj_pieza[b] != i}
        obj_nivel[i] = 1 + max((obj_nivel[x] for x in deps), default=-1)
    nivel_obj = {o: obj_nivel[obj_pieza[o]] for o in obj_nodos}
    with gzip.open(d / f"objetivos-orden{'' if SOLO_LINUX else '-todas'}.tsv.gz", 'wt', encoding='utf-8') as f:
        f.write('objetivo\tcomponente\tnivel\tdepende_de\tciclo\n')
        for o in sorted(obj_nodos, key=lambda o: (nivel_obj[o], o)):
            f.write(f"{o}\t{comp_de(o)}\t{nivel_obj[o]}\t{len(obj_aristas.get(o, ()))}\t{len(obj_sccs[obj_pieza[o]]) if len(obj_sccs[obj_pieza[o]]) > 1 else ''}\n")
    rango_comp = collections.defaultdict(list)
    for o, n in nivel_obj.items():
        rango_comp[comp_de(o)].append(n)
    obj_ciclos = sorted((s for s in obj_sccs if len(s) > 1), key=len, reverse=True)
    fuera = (lambda c: SOLO_LINUX and bool(PLATAFORMA.search(c)))
    nodos = sorted(c for c in set(c for c in componentes if objetivos_prod[c]) | {b for (_, b) in entre} | {a for (a, _) in entre} if not fuera(c))
    aristas = collections.defaultdict(set)
    for (a, b) in entre:
        aristas[a].add(b)
    sccs = tarjan(nodos, aristas)
    pieza = {c: i for i, s in enumerate(sccs) for c in s}
    nivel = {}
    for i, s in enumerate(sccs):
        deps = {pieza[b] for c in s for b in aristas[c] if pieza[b] != i}
        nivel[i] = 1 + max((nivel[x] for x in deps), default=-1)
    nivel_de = {c: nivel[pieza[c]] for c in nodos}

    # Señales del SUPER GRAFO por componente (opcional).
    senales = collections.defaultdict(collections.Counter)
    if len(args) == 2:
        sarif = json.loads(Path(args[1]).read_text(encoding='utf-8'))
        ejes = {}
        for r in sarif['runs'][0]['tool']['driver'].get('rules', []):
            ejes[r['id']] = (r.get('properties') or {}).get('eje')
        for x in sarif['runs'][0]['results']:
            uri = x['locations'][0]['physicalLocation']['artifactLocation']['uri']
            eje = (x.get('properties') or {}).get('eje') or ejes.get(x['ruleId']) or '?'
            senales[componente(uri)][eje] += 1
            if x.get('level') == 'error':
                senales[componente(uri)]['errores'] += 1

    piezas = []
    for i, s in enumerate(sccs):
        piezas.append({'nivel': nivel[i], 'componentes': s,
                       'archivos': sum(componentes.get(c, {}).get('archivos', 0) for c in s),
                       'depende_de_niveles': sorted({nivel[pieza[b]] for c in s for b in aristas[c] if pieza[b] != i})})
    piezas.sort(key=lambda p: (p['nivel'], -p['archivos']))
    ciclos = sorted((p for p in piezas if len(p['componentes']) > 1), key=lambda p: -len(p['componentes']))
    por_nivel = collections.defaultdict(lambda: {'componentes': 0, 'archivos': 0})
    for p in piezas:
        por_nivel[p['nivel']]['componentes'] += len(p['componentes'])
        por_nivel[p['nivel']]['archivos'] += p['archivos']
    resultado = {
        'plataformas': 'solo Linux' if SOLO_LINUX else 'todas',
        'nota': 'Solo objetivos de produccion (sin pruebas, herramientas, ejemplos ni //:gn_all). '
                'Lexico: incluye las ramas de todos los if de GN, asi que sobreestima. '
                'Una pieza de varios componentes es un ciclo: se construye o sustituye entera.',
        'componentes': len(nodos), 'aristas': sum(len(v) for v in aristas.values()), 'piezas': len(piezas),
        'niveles': 1 + max(nivel.values(), default=-1), 'por_nivel': {str(k): v for k, v in sorted(por_nivel.items())},
        'objetivos': {'nodos': len(obj_nodos), 'aristas': sum(len(v) for v in obj_aristas.values()),
                      'niveles': 1 + max(obj_nivel.values(), default=-1), 'ciclos': len(obj_ciclos),
                      'mayor_ciclo': len(obj_ciclos[0]) if obj_ciclos else 0,
                      'ciclos_lista': [s[:20] for s in obj_ciclos[:20]]},
        'rango_de_niveles_por_componente': {c: [min(v), max(v)] for c, v in sorted(rango_comp.items())},
        'ciclos': [{'componentes': len(p['componentes']), 'archivos': p['archivos'], 'nivel': p['nivel'], 'lista': p['componentes']} for p in ciclos],
        'piezas_lista': piezas,
    }
    (d / f'aislamiento-produccion{sufijo}.json').write_text(json.dumps(resultado, indent=1, ensure_ascii=False), encoding='utf-8')

    usa = collections.defaultdict(set); usado = collections.defaultdict(set)
    for (a, b) in entre:
        usa[a].add(b); usado[b].add(a)
    with open(d / f'plan-componentes{sufijo}.tsv', 'w', encoding='utf-8') as f:
        f.write('componente\tcapa\tnivel\tpieza_tamano\tarchivos\tlenguaje_principal\trust\tobjetivos_prod\tusa\tlo_usan\t'
                'seguridad\terrores\telegancia\trendimiento\tlicencia\torigen\n')
        for c in sorted(nodos, key=lambda c: (nivel_de[c], componentes.get(c, {}).get('capa', ''), c)):
            k = componentes.get(c, {})
            s = senales.get(c, {})
            lic = ((k.get('readme_chromium') or {}).get('License') or '').replace('\t', ' ')
            org = (k.get('origen_gclient') or {}).get('url', '')
            f.write('\t'.join(map(str, [c, k.get('capa', ''), nivel_de[c], min(rango_comp.get(c, [''])), max(rango_comp.get(c, [''])), len(sccs[pieza[c]]), k.get('archivos', 0), k.get('lenguaje_principal') or '',
                                        k.get('rust', 0), objetivos_prod[c], len(usa[c]), len(usado[c]), s.get('Seguridad', 0), s.get('errores', 0),
                                        s.get('Elegancia', 0), s.get('Rendimiento', 0), lic, org])) + '\n')
    print(json.dumps({k: v for k, v in resultado.items() if k not in ('ciclos', 'piezas_lista', 'rango_de_niveles_por_componente')}, ensure_ascii=False)[:1500])
    for p in ciclos[:8]:
        print('ciclo', len(p['componentes']), 'componentes,', p['archivos'], 'archivos, nivel', p['nivel'], p['componentes'][:12])


if __name__ == '__main__':
    main()
