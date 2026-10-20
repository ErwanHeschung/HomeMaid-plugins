#!/usr/bin/env python3
"""Releases the plugins whose version is not in index.json yet: for each plugins/<dir> with a
registry.json (its listing), builds the .wasm, signs it with minisign, creates the GitHub
release <id>-v<version> (.wasm, .minisig, entry.json) and adds the version to index.json,
checked by check.py, committed and pushed.

Run by .github/workflows/release.yml on main, once approved; locally too (from the repo root):
    MINISIGN_KEY=<secret key file> python3 tools/release.py [--dry-run]
    python3 tools/release.py --pending    (only lists the tags to release)
Needs cargo with wasm32-wasip2, minisign, gh. A version is released once: bump it in Cargo.toml.
"""
import hashlib, json, os, pathlib, re, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
KEY = (ROOT / 'minisign.pub').read_text().strip().splitlines()[-1]
DRY = '--dry-run' in sys.argv
PENDING = '--pending' in sys.argv
REPO = os.environ.get('GITHUB_REPOSITORY', 'ErwanHeschung/HomeMaid-plugins')


def run(*args, **kw):
    print('+', ' '.join(args), flush=True)
    return subprocess.run(args, check=True, cwd=ROOT, **kw)


def sdk_api():
    """The plugin-api the SDK builds for (`1.2.0` -> `1.2`): the oldest hub minor that runs it."""
    meta = json.loads(subprocess.run(['cargo', 'metadata', '--format-version', '1'], check=True, cwd=ROOT, capture_output=True).stdout)
    manifest = next(p['manifest_path'] for p in meta['packages'] if p['name'] == 'homemaid-plugin')
    lib = (pathlib.Path(manifest).parent / 'src/lib.rs').read_text()
    return re.search(r'API_VERSION: &str = "(\d+\.\d+)', lib).group(1)


index = json.loads((ROOT / 'index.json').read_text())
listed = {(p['id'], v['version']) for p in index['plugins'] for v in p['versions']}
pending = []
for listing in sorted(ROOT.glob('plugins/*/registry.json')):
    entry = json.loads(listing.read_text())
    cargo = (listing.parent / 'Cargo.toml').read_text()
    crate = re.search(r'^name = "(.+)"', cargo, re.M).group(1)
    version = re.search(r'^version = "(.+)"', cargo, re.M).group(1)
    tag = f"{entry['id']}-v{version}"
    if (entry['id'], version) not in listed:
        pending.append((entry, crate, version, tag))
if PENDING:
    for *_, tag in pending:
        print(tag)
    sys.exit()

api = sdk_api() if pending else None
for entry, crate, version, tag in pending:
    run('cargo', 'build', '--release', '-p', crate, '--target', 'wasm32-wasip2')
    wasm = ROOT / 'target/wasm32-wasip2/release' / f"{crate.replace('-', '_')}.wasm"
    run('minisign', '-S', '-s', os.environ['MINISIGN_KEY'], '-m', str(wasm), '-t', f'{tag} plugin-api {api}', stdin=subprocess.DEVNULL)
    entry.update(key=KEY, versions=[{
        'version': version,
        'api': api,
        'permissions': entry.pop('permissions'),
        'url': f'https://github.com/{REPO}/releases/download/{tag}/{wasm.name}',
        'sha256': hashlib.sha256(wasm.read_bytes()).hexdigest(),
        'signature': pathlib.Path(f'{wasm}.minisig').read_text(),
    }])
    out = ROOT / 'target/releases' / tag / 'entry.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(entry, indent=2) + '\n')
    if DRY:
        print(f'{tag}: built and signed, not released (--dry-run): {out}')
        continue
    run('gh', 'release', 'create', tag, str(wasm), f'{wasm}.minisig', str(out), '--repo', REPO,
        '--title', f"{entry['name']} {version}", '--notes', f"Plugin `{entry['id']}` {version}, plugin-api {api}.")
    # The listing follows the plugin's registry.json; versions only grow; the key never changes here.
    mine = next((p for p in index['plugins'] if p['id'] == entry['id']), None)
    if mine is None:
        index['plugins'].append(entry)
    else:
        mine.update({k: entry[k] for k in ('name', 'description', 'repo', 'settings')})
        mine['versions'] += entry['versions']

if pending and not DRY:
    (ROOT / 'index.json').write_text(json.dumps(index, indent=2, ensure_ascii=False) + '\n')
    run('python3', 'check.py', 'index.json')
    run('git', 'commit', '-am', 'Index: ' + ', '.join(tag for *_, tag in pending))
    run('git', 'push', 'origin', 'HEAD:main')
