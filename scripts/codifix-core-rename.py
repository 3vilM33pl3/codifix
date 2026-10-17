#!/usr/bin/env python3
"""One-shot core rename at 6af8cc36; default is a reviewable dry run.

Apply only to the recorded baseline. TOML is parsed before/after and checked
against the same structural rename; Cargo.lock may change local names only.
Protocol strings, state/config/env identities and historical evidence are out of
scope. This script deliberately does not replace arbitrary `memory` strings.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
MAP = json.loads((ROOT / 'docs/reviews/codifix-rename/evidence/core-target-map.json').read_text())
PACKAGES = MAP['packages']
PACKAGE_RE = re.compile(r'(?<![\w-])(' + '|'.join(map(re.escape, sorted(PACKAGES, key=len, reverse=True))) + r')(?![\w-])')
IMPORTS = {old.replace('-', '_'): new.replace('-', '_') for old, new in PACKAGES.items()}
IMPORT_RE = re.compile(r'\b(' + '|'.join(map(re.escape, IMPORTS)) + r')(?=::)')
BINS = {old: value['name'] for old, value in MAP['binaries'].items()}


def packages(text):
    return PACKAGE_RE.sub(lambda m: PACKAGES[m[0]], text)


def structure(value, transform):
    if isinstance(value, dict):
        return {transform(k): structure(v, transform) for k, v in value.items()}
    if isinstance(value, list):
        return [structure(v, transform) for v in value]
    return transform(value) if isinstance(value, str) else value


def manifest(text):
    result = packages(text)
    for old, new in BINS.items():
        result = result.replace(f'"{old}"', f'"{new}"')
        result = result.replace(f'src/bin/{old}', f'src/bin/{new}')
    return result


def manifest_value(text):
    result = BINS.get(text, packages(text))
    for old, new in BINS.items():
        result = result.replace(f'src/bin/{old}', f'src/bin/{new}')
    return result


def rust(text):
    result = IMPORT_RE.sub(lambda m: IMPORTS[m[0]], text)
    result = re.sub(r'\b((?:extern crate|use)\s+)(' + '|'.join(map(re.escape, IMPORTS)) + r')(?=\s*(?:;|as\b))', lambda m: m[1] + IMPORTS[m[2]], result)
    result = re.sub(r'/(mem-[a-z-]+)/', lambda m: packages(m[0]), result)
    for old, new in BINS.items():
        result = result.replace(f'CARGO_BIN_EXE_{old}"', f'CARGO_BIN_EXE_{new}"')
        result = result.replace(f'--bin {old} ', f'--bin {new} ')
        result = result.replace(f'name = "{old}"', f'name = "{new}"')
        result = result.replace(f'#[path = "{old}/', f'#[path = "{new}/')
        if old != 'memory':
            result = re.sub(r'(?<![\w-])' + re.escape(old) + r'(?![\w-])', new, result)
    result = result.replace('memory.exe', 'codifix.exe')
    result = result.replace('current_exe_sibling_binary("memory")', 'current_exe_sibling_binary("codifix")')
    return result


def build_script(text):
    result = packages(text)
    # Package/import guards include names that are intentionally absent.
    result = result.replace('mem-api', 'codifix-api').replace('mem_api::', 'codifix_api::')
    result = result.replace('mem-models', 'codifix-models').replace('mem_federation_runtime', 'codifix_federation_runtime')
    for old, new in BINS.items():
        result = re.sub(r'--bin ' + re.escape(old) + r'''(?=\s|['"`]|$)''', '--bin ' + new, result)
        result = result.replace(f"'--bin','{old}'", f"'--bin','{new}'")
        if old != 'memory':
            result = re.sub(r'(?<![\w-])' + re.escape(old) + r'(?![\w-])', new, result)
    result = result.replace('memory.exe', 'codifix.exe')
    result = re.sub(r'(target/(?:debug|release)/)memory\b', r'\1codifix', result)
    result = result.replace('${built}/memory`', '${built}/codifix`')
    return result


def executable_sites(name, text):
    # These literals denote executable selectors, never semantic memory fields.
    if name == 'crates/mem-cli/src/commands/completion.rs':
        text = text.replace('"memory"', '"codifix"')
    elif name == 'crates/mem-cli/src/commands/runtime/tests.rs':
        text = text.replace('generate(shell, &mut command, "memory",', 'generate(shell, &mut command, "codifix",')
        text = text.replace('contains("_memory")', 'contains("_codifix")')
        text = text.replace('contains("complete -c memory")', 'contains("complete -c codifix")')
    elif name == 'crates/mem-cli/src/commands/status_support.rs':
        text = text.replace('"memory"', '"codifix"').replace('`memory`', '`codifix`')
    elif name == 'crates/mem-cli/src/commands/eval_support.rs':
        text = text.replace('"memory".to_string()', '"codifix".to_string()')
    elif name == 'scripts/windows/test_native_eval_bootstrap.py':
        text = text.replace('"\'memory\'"', '"\'codifix\'"')
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    tracked = subprocess.check_output(['git', 'ls-files'], cwd=ROOT, text=True).splitlines()
    changes = {}
    for name in tracked:
        path = ROOT / name
        if name in ('Cargo.toml', 'Cargo.lock') or (name.startswith('crates/mem-') and path.name == 'Cargo.toml'):
            transform = packages if name == 'Cargo.lock' else manifest
        elif name.startswith('crates/mem-') and path.suffix == '.rs':
            transform = rust
        elif name.startswith(('scripts/', '.github/', 'tests/')) and path.suffix in ('.sh', '.ps1', '.mjs', '.py', '.yml', '.yaml'):
            if name in ('scripts/codifix-core-rename.py', 'scripts/install-local.sh', 'scripts/areas/classify.mjs', 'scripts/areas/classify.test.mjs'):
                continue
            transform = build_script
        else:
            continue
        before = path.read_text()
        after = executable_sites(name, transform(before))
        if path.suffix == '.toml' or name == 'Cargo.lock':
            expected = packages if name == 'Cargo.lock' else manifest_value
            assert tomllib.loads(after) == structure(tomllib.loads(before), expected), name
        if before != after:
            changes[name] = after
    moves = {f'crates/{old}': f'crates/{new}' for old, new in PACKAGES.items()}
    for old, target in MAP['binaries'].items():
        source = target['source']
        if '/src/bin/' in source:
            moves[source] = source.replace('/' + old + '.rs', '/' + target['name'] + '.rs')
            directory = str(Path(source).with_suffix(''))
            if (ROOT / directory).is_dir():
                moves[directory] = str(Path(moves[source]).with_suffix(''))
    print(json.dumps({'changed_files': sorted(changes), 'moves': moves}, indent=2))
    if args.apply:
        assert subprocess.check_output(['git', 'rev-parse', '--short=8', 'HEAD'], cwd=ROOT, text=True).strip() == MAP['baseline']
        subprocess.run(['git', 'diff', '--quiet'], cwd=ROOT, check=True)
        subprocess.run(['git', 'diff', '--cached', '--quiet'], cwd=ROOT, check=True)
        assert all((ROOT / old).exists() for old in moves), 'missing baseline source path'
        assert all(not (ROOT / new).exists() for new in moves.values()), 'target path already exists'
        for name, text in changes.items():
            (ROOT / name).write_text(text)
        for old, new in sorted(moves.items(), key=lambda pair: len(pair[0]), reverse=True):
            (ROOT / old).rename(ROOT / new)


if __name__ == '__main__':
    main()
