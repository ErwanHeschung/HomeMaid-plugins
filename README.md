# HomeMaid-plugins

The registry of [HomeMaid](https://github.com/ErwanHeschung/HomeMaid) plugins: `index.json`,
read by the hub when you browse or install plugins from the app (Settings > Plugins), and the
official plugins themselves (`plugins/`). The hub never needs the Internet to run.

## Official plugins

`plugins/<id>`: a Cargo workspace (`cargo test`, `cargo build --release --target wasm32-wasip2`).
Each has a `registry.json`, its listing (name, description, permissions, the settings the app
asks). They pin the SDK to a HomeMaid tag (`plugin-api-v1.2.0`); their tests run them on a real
hub at the same tag.

To release one, bump its version in `Cargo.toml` and merge: the release workflow waits for an
approval, then builds, signs (CI key, `minisign.pub`), releases `<id>-v<version>` and adds it
to `index.json`.

## Write a plugin

```sh
cargo install cargo-generate
cargo generate --git https://github.com/ErwanHeschung/HomeMaid-plugins --subfolder template
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

CI downloads each `.wasm`, checks its `sha256` and its signature with `key`. The template's
release workflow makes the `.wasm`, its signature and this entry (`entry.json`) on a tag.

### Trust

The hub checks every download against its sha256 and the author's key, and pins the key at
install: an update signed by another key is refused, even if this index changes it. A PR
changing a key fails CI unless labelled `key-change`, and needs a good reason (a lost key):
installed hubs must remove and reinstall the plugin.
