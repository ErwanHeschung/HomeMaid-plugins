# {{project-name}}

A [HomeMaid](https://github.com/ErwanHeschung/HomeMaid) plugin.

## Build

```sh
rustup target add wasm32-wasip2
cargo build --release      # target/wasm32-wasip2/release/{{crate_name}}.wasm
```

## Try it on your hub

Copy the `.wasm` next to `homemaid.toml` and add:

```toml
[[plugin]]
id = "{{plugin_id}}"
path = "{{crate_name}}.wasm"
permissions = ["local-network"]

[plugin.settings]
# what init() reads
```

Secrets go in `data/secrets.toml`, under `[{{plugin_id}}]`. Restart the hub: its log says what the
plugin does, and its devices appear in the app.

## Publish

1. Make a signing key once, and keep a copy safe: every version is signed with it, and the hubs
   that installed your plugin refuse updates signed by another key.
   ```sh
   minisign -G -W -s my-plugin.key -p minisign.pub
   gh secret set MINISIGN_KEY < my-plugin.key
   ```
2. Describe your plugin in `registry.json`: its name and description in the app, the
   permissions of its manifest, and the settings the app asks at install (`key`, `label`,
   `secret`, `required`; secrets are read with `secret()`, the others in `init`).
3. Release: set the version in `Cargo.toml`, then
   ```sh
   git tag v0.1.0 && git push --tags
   ```
   The release workflow builds, signs and publishes the `.wasm` with `entry.json`.
4. Open a PR on [HomeMaid-plugins](https://github.com/ErwanHeschung/HomeMaid-plugins) adding
   `entry.json` to `index.json` (a new version: only its `versions` item).
