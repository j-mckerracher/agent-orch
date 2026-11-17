use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use orchestrator_core::load_gate::LoadLimits;
use orchestrator_store::{McpPatch, SettingsPatch};

use super::load::tests::{at, close_gate, cpu};
use super::*;
use crate::executor::load_feed_enabled;

fn new_api() -> (tempfile::TempDir, DaemonApi) {
    let directory = tempfile::tempdir().unwrap();
    let api = DaemonApi::new(directory.path()).unwrap();
    (directory, api)
}

fn get(api: &DaemonApi) -> Value {
    api.dispatch("settings.get", Value::Null).unwrap()
}

fn set(api: &DaemonApi, key: &str, load: Value) -> Result<Value, RpcError> {
    api.dispatch(
        "settings.set",
        json!({"idempotency_key": key, "load": load}),
    )
}

#[test]
fn dispatch_defaults_reject_invalid_settings_without_partial_writes() {
    let (_directory, api) = new_api();
    let before = get(&api);
    for (index, defaults) in [
        json!({"default_agent": "unknown", "providers": {}}),
        json!({"default_agent": "Codex", "providers": {}}),
        json!({"default_agent": null, "providers": {"codex": {}}}),
        json!({"default_agent": null, "providers": {"codex": {"model": "--flag"}}}),
        json!({"default_agent": null, "providers": {}, "extra": true}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            api.dispatch(
                "settings.set",
                json!({
                    "idempotency_key": format!("defaults-bad-{index}"),
                    "load": {"cpu_percent": 70},
                    "dispatch_defaults": defaults,
                })
            ),
            Err(RpcError::InvalidParams)
        );
        assert_eq!(get(&api), before);
    }
}

#[test]
fn settings_get_returns_limits_and_the_gate_with_no_path() {
    let (directory, api) = new_api();

    let body = get(&api);

    assert_eq!(
        body["load"],
        json!({"cpu_percent": 80, "memory_percent": 85, "disk_percent": 90})
    );
    assert_eq!(body["gate"]["state"], "open");
    assert_eq!(body["gate"]["hold"], Value::Null);
    assert_eq!(
        body["gate"]["meters"],
        json!([
            {"meter": "cpu", "state": "unknown", "reason": "warming_up"},
            {"meter": "memory", "state": "unknown", "reason": "warming_up"},
            {"meter": "disk", "state": "unknown", "reason": "warming_up"},
        ])
    );

    close_gate(&api.config.load);
    let closed = get(&api);
    assert_eq!(closed["gate"]["state"], "closed");
    assert_eq!(
        closed["gate"]["hold"],
        json!({"meter": "cpu", "mean_percent": 95, "limit_percent": 80, "reopen_percent": 70})
    );
    assert_eq!(
        closed["gate"]["meters"][0],
        json!({"meter": "cpu", "state": "mean", "mean_percent": 95})
    );
    assert_eq!(
        closed["gate"]["meters"][1],
        json!({"meter": "memory", "state": "unknown", "reason": "unavailable"})
    );

    api.config
        .load
        .set_limits(LoadLimits::new(100, 85, 90).unwrap());
    assert_eq!(
        get(&api)["gate"]["meters"][0],
        json!({"meter": "cpu", "state": "off"})
    );

    for text in [body.to_string(), closed.to_string()] {
        assert!(!text.contains(directory.path().to_str().unwrap()), "{text}");
        assert!(!text.contains('/') && !text.contains('\\'), "{text}");
    }
    assert_eq!(
        api.dispatch("settings.get", json!({"x": 1})),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn settings_get_never_changes_the_gate() {
    let (_directory, api) = new_api();
    let load = api.config.load.clone();
    let wakes = Arc::new(AtomicUsize::new(0));
    let seen = wakes.clone();
    load.on_open(move || {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(95.0), at(t0, n));
    }
    let before = get(&api);
    for _ in 0..5 {
        assert_eq!(get(&api), before);
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
    // The sampler's next samples, not a poll, report the open to the executor.
    for n in 5..12 {
        load.record(cpu(0.0), at(t0, n));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
}

#[test]
fn settings_set_changes_the_limits_and_the_gate_reads_them_from_the_next_sample() {
    let (_directory, api) = new_api();
    let load = api.config.load.clone();
    let t0 = Instant::now();
    for n in 0..5 {
        load.record(cpu(85.0), at(t0, n));
    }
    assert_eq!(get(&api)["gate"]["state"], "closed");

    let body = set(&api, "k-1", json!({"cpu_percent": 100, "disk_percent": 70})).unwrap();

    assert_eq!(
        body["load"],
        json!({"cpu_percent": 100, "memory_percent": 85, "disk_percent": 70})
    );
    // No restart and no apply step: the gate is open on the same body.
    assert_eq!(body["gate"]["state"], "open");
    assert_eq!(body["gate"]["meters"][0]["state"], "off");
    load.record(cpu(85.0), at(t0, 5));
    assert_eq!(get(&api)["gate"]["state"], "open");
    // Persisted: the store holds what the gate uses.
    let stored = api.store.orchestrator_settings().unwrap().load;
    assert_eq!((stored.cpu, stored.memory, stored.disk), (100, 85, 70));
}

#[test]
fn settings_set_refuses_values_out_of_range_unknown_fields_and_an_empty_load() {
    let (_directory, api) = new_api();
    let refused = [
        json!({}),
        json!({"cpu_percent": 49}),
        json!({"memory_percent": 101}),
        json!({"disk_percent": 0}),
        json!({"cpu_percent": 85.5}),
        json!({"cpu_percent": "80"}),
        json!({"cpu_percent": -1}),
        json!({"cpu_percent": 256}),
        json!({"cpu_percent": 60, "gpu_percent": 60}),
    ];
    for (index, load) in refused.into_iter().enumerate() {
        assert_eq!(
            set(&api, &format!("k-{index}"), load.clone()),
            Err(RpcError::InvalidParams),
            "{load}"
        );
    }
    assert_eq!(
        api.dispatch(
            "settings.set",
            json!({"idempotency_key": "k-x", "load": {"cpu_percent": 60}, "extra": 1})
        ),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        api.dispatch("settings.set", json!({"load": {"cpu_percent": 60}})),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        set(&api, "", json!({"cpu_percent": 60})),
        Err(RpcError::InvalidParams)
    );
    // Nothing changed, in the store or the gate.
    assert_eq!(
        api.store.orchestrator_settings().unwrap().load,
        LoadLimits::default()
    );
    assert_eq!(get(&api)["load"]["cpu_percent"], 80);
    // The bounds themselves are allowed.
    assert!(set(&api, "k-low", json!({"cpu_percent": 50})).is_ok());
    assert!(set(&api, "k-high", json!({"cpu_percent": 100})).is_ok());
}

#[test]
fn a_failed_store_write_changes_no_limit() {
    let (_directory, api) = new_api();
    let load = api.config.load.clone();
    close_gate(&load);
    let params: super::settings::LoadParams =
        serde_json::from_value(json!({"cpu_percent": 100})).unwrap();

    let failed = api.adopt(
        Err(StoreError::Io(std::io::Error::other("disk is full"))),
        &params,
    );

    assert_eq!(failed, Err(RpcError::Internal));
    assert_eq!(load.read(Instant::now()).limits, LoadLimits::default());
    assert_eq!(get(&api)["gate"]["state"], "closed");
}

#[test]
fn settings_set_replays_the_same_key_without_a_second_write() {
    let (_directory, api) = new_api();
    let first = set(&api, "k-same", json!({"cpu_percent": 60})).unwrap();
    assert_eq!(first["load"]["cpu_percent"], 60);

    set(&api, "k-other", json!({"cpu_percent": 70})).unwrap();
    let replay = set(&api, "k-same", json!({"cpu_percent": 60})).unwrap();

    // The stored reply, and the later change is not undone.
    assert_eq!(replay, first);
    assert_eq!(api.store.orchestrator_settings().unwrap().load.cpu, 70);
    assert_eq!(get(&api)["load"]["cpu_percent"], 70);
    // The same key with other values is not a replay.
    assert!(set(&api, "k-same", json!({"cpu_percent": 65})).is_err());
    assert_eq!(api.store.orchestrator_settings().unwrap().load.cpu, 70);
}

#[test]
fn settings_set_writes_one_diagnostics_line_with_field_names_and_numbers_only() {
    let (_directory, mut api) = new_api();
    api.config.diagnostics = orchestrator_diagnostics::Diagnostics::memory();

    set(
        &api,
        "k-log",
        json!({"memory_percent": 75, "disk_percent": 100}),
    )
    .unwrap();

    let lines = api.config.diagnostics.lines();
    let changed: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line["event"] == "settings_changed")
        .collect();
    assert_eq!(changed.len(), 1, "{lines:?}");
    assert_eq!(changed[0]["memory_percent"], "75");
    assert_eq!(changed[0]["disk_percent"], "100");
    let text = changed[0].to_string();
    assert!(
        !text.contains("cpu_percent") && !text.contains("k-log"),
        "{text}"
    );
}

#[test]
fn the_limits_survive_a_daemon_restart() {
    let directory = tempfile::tempdir().unwrap();
    {
        let api = DaemonApi::new(directory.path()).unwrap();
        set(
            &api,
            "k-1",
            json!({"cpu_percent": 55, "memory_percent": 100}),
        )
        .unwrap();
    }

    let api = DaemonApi::new(directory.path()).unwrap();

    assert_eq!(
        get(&api)["load"],
        json!({"cpu_percent": 55, "memory_percent": 100, "disk_percent": 90})
    );
    assert_eq!(api.config.load.read(Instant::now()).limits.cpu, 55);
}

#[test]
fn settings_get_reports_the_feed_and_the_environment_switch_turns_it_off() {
    let (_directory, mut api) = new_api();
    // Unit tests never feed the gate from the real sampler.
    assert!(!api.config.load_feed);
    assert_eq!(get(&api)["feed"], "disabled_by_environment");
    api.config.load_feed = true;
    assert_eq!(get(&api)["feed"], "sampler");

    assert!(load_feed_enabled(None, false));
    assert!(load_feed_enabled(Some("on"), false));
    assert!(!load_feed_enabled(Some("off"), false));
    assert!(!load_feed_enabled(None, true));
}

#[test]
fn settings_are_advertised_as_a_capability() {
    let (_directory, api) = new_api();
    assert!(api.capabilities().contains(&Capability::Settings));
}

fn set_mcp(api: &DaemonApi, key: &str, mcp: Value) -> Result<Value, RpcError> {
    api.dispatch("settings.set", json!({"idempotency_key": key, "mcp": mcp}))
}

#[test]
fn mcp_setting_is_always_present_and_off_by_default() {
    let (_directory, api) = new_api();
    assert_eq!(get(&api)["mcp"], json!({"enabled": false}));
    assert!(!api.store.orchestrator_settings().unwrap().mcp.enabled);
}

#[test]
fn mcp_setting_turns_on_and_off_and_leaves_the_other_groups_alone() {
    let (_directory, api) = new_api();
    let before = get(&api);

    let on = set_mcp(&api, "mcp-1", json!({"enabled": true})).unwrap();
    assert_eq!(on["mcp"], json!({"enabled": true}));
    assert!(api.store.orchestrator_settings().unwrap().mcp.enabled);
    for group in ["load", "supervision", "wake_limits", "chat_pr_delivery"] {
        assert_eq!(on[group], before[group], "{group} changed");
    }

    // A change to another group keeps the setting.
    set(&api, "mcp-2", json!({"cpu_percent": 60})).unwrap();
    assert_eq!(get(&api)["mcp"], json!({"enabled": true}));

    let off = set_mcp(&api, "mcp-3", json!({"enabled": false})).unwrap();
    assert_eq!(off["mcp"], json!({"enabled": false}));
}

#[test]
fn mcp_setting_refuses_an_empty_group_a_non_boolean_and_unknown_fields() {
    let (_directory, api) = new_api();
    for (key, mcp) in [
        ("mcp-e", json!({})),
        ("mcp-n", json!({"enabled": null})),
        ("mcp-s", json!({"enabled": "true"})),
        ("mcp-i", json!({"enabled": 1})),
        ("mcp-u", json!({"enabled": true, "tools": ["tasks.get"]})),
    ] {
        assert_eq!(
            set_mcp(&api, key, mcp.clone()),
            Err(RpcError::InvalidParams),
            "{mcp}"
        );
    }
    assert_eq!(get(&api)["mcp"], json!({"enabled": false}));
}

#[test]
fn mcp_setting_replays_the_same_key_without_a_second_write() {
    let (_directory, api) = new_api();
    let first = set_mcp(&api, "mcp-r", json!({"enabled": true})).unwrap();
    api.store
        .update_orchestrator_settings(&SettingsPatch {
            mcp: McpPatch {
                enabled: Some(false),
            },
            ..SettingsPatch::default()
        })
        .unwrap();
    let replay = set_mcp(&api, "mcp-r", json!({"enabled": true})).unwrap();
    assert_eq!(replay, first);
    assert!(!api.store.orchestrator_settings().unwrap().mcp.enabled);
}
