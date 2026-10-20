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

1. Make a signing key once, and keep it safe: it signs every version, and the hubs that
   installed your plugin refuse updates signed by another key.
   ```sh
   minisign -G
   ```
2. Sign the build and attach both files to a GitHub release:
   ```sh
   minisign -Sm target/wasm32-wasip2/release/{{crate_name}}.wasm
   gh release create v0.1.0 target/wasm32-wasip2/release/{{crate_name}}.wasm*
   ```
3. Open a PR on [homemaid-plugins](https://github.com/ErwanHeschung/homemaid-plugins) adding your
   entry to `index.json` (format in its README): `sha256sum` of the `.wasm`, the content of the
   `.minisig`, your public key (second line of `minisign.pub`).
