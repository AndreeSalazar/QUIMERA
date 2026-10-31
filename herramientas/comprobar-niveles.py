#!/usr/bin/env python3
"""Guardián de la jerarquía de QUIMERA.

Uso: python -I herramientas/comprobar-niveles.py   (desde la raíz del proyecto)

Cada crate declara en su Cargo.toml:
    [package.metadata.quimera]
    nivel = 2            # 0 = cimientos
    capa = "motor-js"
    no_std = true
    externos = []        # crates de fuera permitidos

Reglas (falla con código 1 si alguna se rompe):
  1. Un crate solo usa crates de QUIMERA de niveles MENORES (nunca del mismo ni de uno mayor).
  2. Solo usa crates de fuera que estén en su lista `externos`.
  3. Si declara `no_std = true`, su lib.rs lo dice (`#![no_std]` o `#![cfg_attr(not(test), no_std)]`).
  4. Su carpeta coincide con su capa (`motor-js/gc` -> capa "motor-js").
  5. Cada miembro del workspace tiene su metadata.
"""
import sys
import tomllib
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent


def main():
    ws = tomllib.loads((RAIZ / 'Cargo.toml').read_text(encoding='utf-8'))
    miembros = ws['workspace']['members']
    crates = {}
    fallos = []
    for m in miembros:
        doc = tomllib.loads((RAIZ / m / 'Cargo.toml').read_text(encoding='utf-8'))
        nombre = doc['package']['name']
        meta = doc['package'].get('metadata', {}).get('quimera')
        if meta is None:
            fallos.append(f'{m}: falta [package.metadata.quimera]')
            continue
        deps = set(doc.get('dependencies', {})) | set(doc.get('build-dependencies', {}))
        crates[nombre] = {'ruta': m, 'meta': meta, 'deps': deps}
    for nombre, c in sorted(crates.items(), key=lambda x: x[1]['meta']['nivel']):
        meta, ruta = c['meta'], c['ruta']
        if ruta.split('/')[0] != meta.get('capa'):
            fallos.append(f"{nombre}: está en {ruta.split('/')[0]}/ pero declara capa {meta.get('capa')!r}")
        for d in sorted(c['deps']):
            if d in crates:
                nd = crates[d]['meta']['nivel']
                if nd >= meta['nivel']:
                    fallos.append(f"{nombre} (nivel {meta['nivel']}) usa {d} (nivel {nd}): solo se puede usar lo de niveles menores")
            elif d not in meta.get('externos', []):
                fallos.append(f"{nombre}: usa {d}, que es de fuera y no está en su lista `externos`")
        if meta.get('no_std'):
            lib = RAIZ / ruta / 'src' / 'lib.rs'
            texto = lib.read_text(encoding='utf-8') if lib.is_file() else ''
            if '#![no_std]' not in texto and 'no_std)]' not in texto:
                fallos.append(f'{nombre}: declara no_std = true pero su lib.rs no lo dice')
    for nombre, c in sorted(crates.items(), key=lambda x: (x[1]['meta']['nivel'], x[0])):
        m = c['meta']
        print(f"  nivel {m['nivel']:>2}  {m['capa']:<10} {nombre:<20} usa: {', '.join(sorted(c['deps'])) or '-'}")
    if fallos:
        print('\nRoto:')
        for f in fallos:
            print(f'  ✗ {f}')
        sys.exit(1)
    print(f'\nJerarquía correcta: {len(crates)} crates.')


if __name__ == '__main__':
    main()
