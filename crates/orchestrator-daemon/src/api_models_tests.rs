//! M7-09: agent catalog, selection validation and effective configuration,
//! against fake agent CLIs (shell scripts, so Unix only and not Windows
//! evidence).

use super::*;

const MODELS: &str = r#"{"models":[
    {"slug":"gpt-6-luna","display_name":"GPT-6 Luna","visibility":"list",
     "default_reasoning_level":"medium",
     "supported_reasoning_levels":[{"effort":"low","description":"x"},{"effort":"medium","description":"y"}]},
    {"slug":"gpt-6-sol","display_name":"GPT-6 Sol","visibility":"list",
     "supported_reasoning_levels":[{"effort":"high","description":"x"}]},
    {"slug":"hidden-model","visibility":"hide","supported_reasoning_levels":[]}
]}"#;
const CLAUDE_HELP: &str =
    "Options:\n  --effort <level>    Effort level\n                      (low, medium, high)\n\
     -p\n--verbose\n--output-format\n--include-partial-messages\n--permission-mode\n\
     --permission-prompts\n--restricted\n--strict-mcp-config\n--tools\n--allowedTools\n--model\n--plugin-dir\n--agent\n";

/// A fake `copilot --acp` that lists `gpt-a` (levels low and high) and `mini` (no levels).
const COPILOT_ACP: &str = r#"if [ "$1" = "--version" ]; then printf '%s\n' 'GitHub Copilot CLI 1.0.114.'; exit 0; fi
if [ "$1" != "--acp" ]; then exit 1; fi
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id" ;;
    *'"session/new"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"s1","models":{"availableModels":[{"modelId":"gpt-a","name":"GPT A"},{"modelId":"mini","name":"Mini"}]}}}\n' "$id" ;;
    *'"value":"gpt-a"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"configOptions":[{"id":"reasoning_effort","currentValue":"high","options":[{"value":"low"},{"value":"high"}]}]}}\n' "$id" ;;
    *'"session/set_config_option"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"configOptions":[]}}\n' "$id" ;;
    *'"session/close"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"; exit 0 ;;
  esac
done"#;

struct Agents {
    codex: PathBuf,
    claude: PathBuf,
    codex_argv: PathBuf,
    claude_argv: PathBuf,
    signed_out: PathBuf,
}

/// Fake codex and claude CLIs. Creating the `signed_out` marker file makes
/// codex report that no account is signed in.
fn agents(fixture: &Fixture) -> Agents {
    let models = fixture.marker("models.json");
    fs::write(&models, MODELS).unwrap();
    let help = fixture.marker("claude-help.txt");
    fs::write(&help, CLAUDE_HELP).unwrap();
    let signed_out = fixture.marker("codex-signed-out");
    let codex_argv = fixture.marker("codex-argv.txt");
    let claude_argv = fixture.marker("claude-argv.txt");
    let codex = fixture.script(
        "fake-codex",
        &format!(
            "case \"$1\" in\n\
             login) if [ -f '{signed_out}' ]; then echo 'Not logged in'; exit 1; fi; echo 'Logged in using ChatGPT'; exit 0;;\n\
             debug) cat '{models}'; exit 0;;\n\
             esac\n\
             for a in \"$@\"; do printf '%s\\n' \"$a\" | sed -n 1p; done > '{argv}.tmp' && mv '{argv}.tmp' '{argv}'\n\
             echo produced > agent-output.txt\n\
             {THREAD_STARTED}{TURN_COMPLETED}",
            signed_out = signed_out.display(),
            models = models.display(),
            argv = codex_argv.display(),
        ),
    );
    let claude = fixture.script(
        "fake-claude",
        &format!(
            "case \"$1\" in\n\
             auth) echo '{{\"loggedIn\":true}}'; exit 0;;\n\
             --help) cat '{help}'; exit 0;;\n\
             esac\n\
             for a in \"$@\"; do printf '%s\\n' \"$a\" | sed -n 1p; done > '{argv}.tmp' && mv '{argv}.tmp' '{argv}'\n\
             echo '{{}}'\n",
            help = help.display(),
            argv = claude_argv.display(),
        ),
    );
    Agents {
        codex,
        claude,
        codex_argv,
        claude_argv,
        signed_out,
    }
}

fn api_for(fixture: &Fixture, agents: &Agents) -> DaemonApi {
    fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(agents.claude.clone());
    })
}

fn save_dispatch_defaults(api: &DaemonApi, key: &str, defaults: Value) -> Result<Value, RpcError> {
    api.dispatch(
        "settings.set",
        json!({"idempotency_key": key, "dispatch_defaults": defaults}),
    )
}

fn dispatch_defaults() -> Value {
    json!({
        "default_agent": "codex",
        "providers": {
            "codex": {"model": "gpt-6-luna", "effort": "low"},
            "claude-code": {"model": "sonnet", "effort": "medium"}
        }
    })
}

#[test]
fn dispatch_defaults_reach_the_agent_and_survive_restart_without_changing_existing_tasks() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);
    save_dispatch_defaults(&api, "defaults-save", dispatch_defaults()).unwrap();
    let submitted = submit_with(&fixture, &api, "defaults-task", "", Value::Null).unwrap();
    let task_id = submitted["contract"]["id"].as_str().unwrap();
    assert_eq!(submitted["contract"]["agent"], "codex");
    assert_eq!(
        submitted["contract"]["model_selection"],
        dispatch_defaults()["providers"]["codex"]
    );
    wait_state(&api, task_id, "Succeeded");
    let argv = fs::read_to_string(&agents.codex_argv).unwrap();
    assert!(argv.contains("-m\ngpt-6-luna\n"), "{argv}");
    assert!(argv.contains("model_reasoning_effort=\"low\""), "{argv}");
    drop(api);
    let api = api_for(&fixture, &agents);
    assert_eq!(
        api.dispatch("settings.get", Value::Null).unwrap()["dispatch_defaults"],
        dispatch_defaults()
    );
    let changed = json!({"default_agent": "claude-code", "providers": {}});
    save_dispatch_defaults(&api, "defaults-change", changed).unwrap();
    assert_eq!(
        task(&api, task_id)["contract"]["model_selection"]["model"],
        "gpt-6-luna"
    );
    let next = submit_with(&fixture, &api, "defaults-next", "", Value::Null).unwrap();
    assert_eq!(next["contract"]["agent"], "claude-code");
    assert!(next["contract"]["model_selection"].is_null());
    save_dispatch_defaults(
        &api,
        "defaults-clear",
        json!({"default_agent":null,"providers":{}}),
    )
    .unwrap();
    assert_eq!(
        submit_with(&fixture, &api, "defaults-task", "", Value::Null).unwrap(),
        submitted
    );
    assert_eq!(
        submit_with(&fixture, &api, "defaults-missing", "", Value::Null),
        Err(RpcError::InvalidParams)
    );
    assert_eq!(
        submit_with(&fixture, &api, "defaults-task", "claude-code", Value::Null),
        Err(RpcError::IdempotencyConflict)
    );
}

#[test]
fn dispatch_defaults_respect_explicit_choices_and_work_classes_without_mixing_model_efforts() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);
    save_dispatch_defaults(&api, "defaults-save", dispatch_defaults()).unwrap();
    for (key, agent, selection, expected) in [
        (
            "provider",
            "claude-code",
            Value::Null,
            json!({"model":"sonnet","effort":"medium"}),
        ),
        (
            "effort",
            "codex",
            json!({"effort":"medium"}),
            json!({"model":"gpt-6-luna","effort":"medium"}),
        ),
        (
            "model",
            "codex",
            json!({"model":"gpt-6-sol"}),
            json!({"model":"gpt-6-sol"}),
        ),
        (
            "pair",
            "codex",
            json!({"model":"gpt-6-sol","effort":"high"}),
            json!({"model":"gpt-6-sol","effort":"high"}),
        ),
    ] {
        let task = submit_with(&fixture, &api, key, agent, selection).unwrap();
        assert_eq!(task["contract"]["model_selection"], expected, "{key}");
    }
    assert_eq!(
        submit_with(&fixture, &api, "empty-choice", "codex", json!({})),
        Err(RpcError::InvalidParams)
    );
    let classes = api.dispatch("settings.workClasses.set", json!({
        "classes": [{"name": "review", "agent": "codex", "model": "gpt-6-sol", "effort": "high"}]
    })).unwrap();
    assert_eq!(classes["saved"], true);
    let workstream = api
        .dispatch(
            "workstreams.create",
            json!({"title":"Defaults", "idempotency_key":"defaults-ws"}),
        )
        .unwrap();
    for (key, class, expected) in [
        (
            "ws-default",
            None,
            dispatch_defaults()["providers"]["codex"].clone(),
        ),
        (
            "ws-class",
            Some("review"),
            json!({"model":"gpt-6-sol","effort":"high"}),
        ),
    ] {
        let created = api.dispatch("workstreams.add_task", json!({
            "idempotency_key":key, "workstream_id":workstream["id"],
            "contract": {
                "project_id":fixture.project_id(), "objective":"Review changes", "kind":"Investigate",
                "dependencies":[], "verification":"None", "delivery":"ReportOnly",
                "autonomy":"WorkspaceWrites", "max_retries":0, "max_repairs":0, "work_class":class,
            }
        })).unwrap();
        let record = task(&api, created["created"][0].as_str().unwrap());
        assert_eq!(record["contract"]["model_selection"], expected);
    }
}

#[test]
fn dispatch_defaults_refuse_invalid_or_unavailable_catalog_choices() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);
    save_dispatch_defaults(&api, "defaults-save", dispatch_defaults()).unwrap();
    for (key, selection) in [
        ("unknown", json!({"model":"gone"})),
        ("effort", json!({"model":"gpt-6-sol","effort":"low"})),
    ] {
        assert_eq!(
            save_dispatch_defaults(
                &api,
                key,
                json!({
                    "default_agent":"codex","providers":{"codex":selection}
                })
            ),
            Err(RpcError::InvalidParams)
        );
    }
    assert_eq!(
        api.dispatch("settings.get", Value::Null).unwrap()["dispatch_defaults"],
        dispatch_defaults()
    );
    drop(api);
    fs::write(&agents.signed_out, "").unwrap();
    let api = api_for(&fixture, &agents);
    assert_eq!(
        submit_with(&fixture, &api, "unavailable", "", Value::Null),
        Err(RpcError::Conflict)
    );
    assert_eq!(api.dispatch("tasks.list", json!({})).unwrap(), json!([]));
}

fn submit_with(
    fixture: &Fixture,
    api: &DaemonApi,
    key: &str,
    agent: &str,
    selection: Value,
) -> Result<Value, RpcError> {
    let mut contract = json!({
        "project_id": fixture.project_id(),
        "parent_id": null,
        "objective": format!("objective of {key}"),
        "kind": "Implement",
        "agent": agent,
        "dependencies": [],
        "verification": "None",
        "delivery": "ReportOnly",
        "autonomy": "WorkspaceWrites",
        "max_retries": 0,
        "max_repairs": 0,
        // Named, so these tests keep the Restricted launch argv they assert. The task default
        // is Auto, which the fake CLIs here do not advertise.
        "permission": "restricted"
    });
    if !selection.is_null() {
        contract["model_selection"] = selection;
    }
    api.dispatch(
        "tasks.submit",
        json!({"idempotency_key": key, "contract": contract}),
    )
}

fn entry<'a>(catalog: &'a Value, agent: &str) -> &'a Value {
    catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["agent"] == agent)
        .unwrap()
}

fn validate(api: &DaemonApi, agent: &str, model: Value, effort: Value) -> Value {
    api.dispatch(
        "agents.validate_selection",
        json!({"agent": agent, "model": model, "effort": effort}),
    )
    .unwrap()
}

#[test]
fn catalog_lists_only_what_the_installed_clis_report() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);

    let catalog = api.dispatch("agents.catalog", Value::Null).unwrap();

    let codex = entry(&catalog, "codex");
    assert_eq!(codex["provider"], "openai");
    assert_eq!(codex["state"], "available");
    let models = codex["models"]["items"].as_array().unwrap();
    let ids: Vec<_> = models
        .iter()
        .map(|model| model["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["gpt-6-luna", "gpt-6-sol"]);
    assert_eq!(models[0]["efforts"], json!(["low", "medium"]));
    assert_eq!(models[0]["default_effort"], "medium");
    assert_eq!(models[1]["default_effort"], Value::Null);
    assert_eq!(codex["efforts"]["state"], "unsupported");

    let claude = entry(&catalog, "claude-code");
    assert_eq!(claude["provider"], "anthropic");
    assert_eq!(claude["models"]["state"], "available");
    let aliases: Vec<_> = claude["models"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| model["id"].as_str().unwrap())
        .collect();
    assert_eq!(aliases, ["opus", "sonnet", "haiku"]);
    assert!(claude["models"]["detail"]
        .as_str()
        .unwrap()
        .contains("documented"));
    assert_eq!(claude["efforts"]["state"], "available");
    assert_eq!(claude["efforts"]["items"], json!(["low", "medium", "high"]));

    let pi = entry(&catalog, "pi");
    assert_eq!(pi["state"], "unsupported");
    assert_eq!(pi["models"]["items"], json!([]));

    let text = catalog.to_string();
    assert!(!text.contains(&fixture.root.path().display().to_string()));
    assert_eq!(
        api.dispatch("agents.catalog", json!({"x": 1})),
        Err(RpcError::InvalidParams)
    );
}

#[test]
fn signed_out_or_missing_agents_offer_nothing_rather_than_an_empty_list() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    fs::write(&agents.signed_out, "").unwrap();
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(fixture.marker("no-such-claude"));
    });

    let catalog = api.dispatch("agents.catalog", Value::Null).unwrap();

    let codex = entry(&catalog, "codex");
    assert_eq!(codex["state"], "authentication_failed");
    assert_eq!(codex["models"]["state"], "authentication_failed");
    assert_eq!(codex["models"]["items"], json!([]));
    let claude = entry(&catalog, "claude-code");
    assert_eq!(claude["state"], "unavailable");
    assert_eq!(claude["efforts"]["items"], json!([]));
}

#[test]
fn copilot_is_listed_unavailable_and_its_check_records_nothing_when_the_cli_is_missing() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let missing = fixture.marker("no-such-copilot");
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(agents.claude.clone());
        config.copilot_program = Some(missing);
    });

    let catalog = api.dispatch("agents.catalog", Value::Null).unwrap();
    let copilot = entry(&catalog, "github-copilot");
    assert_eq!(copilot["provider"], "github");
    assert_eq!(copilot["state"], "unavailable");
    // A missing CLI makes every facet unavailable, as it does for the others.
    assert_eq!(copilot["models"]["state"], "unavailable");
    assert_eq!(copilot["models"]["items"], json!([]));
    assert_eq!(copilot["efforts"]["state"], "unavailable");

    let check = api
        .dispatch("agents.check_copilot_sign_in", json!({"force": true}))
        .unwrap();
    assert_eq!(check["state"], "not_checked");
    assert_eq!(check["checked_at_ms"], Value::Null);
    assert!(check["reason"].as_str().unwrap().contains("not found"));
    assert_eq!(
        api.dispatch("agents.check_copilot_sign_in", json!({"surprise": 1})),
        Err(RpcError::InvalidParams)
    );

    let capabilities = api.dispatch("agents.capabilities", Value::Null).unwrap();
    let copilot = entry(&capabilities, "github-copilot");
    assert_eq!(copilot["installed"], false);
    assert_eq!(copilot["launchable"], false);
    assert_eq!(copilot["sign_in"]["state"], "not_checked");
}

/// DPES-1105: a Copilot choice is checked against the models its CLI lists. A model it lists is valid, with
/// a level that model supports; one it does not list is refused; a missing CLI is refused with its own reason.
/// Unix only: the fake is a shell script.
#[cfg(unix)]
#[test]
fn a_copilot_choice_is_validated_by_its_catalog_and_names_a_missing_cli() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let copilot = fixture.script("fake-copilot", COPILOT_ACP);
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(agents.claude.clone());
        config.copilot_program = Some(copilot);
    });

    assert_eq!(
        validate(&api, "github-copilot", json!("gpt-a"), json!("high"))["valid"],
        true
    );
    assert_eq!(
        validate(&api, "github-copilot", json!("gpt-a"), json!("max"))["reason"],
        "effort_unsupported"
    );
    assert_eq!(
        validate(&api, "github-copilot", json!("no-such-model"), Value::Null)["reason"],
        "unknown_model"
    );

    let missing = fixture.marker("no-such-copilot");
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(agents.claude.clone());
        config.copilot_program = Some(missing);
    });
    let refused = validate(
        &api,
        "github-copilot",
        json!("claude-haiku-4.5"),
        Value::Null,
    );
    assert_eq!(refused["valid"], false, "{refused}");
    assert_eq!(refused["reason"], "agent_unavailable", "{refused}");
    assert!(
        refused["detail"].as_str().unwrap().contains("not found"),
        "{refused}"
    );
}

/// DPES-1105: the explicit check answers at once and runs on its own thread. The
/// fake CLI takes three seconds, so a request that waited for it would be slow.
/// Unix only: the fake is a shell script.
#[cfg(unix)]
#[test]
fn a_copilot_check_returns_at_once_and_settles_in_the_background() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let slow = fixture.script("fake-copilot-slow", "sleep 3\n");
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(agents.claude.clone());
        config.copilot_program = Some(slow);
    });

    let started = std::time::Instant::now();
    let check = api
        .dispatch("agents.check_copilot_sign_in", json!({"force": false}))
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "the request must not wait for the background run"
    );
    assert_eq!(check["state"], "checking");

    // A second explicit check while the first runs starts no second paid run.
    let again = api
        .dispatch("agents.check_copilot_sign_in", json!({"force": true}))
        .unwrap();
    assert_eq!(again["state"], "checking");

    // The zero-cost status read follows the run to a settled state. A silent
    // exit is never a sign-in fact, so the state must not become ready.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let capabilities = api.dispatch("agents.capabilities", Value::Null).unwrap();
        let state = entry(&capabilities, "github-copilot")["sign_in"]["state"].clone();
        if state != "checking" {
            assert_ne!(state, "ready", "a silent exit must not count as signed in");
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the background check never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// The check RPC returns at once while a fake run is in flight, a second call
/// starts nothing, and the answer reaches `agents.capabilities` when the run ends.
#[cfg(unix)]
#[test]
fn the_copilot_check_returns_at_once_and_its_answer_reaches_capabilities() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let runs = fixture.marker("copilot-runs.txt");
    let stream = fixture.marker("copilot-stream.jsonl");
    fs::write(
        &stream,
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"OK\",\"toolRequests\":[]}}\n\
         {\"type\":\"result\",\"exitCode\":0}\n",
    )
    .unwrap();
    let copilot = fixture.script(
        "fake-copilot",
        &format!(
            "if [ \"$1\" = \"--version\" ]; then\n  printf '%s\\n' 'GitHub Copilot CLI 1.0.109.'\n  exit 0\nfi\n\
             if [ \"$1\" = \"--help\" ]; then\n  printf '%s\\n' --prompt --output-format --log-level --no-auto-update --no-ask-user --no-custom-instructions --disable-builtin-mcps --disallow-temp-dir --usage-output-file -C --allow-tool --deny-tool --model\n  exit 0\nfi\n\
             echo run >> '{runs}'\nsleep 2\ncat '{stream}'\nexit 0\n",
            runs = runs.display(),
            stream = stream.display(),
        ),
    );
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(agents.claude.clone());
        config.copilot_program = Some(copilot.clone());
    });

    let started = std::time::Instant::now();
    let first = api
        .dispatch("agents.check_copilot_sign_in", json!({}))
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the check RPC waited for the run: {:?}",
        started.elapsed()
    );
    assert_eq!(first["state"], "checking");
    let second = api
        .dispatch("agents.check_copilot_sign_in", json!({"force": true}))
        .unwrap();
    assert_eq!(second["state"], "checking");

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let settled = loop {
        let capabilities = api.dispatch("agents.capabilities", Value::Null).unwrap();
        let sign_in = entry(&capabilities, "github-copilot")["sign_in"].clone();
        if sign_in["state"] != "checking" {
            break sign_in;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the check never settled"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(settled["state"], "ready", "{settled}");
    assert!(settled["checked_at_ms"].is_u64(), "{settled}");
    assert_eq!(
        fs::read_to_string(&runs).unwrap().lines().count(),
        1,
        "exactly one run, however many starts"
    );
}

#[test]
fn validate_selection_gives_a_precise_reason_for_every_refusal() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);
    let refused = |agent: &str, model: Value, effort: Value| {
        let result = validate(&api, agent, model, effort);
        assert_eq!(result["valid"], false, "{agent} {result}");
        assert!(result["detail"].as_str().is_some_and(|d| !d.is_empty()));
        result["reason"].as_str().unwrap().to_owned()
    };

    assert_eq!(
        validate(&api, "codex", json!("gpt-6-luna"), json!("low")),
        json!({"valid": true})
    );
    assert_eq!(
        validate(&api, "codex", json!("gpt-6-sol"), Value::Null),
        json!({"valid": true})
    );
    assert_eq!(
        validate(&api, "claude-code", Value::Null, json!("high")),
        json!({"valid": true})
    );
    assert_eq!(
        refused("codex", json!("gpt-9"), Value::Null),
        "unknown_model"
    );
    assert_eq!(
        refused("codex", json!("hidden-model"), Value::Null),
        "unknown_model"
    );
    assert_eq!(
        refused("codex", json!("gpt-6-sol"), json!("low")),
        "effort_unsupported"
    );
    assert_eq!(
        refused("codex", Value::Null, json!("low")),
        "effort_requires_model"
    );
    assert_eq!(
        validate(&api, "claude-code", json!("opus"), Value::Null),
        json!({"valid": true})
    );
    assert_eq!(
        refused("claude-code", json!("fable"), Value::Null),
        "unknown_model"
    );
    assert_eq!(
        refused("claude-code", Value::Null, json!("ultra")),
        "effort_unsupported"
    );
    assert_eq!(refused("pi", json!("x"), Value::Null), "agent_unsupported");
    assert_eq!(
        refused("nonesuch", json!("x"), Value::Null),
        "agent_unsupported"
    );

    for bad in [
        json!({"agent": "codex"}),
        json!({"agent": "codex", "model": "--evil"}),
        json!({"agent": "codex", "model": "ok", "effort": "HIGH"}),
        json!({"agent": "codex", "model": "ok", "extra": 1}),
    ] {
        assert_eq!(
            api.dispatch("agents.validate_selection", bad),
            Err(RpcError::InvalidParams)
        );
    }
}

#[test]
fn validate_selection_reports_authentication_failure() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    fs::write(&agents.signed_out, "").unwrap();
    let api = api_for(&fixture, &agents);

    let result = validate(&api, "codex", json!("gpt-6-luna"), Value::Null);

    assert_eq!(result["valid"], false);
    assert_eq!(result["reason"], "authentication_failed");
}

#[test]
fn submit_refuses_a_selection_the_adapter_does_not_offer_and_creates_no_task() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);

    let unknown = submit_with(&fixture, &api, "k1", "codex", json!({"model": "gpt-9"}));
    let effort = submit_with(
        &fixture,
        &api,
        "k2",
        "codex",
        json!({"model": "gpt-6-sol", "effort": "low"}),
    );
    let claude_model = submit_with(
        &fixture,
        &api,
        "k3",
        "claude-code",
        json!({"model": "fable"}),
    );
    let malformed = submit_with(&fixture, &api, "k4", "codex", json!({"model": "--evil"}));
    let empty = submit_with(&fixture, &api, "k5", "codex", json!({}));
    let pi = submit_with(&fixture, &api, "k6", "pi", json!({"model": "x"}));

    assert_eq!(unknown, Err(RpcError::InvalidParams));
    assert_eq!(effort, Err(RpcError::InvalidParams));
    assert_eq!(claude_model, Err(RpcError::InvalidParams));
    assert_eq!(malformed, Err(RpcError::InvalidParams));
    assert_eq!(empty, Err(RpcError::InvalidParams));
    // The adapter's state, not the request, is what blocks these.
    assert_eq!(pi, Err(RpcError::Conflict));
    let tasks = api.dispatch("tasks.list", json!({})).unwrap();
    assert_eq!(tasks, json!([]));
}

#[test]
fn submit_refuses_a_selection_for_a_signed_out_or_missing_agent() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    fs::write(&agents.signed_out, "").unwrap();
    let api = fixture.api_with(&agents.codex, |config| {
        config.claude_program = Some(fixture.marker("no-such-claude"));
    });

    let signed_out = submit_with(
        &fixture,
        &api,
        "k1",
        "codex",
        json!({"model": "gpt-6-luna"}),
    );
    let missing = submit_with(
        &fixture,
        &api,
        "k2",
        "claude-code",
        json!({"effort": "low"}),
    );

    assert_eq!(signed_out, Err(RpcError::Conflict));
    assert_eq!(missing, Err(RpcError::Conflict));
    assert_eq!(api.dispatch("tasks.list", json!({})).unwrap(), json!([]));
}

#[test]
fn accepted_selection_reaches_the_agent_and_shows_in_the_effective_configuration() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);

    let submitted = submit_with(
        &fixture,
        &api,
        "sel-1",
        "codex",
        json!({"model": "gpt-6-luna", "effort": "low"}),
    )
    .unwrap();
    let task_id = submitted["contract"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        submitted["contract"]["model_selection"],
        json!({"model": "gpt-6-luna", "effort": "low"})
    );
    wait_state(&api, &task_id, "Succeeded");

    let argv = fs::read_to_string(&agents.codex_argv).unwrap();
    // The status directory (DPES-1108 phase 4) goes after the selection, before the separator.
    let status = super::worker_status::status_dir(&fixture.data(), &task_id, 1);
    let status = status.display().to_string();
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        [
            "exec",
            "--json",
            "--sandbox",
            "workspace-write",
            "--ignore-user-config",
            "-c",
            "developer_instructions=\"fixture instructions\"",
            "-m",
            "gpt-6-luna",
            "-c",
            "model_reasoning_effort=\"low\"",
            "-c",
            "show_raw_agent_reasoning=true",
            "-c",
            "model_reasoning_summary=\"detailed\"",
            "--add-dir",
            status.as_str(),
            "--",
            "# MCAO worker brief",
        ]
    );
    let session_id = task(&api, &task_id)["sessions"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let got = api
        .dispatch(
            "sessions.get",
            json!({"task_id": task_id, "session_id": session_id}),
        )
        .unwrap();
    assert_eq!(
        got["effective_configuration"],
        json!({
            "agent": "codex",
            "provider": "openai",
            "model": {"value": "gpt-6-luna", "source": "task_selection"},
            "effort": {"value": "low", "source": "task_selection"},
        })
    );
    let listed = api
        .dispatch("sessions.list", json!({"task_id": task_id}))
        .unwrap();
    assert_eq!(
        listed[0]["effective_configuration"],
        got["effective_configuration"]
    );
}

#[test]
fn task_without_a_selection_launches_unchanged_and_reports_adapter_defaults_as_unknown() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);

    let submitted = submit_with(&fixture, &api, "plain-1", "codex", Value::Null).unwrap();
    let task_id = submitted["contract"]["id"].as_str().unwrap().to_owned();
    assert!(submitted["contract"].get("model_selection").is_none());
    wait_state(&api, &task_id, "Succeeded");

    let argv = fs::read_to_string(&agents.codex_argv).unwrap();
    let status = super::worker_status::status_dir(&fixture.data(), &task_id, 1);
    let status = status.display().to_string();
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        [
            "exec",
            "--json",
            "--sandbox",
            "workspace-write",
            "--ignore-user-config",
            "-c",
            "developer_instructions=\"fixture instructions\"",
            "-c",
            "show_raw_agent_reasoning=true",
            "-c",
            "model_reasoning_summary=\"detailed\"",
            "--add-dir",
            status.as_str(),
            "--",
            "# MCAO worker brief"
        ]
    );
    let session_id = task(&api, &task_id)["sessions"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let got = api
        .dispatch(
            "sessions.get",
            json!({"task_id": task_id, "session_id": session_id}),
        )
        .unwrap();
    assert_eq!(
        got["effective_configuration"]["model"],
        json!({"value": null, "source": "adapter_default"})
    );
    assert_eq!(
        got["effective_configuration"]["effort"],
        json!({"value": null, "source": "adapter_default"})
    );
}

#[test]
fn claude_effort_selection_reaches_the_agent() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);

    submit_with(
        &fixture,
        &api,
        "claude-1",
        "claude-code",
        json!({"effort": "high"}),
    )
    .unwrap();

    wait_until("claude to record its arguments", || {
        agents.claude_argv.exists()
    });
    let argv = fs::read_to_string(&agents.claude_argv).unwrap();
    let args: Vec<_> = argv.lines().collect();
    assert_eq!(args[args.len() - 2..], ["--effort", "high"]);
    assert!(!args.contains(&"--model"));
}

#[test]
fn claude_model_alias_reaches_the_launched_claude_argv() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);

    submit_with(
        &fixture,
        &api,
        "claude-alias",
        "claude-code",
        json!({"model": "sonnet", "effort": "high"}),
    )
    .unwrap();

    wait_until("claude to record its arguments", || {
        agents.claude_argv.exists()
    });
    let argv = fs::read_to_string(&agents.claude_argv).unwrap();
    let args: Vec<_> = argv.lines().collect();
    // The alias and the effort are the last two pairs, after the fixed restricted invocation.
    assert_eq!(
        args[args.len() - 4..],
        ["--model", "sonnet", "--effort", "high"]
    );
}

#[test]
fn launch_checks_the_selection_again_against_a_fresh_catalog() {
    let fixture = Fixture::new();
    let agents = agents(&fixture);
    let api = api_for(&fixture, &agents);
    // Primes the submit-time cache while the account is signed in.
    api.dispatch("agents.catalog", Value::Null).unwrap();
    fs::write(&agents.signed_out, "").unwrap();

    let submitted = submit_with(
        &fixture,
        &api,
        "stale-1",
        "codex",
        json!({"model": "gpt-6-luna"}),
    )
    .expect("the cached catalog still accepts the selection");
    let task_id = submitted["contract"]["id"].as_str().unwrap().to_owned();

    wait_state(&api, &task_id, "Failed");
    assert!(
        !agents.codex_argv.exists(),
        "the agent must not have launched"
    );
}

/// A fake codex whose `debug models` probe runs `debug_body` first and
/// appends a line to the returned counter file each time it is run.
fn probing_codex(fixture: &Fixture, debug_body: &str) -> (PathBuf, PathBuf, PathBuf) {
    let models = fixture.marker("probing-models.json");
    fs::write(&models, MODELS).unwrap();
    let counter = fixture.marker("debug-calls.txt");
    let started = fixture.marker("debug-started");
    let codex = fixture.script(
        "probing-codex",
        &format!(
            "case \"$1\" in\n\
             login) echo 'Logged in'; exit 0;;\n\
             debug) echo x >> '{counter}'; touch '{started}'; {debug_body}\n cat '{models}'; exit 0;;\n\
             esac\n\
             {THREAD_STARTED}{TURN_COMPLETED}",
            counter = counter.display(),
            started = started.display(),
            models = models.display(),
        ),
    );
    (codex, counter, started)
}

fn probe_count(counter: &Path) -> usize {
    fs::read_to_string(counter).map_or(0, |calls| calls.lines().count())
}

fn probing_api(fixture: &Fixture, codex: &Path, timeout: Duration, wait: Duration) -> DaemonApi {
    let claude = agents(fixture).claude;
    fixture.api_with(codex, |config| {
        config.claude_program = Some(claude);
        config.catalog_timeout = timeout;
        config.catalog_wait = wait;
    })
}

#[test]
fn a_slow_catalog_read_does_not_block_an_unrelated_submit() {
    let fixture = Fixture::new();
    let (codex, _, started) = probing_codex(&fixture, "sleep 2");
    let api = probing_api(
        &fixture,
        &codex,
        Duration::from_secs(10),
        Duration::from_secs(10),
    );
    let selecting_finished = std::sync::atomic::AtomicBool::new(false);

    std::thread::scope(|scope| {
        let selecting = scope.spawn(|| {
            let result = submit_with(
                &fixture,
                &api,
                "slow-1",
                "codex",
                json!({"model": "gpt-6-luna"}),
            );
            selecting_finished.store(true, std::sync::atomic::Ordering::SeqCst);
            result
        });
        wait_until("the cold catalog read to start", || started.exists());

        let unrelated = submit_with(&fixture, &api, "plain-1", "codex", Value::Null);

        assert!(unrelated.is_ok(), "{unrelated:?}");
        assert!(
            !selecting_finished.load(std::sync::atomic::Ordering::SeqCst),
            "the unrelated submit waited for the catalog read"
        );
        assert!(selecting.join().unwrap().is_ok());
    });
}

#[test]
fn a_read_that_outlasts_the_wait_is_refused_with_a_retry_hint_and_the_key_stays_usable() {
    let fixture = Fixture::new();
    let (codex, _, _) = probing_codex(&fixture, "sleep 2");
    let api = probing_api(
        &fixture,
        &codex,
        Duration::from_secs(10),
        Duration::from_millis(300),
    );
    let selection = json!({"model": "gpt-6-luna"});

    let began = Instant::now();
    let first = submit_with(&fixture, &api, "retry-1", "codex", selection.clone());
    let elapsed = began.elapsed();

    assert_eq!(first, Err(RpcError::Conflict));
    assert!(elapsed < Duration::from_millis(1900), "{elapsed:?}");
    assert_eq!(api.dispatch("tasks.list", json!({})).unwrap(), json!([]));
    let pending = validate(&api, "codex", json!("gpt-6-luna"), Value::Null);
    assert_eq!(pending["valid"], false);
    assert_eq!(pending["reason"], "agent_state_unknown");
    let listed = api.dispatch("agents.catalog", Value::Null).unwrap();
    assert!(entry(&listed, "codex")["detail"]
        .as_str()
        .unwrap()
        .contains("retry"));

    // The read finishes in the background; the same key then succeeds, so the
    // refusal was not stored under it.
    let mut retried = Err(RpcError::Conflict);
    wait_until("the background read to make the retry succeed", || {
        retried = submit_with(&fixture, &api, "retry-1", "codex", selection.clone());
        retried != Err(RpcError::Conflict)
    });
    assert!(retried.is_ok(), "{retried:?}");
}

#[test]
fn a_timed_out_facet_is_not_cached_but_a_definitive_read_is() {
    let fixture = Fixture::new();
    let first_run = fixture.marker("first-debug-run");
    let (codex, counter, _) = probing_codex(
        &fixture,
        &format!(
            "if [ ! -f '{marker}' ]; then touch '{marker}'; sleep 30; fi",
            marker = first_run.display()
        ),
    );
    let api = probing_api(
        &fixture,
        &codex,
        Duration::from_secs(2),
        Duration::from_secs(10),
    );

    let timed_out = api.dispatch("agents.catalog", Value::Null).unwrap();
    assert_eq!(entry(&timed_out, "codex")["models"]["state"], "unknown");

    let recovered = api.dispatch("agents.catalog", Value::Null).unwrap();
    assert_eq!(entry(&recovered, "codex")["models"]["state"], "available");
    assert_eq!(probe_count(&counter), 2, "the timed-out read was reused");

    api.dispatch("agents.catalog", Value::Null).unwrap();
    assert_eq!(probe_count(&counter), 2, "a definitive read was not reused");
    assert!(submit_with(
        &fixture,
        &api,
        "after-timeout",
        "codex",
        json!({"model": "gpt-6-sol"})
    )
    .is_ok());
}

#[test]
fn concurrent_cold_reads_share_one_set_of_probes() {
    let fixture = Fixture::new();
    let (codex, counter, started) = probing_codex(&fixture, "sleep 1");
    let api = probing_api(
        &fixture,
        &codex,
        Duration::from_secs(10),
        Duration::from_secs(10),
    );

    std::thread::scope(|scope| {
        let readers: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| api.dispatch("agents.catalog", Value::Null).unwrap()))
            .collect();
        for reader in readers {
            let catalog = reader.join().unwrap();
            assert_eq!(entry(&catalog, "codex")["models"]["state"], "available");
        }
    });

    assert!(started.exists());
    assert_eq!(probe_count(&counter), 1);
}

#[test]
fn logged_text_cannot_carry_control_characters() {
    assert_eq!(
        log_safe("agent\nFORGED: line\u{1b}[31m\0"),
        "agent?FORGED: line?[31m?"
    );
    assert_eq!(log_safe("codex"), "codex");
}
