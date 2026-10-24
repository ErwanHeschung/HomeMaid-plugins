#!/usr/bin/env python3
"""Checks index.json before it is merged: every field the core reads, unique ids, and each
version's .wasm downloaded, matching its sha256 and signed by its plugin's key (minisign -V).

    python3 check.py index.json [--base old-index.json] [--allow-key-change]

With --base, a plugin whose key differs from the base index is refused: installed cores pin the
key and would refuse its updates anyway. A key change needs the `key-change` label on the PR.
"""
import hashlib, json, re, subprocess, sys, tempfile, urllib.request

ID = re.compile(r'^[a-z0-9][a-z0-9-]{0,40}$')
VERSION = re.compile(r'^\d+\.\d+\.\d+$')
API = re.compile(r'^\d+\.\d+$')


def fail(msg):
    sys.exit(f'index.json: {msg}')


def main():
    args = sys.argv[1:]
    index = json.load(open(args[0]))
    base = json.load(open(args[args.index('--base') + 1])) if '--base' in args else None
    if index.get('format') != 1:
        fail('format must be 1')
    seen = set()
    for p in index['plugins']:
        pid = p.get('id', '')
        if not ID.match(pid):
            fail(f'bad id {pid!r}: lowercase letters, digits and dashes')
        if pid in seen:
            fail(f'{pid}: listed twice')
        seen.add(pid)
        if base and '--allow-key-change' not in args:
            old = next((o for o in base['plugins'] if o['id'] == pid), None)
            if old and old['key'] != p.get('key'):
                fail(f'{pid}: its key changed; installed cores will refuse its updates (label the PR `key-change` if intended)')
        for field in ('name', 'key', 'versions'):
            if not p.get(field):
                fail(f'{pid}: `{field}` is missing')
        for s in p.get('settings', []) + p.get('device_settings', []):
            if not s.get('key') or not s.get('label'):
                fail(f'{pid}: every setting needs a `key` and a `label`')
        for v in p['versions']:
            where = f"{pid} {v.get('version')}"
            if not VERSION.match(v.get('version', '')) or not API.match(v.get('api', '')):
                fail(f'{where}: `version` is x.y.z and `api` is x.y')
            for field in ('url', 'sha256', 'signature'):
                if not v.get(field):
                    fail(f'{where}: `{field}` is missing')
            if not isinstance(v.get('permissions'), list):
                fail(f'{where}: `permissions` is a list')
            data = urllib.request.urlopen(v['url'], timeout=60).read()
            if hashlib.sha256(data).hexdigest() != v['sha256'].lower():
                fail(f'{where}: the file at `url` does not match `sha256`')
            with tempfile.NamedTemporaryFile() as wasm, tempfile.NamedTemporaryFile('w') as sig:
                wasm.write(data)
                wasm.flush()
                sig.write(v['signature'])
                sig.flush()
                ok = subprocess.run(['minisign', '-V', '-q', '-P', p['key'], '-m', wasm.name, '-x', sig.name])
                if ok.returncode != 0:
                    fail(f"{where}: not signed by {pid}'s key")
            print(f'{where}: ok')


main()
