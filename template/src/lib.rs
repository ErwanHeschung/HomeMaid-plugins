//! A HomeMaid plugin: one switch, remembered in the plugin's private store. Replace it with your
//! device: find it in `discover`, send it commands in `apply`, read it back in `refresh`.
//!
//! The hub gives a plugin nothing but what it asks for in its manifest: sockets to the local
//! network (`local-network`), timers, logs, its settings, its secrets and a small key-value store.

use std::collections::BTreeMap;

use homemaid_plugin::{CapKind, CapMeta, CapValue, DiscoveredEntity, LOCAL_NETWORK, LogLevel, Manifest, Plugin, report_state};

struct MyPlugin;

impl Plugin for MyPlugin {
    fn manifest() -> Manifest {
        // The permissions must match the registry entry: the user accepts those.
        Manifest { id: "{{plugin_id}}".into(), version: env!("CARGO_PKG_VERSION").into(), permissions: vec![LOCAL_NETWORK.into()] }
    }

    /// `settings` are `[plugin.settings]` of homemaid.toml (what the app asked at install);
    /// secrets are read with `homemaid_plugin::secret("password")`.
    fn init(settings: BTreeMap<String, String>) -> Result<(), String> {
        homemaid_plugin::log(LogLevel::Info, &format!("started with {} settings", settings.len()));
        Ok(())
    }

    /// One entity per thing the user controls. `local_id` must stay the same across restarts:
    /// the hub derives the entity's stable id from it.
    fn discover() -> Result<Vec<DiscoveredEntity>, String> {
        Ok(vec![DiscoveredEntity {
            local_id: "switch".into(),
            name: "Example switch".into(),
            device: None,
            caps: vec![(CapKind::OnOff, CapMeta::default())],
        }])
    }

    /// The hub wants `kind` to be `value`. Send it to the device; the command is confirmed once
    /// `report_state` says so (here at once, usually from `refresh`).
    fn apply(local_id: &str, kind: CapKind, value: CapValue) -> Result<(), String> {
        let CapValue::OnOff(on) = value else { return Err(format!("{local_id}: cannot set {kind}")) };
        homemaid_plugin::kv_set(local_id, if on { "on" } else { "off" });
        report_state(local_id, &kind, &value);
        Ok(())
    }

    /// Reads the devices back. Returns when to be called again, in ms.
    fn refresh() -> u32 {
        let on = homemaid_plugin::kv_get("switch").is_some_and(|v| v == "on");
        report_state("switch", &CapKind::OnOff, &CapValue::OnOff(on));
        5000
    }
}

homemaid_plugin::export!(MyPlugin with_types_in homemaid_plugin::bindings);
