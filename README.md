# HomeMaid plugins

The registry of [HomeMaid](https://github.com/ErwanHeschung/HomeMaid) plugins: `index.json`,
read by the hub when you browse or install plugins from the app (Settings > Plugins). Nothing
else is read from here: the hub never needs the Internet to run.

## Write a plugin

```sh
cargo install cargo-generate
cargo generate --git https://github.com/ErwanHeschung/homemaid-plugins --subfolder template
```

See the generated README: build, try it on your hub, publish.

## Publish a plugin

Open a PR adding your plugin (or a new version) to `index.json`:

```json
{
  "id": "my-plugin",
  "name": "My plugin",
  "description": "What it brings to the home, in one line.",
  "repo": "https://github.com/you/my-plugin",
  "key": "RWQ...",
  "settings": [
    { "key": "hosts", "label": "IP addresses (comma-separated)", "required": true },
    { "key": "password", "label": "Device password", "secret": true, "required": true }
  ],
  "versions": [
    {
      "version": "0.1.0",
      "api": "1.2",
      "permissions": ["local-network"],
      "url": "https://github.com/you/my-plugin/releases/download/v0.1.0/my_plugin.wasm",
      "sha256": "…",
      "signature": "untrusted comment: …\n…\ntrusted comment: …\n…\n"
    }
  ]
}
```

- `id` is the one in your plugin's manifest. `key` is your minisign public key (the second line
  of `minisign.pub`).
- `settings` are asked in the app at install. Secrets go to the hub's `secrets.toml` and are
  never shown again; the others to `[plugin.settings]`. Your plugin reads them in `init` and
  with `secret()`.
- `api` is the plugin-api your SDK builds for (`API_VERSION`, `1.2.0` → `1.2`): older hubs do
  not offer it.
- `permissions` must match your manifest; the user accepts them at install and the hub refuses
  a plugin asking for more.

CI downloads each `.wasm`, checks its `sha256` and its signature with `key`.

### Trust

The hub checks every download against its sha256 and the author's key, and pins the key at
install: an update signed by another key is refused, even if this index changes it. A PR
changing a key fails CI unless labelled `key-change`, and needs a good reason (a lost key):
installed hubs must remove and reinstall the plugin.
