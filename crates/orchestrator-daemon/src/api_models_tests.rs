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
    "Options:\n  --effort <level>    Effort level\n                      (low, medium, high)\n";

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
             printf '%s\\n' \"$@\" > '{argv}'\n\
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
             printf '%s\\n' \"$@\" > '{argv}'\n\
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
        "max_repairs": 0
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
    assert_eq!(claude["models"]["state"], "unsupported");
    assert_eq!(claude["models"]["items"], json!([]));
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
        refused("claude-code", json!("opus"), Value::Null),
        "models_unsupported"
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
        json!({"model": "opus"}),
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
    wait_state(&api, &task_id, "Verifying");

    let argv = fs::read_to_string(&agents.codex_argv).unwrap();
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        [
            "exec",
            "--json",
            "--approve-for-me",
            "-m",
            "gpt-6-luna",
            "-c",
            "model_reasoning_effort=\"low\"",
            "--",
            "objective of sel-1",
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
    wait_state(&api, &task_id, "Verifying");

    let argv = fs::read_to_string(&agents.codex_argv).unwrap();
    assert_eq!(
        argv.lines().collect::<Vec<_>>(),
        [
            "exec",
            "--json",
            "--approve-for-me",
            "--",
            "objective of plain-1"
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
