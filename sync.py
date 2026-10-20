#!/usr/bin/env python3
"""Proposes the official plugins' new versions to index.json: each release of the source repos
with an entry.json asset (made by HomeMaid's release workflow) whose version is not listed yet
is added, the listing (name, description, settings) refreshed, never the key.

    GH_TOKEN=... python3 sync.py    (prints what changed; exits 0 when nothing did)

The result is then checked (check.py) and proposed as a PR by .github/workflows/sync.yml.
"""
import json, subprocess, sys, urllib.request

SOURCES = ['ErwanHeschung/HomeMaid']


def entries(repo):
    """The entry.json of each published release, oldest first."""
    query = '.[] | select(.draft | not) | .assets[] | select(.name == "entry.json") | .browser_download_url'
    out = subprocess.run(['gh', 'api', '--paginate', f'repos/{repo}/releases', '--jq', query], check=True, capture_output=True, text=True).stdout
    return [json.loads(urllib.request.urlopen(url, timeout=60).read()) for url in reversed(out.split())]


index = json.load(open('index.json'))
by_id = {p['id']: p for p in index['plugins']}
changed = []
for repo in SOURCES:
    for entry in entries(repo):
        version = entry['versions'][0]
        mine = by_id.get(entry['id'])
        if mine is None:
            by_id[entry['id']] = entry
            index['plugins'].append(entry)
            changed.append(f"{entry['id']} {version['version']} (new plugin)")
            continue
        if mine['key'] != entry['key']:
            print(f"{entry['id']} {version['version']}: signed with another key than the index's, left out", file=sys.stderr)
            continue
        if any(v['version'] == version['version'] for v in mine['versions']):
            continue
        mine.update({k: entry[k] for k in ('name', 'description', 'repo', 'settings') if k in entry})
        mine['versions'].append(version)
        changed.append(f"{entry['id']} {version['version']}")

if changed:
    with open('index.json', 'w') as f:
        json.dump(index, f, indent=2, ensure_ascii=False)
        f.write('\n')
print('\n'.join(changed))
