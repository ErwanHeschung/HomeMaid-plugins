//! End to end: the plugin compiled to wasm32-wasip2, on a real hub, against a fake P300 speaking KLAP v2.

mod common;

use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use common::{Plugin, start, start_with, wait_for};
use homemaid::model::{CapKind, CapValue, CommandStatus, entity_uuid};
use serde_json::{Value, json};
use tplink_plugin::klap;

const USER: &str = "me@example.com";
const PASSWORD: &str = "good";
/// device_id and base64 nickname of each outlet.
const OUTLETS: [(&str, &str); 3] = [("CHILD1", "TGFtcGU="), ("CHILD2", "UmFkaWF0ZXVy"), ("CHILD3", "")];

#[derive(Default)]
struct Device {
    on: [bool; 3],
    /// Seeds of each session, by cookie, once handshake2 succeeded.
    sessions: HashMap<String, (Vec<u8>, Vec<u8>, bool)>,
    handshake1: u32,
    handshake2: u32,
    /// Next request is answered 403, as when the device expired the session.
    reject_next: bool,
    /// Schedule rules by outlet, as the Tapo app keeps them.
    rules: HashMap<String, Vec<Value>>,
    next_rule: u32,
}

type Shared = Arc<Mutex<Device>>;

fn auth() -> [u8; 32] {
    klap::auth_hash(USER, PASSWORD)
}

fn session_of(device: &Device, headers: &HeaderMap) -> Option<(String, Vec<u8>, Vec<u8>, bool)> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?.strip_prefix("TP_SESSIONID=")?.to_string();
    let (local, remote, confirmed) = device.sessions.get(&cookie)?.clone();
    Some((cookie, local, remote, confirmed))
}

async fn handshake1(State(device): State<Shared>, body: Bytes) -> Response {
    let mut device = device.lock().unwrap();
    device.handshake1 += 1;
    let remote = vec![device.handshake1 as u8; 16];
    let cookie = format!("S{}", device.handshake1);
    let mut reply = remote.clone();
    reply.extend(klap::server_hash(&body, &remote, &auth()));
    device.sessions.insert(cookie.clone(), (body.to_vec(), remote, false));
    ([(header::SET_COOKIE, format!("TP_SESSIONID={cookie};TIMEOUT=86400"))], reply).into_response()
}

async fn handshake2(State(device): State<Shared>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let mut device = device.lock().unwrap();
    let Some((cookie, local, remote, _)) = session_of(&device, &headers) else { return StatusCode::FORBIDDEN };
    if body[..] != klap::client_hash(&local, &remote, &auth()) {
        return StatusCode::BAD_REQUEST;
    }
    device.handshake2 += 1;
    device.sessions.insert(cookie, (local, remote, true));
    StatusCode::OK
}

async fn request(
    State(device): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let mut device = device.lock().unwrap();
    let Some((_, local, remote, true)) = session_of(&device, &headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if std::mem::take(&mut device.reject_next) {
        device.sessions.clear();
        return StatusCode::FORBIDDEN.into_response();
    }
    let seq: i32 = query["seq"].parse().unwrap();
    let session = klap::Session::new(&local, &remote, &auth());
    assert!(session.verify(seq, &body), "request signature");
    let request: Value = serde_json::from_slice(&session.decrypt(seq, &body).unwrap()).unwrap();
    assert!(request["terminal_uuid"].is_string() && request["request_time_milis"].is_number());

    let result = match request["method"].as_str().unwrap() {
        "get_device_info" => json!({ "device_id": "P300DEV", "model": "P300" }),
        "get_child_device_list" => {
            let children: Vec<Value> = OUTLETS
                .iter()
                .zip(device.on)
                .enumerate()
                .map(|(i, ((id, nickname), on))| {
                    json!({ "device_id": id, "nickname": nickname, "position": i + 1, "device_on": on, "model": "P300" })
                })
                .collect();
            json!({ "child_device_list": children, "start_index": 0, "sum": 3 })
        }
        "control_child" => {
            let params = &request["params"];
            let outlet = params["device_id"].as_str().unwrap().to_string();
            let inner = &params["requestData"]["params"];
            let result = match params["requestData"]["method"].as_str().unwrap() {
                "set_device_info" => {
                    let index = OUTLETS.iter().position(|(id, _)| outlet == *id).unwrap();
                    device.on[index] = inner["device_on"].as_bool().unwrap();
                    json!({})
                }
                // Pages of 2, to exercise paging.
                "get_schedule_rules" => {
                    let rules = device.rules.get(&outlet).cloned().unwrap_or_default();
                    let start = inner["start_index"].as_u64().unwrap() as usize;
                    json!({ "rule_list": rules.iter().skip(start).take(2).collect::<Vec<_>>(), "start_index": start, "sum": rules.len() })
                }
                "add_schedule_rule" => {
                    for field in ["enable", "mode", "week_day", "s_type", "s_min", "time_offset", "e_type", "e_action", "desired_states", "year"] {
                        assert!(inner.get(field).is_some(), "rule without {field}: {inner}");
                    }
                    device.next_rule += 1;
                    let mut rule = inner.clone();
                    rule["id"] = json!(format!("S{}", device.next_rule));
                    device.rules.entry(outlet).or_default().push(rule);
                    json!({})
                }
                "remove_schedule_rules" => {
                    let gone: Vec<Value> = inner["rule_list"].as_array().unwrap().iter().map(|r| r["id"].clone()).collect();
                    device.rules.entry(outlet).or_default().retain(|r| !gone.contains(&r["id"]));
                    json!({})
                }
                method => panic!("unexpected child method {method}"),
            };
            json!({ "responseData": { "error_code": 0, "result": result } })
        }
        method => panic!("unexpected method {method}"),
    };
    session.seal(seq, json!({ "error_code": 0, "result": result }).to_string().as_bytes()).into_response()
}

async fn fake_p300() -> (Shared, SocketAddr) {
    let device = Shared::default();
    let app = Router::new()
        .route("/app/handshake1", post(handshake1))
        .route("/app/handshake2", post(handshake2))
        .route("/app/request", post(request))
        .with_state(device.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (device, addr)
}

/// The plugin with strips the user added, each by its address.
fn tplink(hosts: &[String], password: &str) -> Plugin<'static> {
    let devices: Vec<(&'static str, &'static [(&'static str, &'static str)])> = hosts
        .iter()
        .enumerate()
        .map(|(i, host)| {
            let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
            let settings: &'static [(&str, &str)] =
                Box::leak(Box::new([("host", leak(host.clone())), ("username", USER), ("password", leak(password.into()))]));
            (leak(format!("strip{i}")), settings)
        })
        .collect();
    Plugin {
        id: "tplink",
        package: "tplink-plugin",
        // The fake P300 listens on loopback.
        permissions: &["local-network", "loopback"],
        settings: &[],
        secrets: &[],
        devices: Box::leak(devices.into_boxed_slice()),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn outlets_are_discovered_and_controlled() {
    let (device, addr) = fake_p300().await;
    let (core, _data) = start(tplink(&[addr.to_string()], PASSWORD));
    let outlet = |i: usize| entity_uuid("tplink", OUTLETS[i].0);
    let slot = |i: usize| core.with(|r| r.entities[&outlet(i)].caps[&CapKind::OnOff].clone());

    wait_for("discovery", Duration::from_secs(10), || {
        core.with(|r| r.entities.len() == 3 && r.entities.values().all(|e| e.available))
    })
    .await;
    let names: Vec<String> = (0..3).map(|i| core.with(|r| r.entities[&outlet(i)].name.clone())).collect();
    assert_eq!(names, ["Lampe", "Radiateur", "P300 prise 3"]);
    assert_eq!(core.with(|r| r.entities[&outlet(0)].device.clone()).as_deref(), Some("P300DEV"));
    assert_eq!(core.with(|r| r.entities[&outlet(0)].added.clone()).as_deref(), Some("strip0"), "the strip it came with");

    core.with(|r| r.set_desired(outlet(1), &CapKind::OnOff, CapValue::OnOff(true), Instant::now())).unwrap();
    wait_for("confirmation", Duration::from_secs(5), || slot(1).status == CommandStatus::Confirmed).await;
    assert!(device.lock().unwrap().on[1]);

    // Pressed on the device itself: reported by the next refresh and adopted as desired.
    device.lock().unwrap().on[0] = true;
    wait_for("physical change", Duration::from_secs(5), || slot(0).desired == Some(CapValue::OnOff(true))).await;

    // The device dropped the session: a new handshake is done transparently.
    let before = device.lock().unwrap().handshake2;
    device.lock().unwrap().reject_next = true;
    core.with(|r| r.set_desired(outlet(2), &CapKind::OnOff, CapValue::OnOff(true), Instant::now())).unwrap();
    wait_for("confirmation after 403", Duration::from_secs(5), || slot(2).status == CommandStatus::Confirmed).await;
    assert!(device.lock().unwrap().handshake2 > before);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_strip_is_added_live_and_a_wrong_password_refused() {
    let (device, addr) = fake_p300().await;
    let (core, _data) = start(tplink(&[], PASSWORD));
    let plugins = core.plugins.get().unwrap();
    let started = || plugins.states()["tplink"].version.is_some();
    wait_for("the plugin, with no strip", Duration::from_secs(10), started).await;
    let form = |password: &str| BTreeMap::from([("host", addr.to_string()), ("username", USER.into()), ("password", password.into())].map(|(k, v)| (k.to_string(), v)));

    let refused = plugins.add_device("tplink", "strip0", form("bad")).await.unwrap_err();
    assert!(refused.contains("refused the login"), "said in the form: {refused}");
    assert_eq!(device.lock().unwrap().handshake2, 0, "the client stops at the handshake1 hash check");
    assert!(core.with(|r| r.entities.is_empty()));

    plugins.add_device("tplink", "strip0", form(PASSWORD)).await.unwrap();
    assert_eq!(core.with(|r| r.entities.values().filter(|e| e.added.as_deref() == Some("strip0")).count()), 3, "its outlets at once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_strip_refused_at_start_is_said_and_the_plugin_runs() {
    let (_device, addr) = fake_p300().await;
    let (core, _data) = start(tplink(&[addr.to_string()], "bad"));
    let plugins = core.plugins.get().unwrap();
    wait_for("the plugin", Duration::from_secs(10), || plugins.states()["tplink"].version.is_some()).await;
    let error = plugins.states()["tplink"].devices.get("strip0").cloned().unwrap_or_default();
    assert!(error.contains("refused the login"), "{error}");
    assert!(core.with(|r| r.entities.is_empty()));
}

#[tokio::test(flavor = "multi_thread")]
async fn silent_device_does_not_hang_the_plugin() {
    // Accepts connections and never answers.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let hosts = [listener.local_addr().unwrap().to_string()];
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let held = accepted.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            held.lock().unwrap().push(stream.unwrap());
        }
    });
    let (core, _data) = start(tplink(&hosts, PASSWORD));
    // Read timeout (5 s): the strip is left out, and the plugin runs.
    let plugins = core.plugins.get().unwrap();
    wait_for("the plugin, without the strip", Duration::from_secs(15), || plugins.states()["tplink"].version.is_some()).await;
    assert!(plugins.states()["tplink"].devices.contains_key("strip0"));
    assert!(!accepted.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unplugged_strip_does_not_hide_the_others() {
    let (_device, addr) = fake_p300().await;
    let unplugged = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let (core, _data) = start(tplink(&[unplugged.to_string(), addr.to_string()], PASSWORD));
    wait_for("discovery of the strip that answers", Duration::from_secs(10), || {
        core.with(|r| r.entities.len() == 3 && r.entities.values().all(|e| e.available))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn schedules_become_rules_on_the_strip_next_to_the_users() {
    let (device, addr) = fake_p300().await;
    // A rule made in the Tapo app, and one this plugin added at an earlier start.
    let theirs = json!({ "id": "APP1", "s_type": "normal", "s_min": 600 });
    device.lock().unwrap().rules.insert("CHILD1".into(), vec![theirs.clone(), json!({ "id": "OLD", "s_type": "normal" })]);
    #[derive(serde::Deserialize)]
    struct File {
        automation: Vec<homemaid::automations::AutomationConfig>,
    }
    let file: File = toml::from_str(r#"
        [[automation]]
        id = "dusk"
        [[automation.trigger]]
        at = "sunset-15"
        days = ["mon", "fri"]
        [[automation.action]]
        set = { select = 'name:"Lampe"', cap = "on_off", value = true }
        [[automation.trigger]]
        at = "07:30"
    "#).unwrap();
    let paris = homemaid::automations::Location { latitude: 48.86, longitude: 2.35 };
    let automations = homemaid::automations::Automations::new(&file.automation, Some(paris)).unwrap();
    let (core, data) = start_with(tplink(&[addr.to_string()], PASSWORD), automations);
    // As if the plugin had added "OLD" before; written while the plugin still compiles (seconds).
    let kv = data.0.join("plugins/tplink.kv.json");
    std::fs::write(&kv, json!({ "schedules:CHILD1": json!({ "want": [], "ids": ["OLD"], "synced": false }).to_string() }).to_string()).unwrap();

    let lamp = entity_uuid("tplink", "CHILD1");
    wait_for("discovery", Duration::from_secs(10), || core.with(|r| r.entities.get(&lamp).is_some_and(|e| e.available))).await;
    let rules = device.lock().unwrap().rules["CHILD1"].clone();
    assert_eq!(rules.len(), 3, "the app's rule, plus two: {rules:?}");
    assert_eq!(rules[0], theirs, "the app's rule is left alone; the plugin's old one is replaced");
    let dusk = &rules[1];
    assert_eq!((dusk["s_type"].as_str(), dusk["time_offset"].as_i64()), (Some("sunset"), Some(-15)));
    assert_eq!(dusk["week_day"], 0b010_0010, "monday and friday, sunday = bit 0");
    assert_eq!(dusk["desired_states"]["on"], true);
    assert_eq!((rules[2]["s_type"].as_str(), rules[2]["s_min"].as_i64()), (Some("normal"), Some(450)));
    assert!(device.lock().unwrap().rules.get("CHILD2").is_none_or(Vec::is_empty), "not this outlet's automation");

    let list = core.automations().list(&core, &jiff::Zoned::now());
    assert_eq!(list[0]["delegated"], json!(["Lampe"]));
}
