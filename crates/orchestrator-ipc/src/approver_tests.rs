//! The approver credential through the public `Server` and `Client` API, plus
//! raw frames where a test must send a handshake by hand. No platform-specific
//! socket code, so these also compile and run on Windows. No assertion message
//! here carries a secret: anything that holds a generated secret is checked with
//! `assert!` and fixed text, never `assert_eq!`, so a failure prints none.

use crate::approver::USER_ONLY_METHODS;
use crate::client::read_token;
use crate::protocol::{self, RpcId, RpcRequest, RpcResponse};
use crate::session::method_capability;
use crate::transport::{connect as connect_local, IpcStream};
use crate::{
    ApproverSecret, Capability, Client, IpcError, IpcEvent, RpcError, RpcHandler, Server,
    ServerConfig, DEFAULT_IO_TIMEOUT,
};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// A fixed, well-formed value for the pure cases. It is a test vector, not a
/// secret, so it may appear in a failure message.
const FIXED: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const NEGOTIATED: [Capability; 10] = [
    Capability::Health,
    Capability::OrchestratorActions,
    Capability::Approvals,
    Capability::Review,
    Capability::Settings,
    Capability::Monitoring,
    Capability::Projects,
    Capability::Holds,
    Capability::StandingApprovals,
    Capability::Coordinator,
];

/// Answers with the method name and records every call that reaches it, so a
/// test can tell a refusal at the gate from a call that was let through.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<String>>,
}

impl Recorder {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl RpcHandler for Recorder {
    fn capabilities(&self) -> Vec<Capability> {
        Capability::ALL.to_vec()
    }

    fn dispatch(&self, method: &str, _params: Value) -> Result<Value, RpcError> {
        self.calls.lock().unwrap().push(method.to_owned());
        Ok(json!({"method": method}))
    }

    fn events_after(&self, _after_sequence: i64, _limit: usize) -> Result<Vec<IpcEvent>, RpcError> {
        Ok(Vec::new())
    }
}

fn new_secret() -> Arc<ApproverSecret> {
    Arc::new(ApproverSecret::generate().unwrap())
}

fn serve(
    dir: &Path,
    recorder: &Arc<Recorder>,
    approver: Option<&Arc<ApproverSecret>>,
) -> JoinHandle<()> {
    let dir = dir.to_path_buf();
    let recorder = recorder.clone();
    // `ApproverSecret` is deliberately not `Clone`; the server gets its own copy.
    let approver = approver.map(|secret| ApproverSecret::from_wire(&secret.to_wire()).unwrap());
    thread::spawn(move || {
        let mut config = ServerConfig::new(dir, "approver-test")
            .with_idle_timeout(Duration::from_secs(30))
            .with_stream_timeout(Duration::from_secs(5));
        if let Some(approver) = approver {
            config = config.with_approver(approver);
        }
        Server::serve_with_handler(config, recorder).unwrap();
    })
}

fn connect(
    dir: &Path,
    capabilities: &[Capability],
    approver: Option<&Arc<ApproverSecret>>,
) -> Client {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match Client::connect_with_approver(
            dir,
            capabilities,
            DEFAULT_IO_TIMEOUT,
            approver.cloned(),
        ) {
            Ok(client) => return client,
            Err(error) => {
                assert!(Instant::now() < deadline, "daemon not ready: {error}");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

/// A session that negotiated exactly what `method` needs, so a refusal can only
/// come from the approver gate.
fn connect_for(dir: &Path, method: &str, approver: Option<&Arc<ApproverSecret>>) -> Client {
    // A listed name that no capability routes protects nothing; fail naming it.
    let capability = method_capability(method).expect(method);
    connect(dir, &[Capability::Health, capability], approver)
}

fn stop(dir: &Path, daemon: JoinHandle<()>) {
    connect(dir, &[Capability::Health], None)
        .shutdown()
        .unwrap();
    daemon.join().unwrap();
}

fn handshake_params(dir: &Path, capabilities: &[Capability], approver: Option<&str>) -> Value {
    let names: Vec<&str> = capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect();
    let mut params = json!({
        "token": read_token(dir).unwrap(),
        "versions": [1],
        "capabilities": names,
    });
    if let Some(approver) = approver {
        params["approver"] = json!(approver);
    }
    params
}

fn raw_call(stream: &mut Box<dyn IpcStream>, id: u64, method: &str, params: Value) -> RpcResponse {
    let request = RpcRequest {
        jsonrpc: "2.0".into(),
        id: RpcId::Number(id.into()),
        method: method.into(),
        params,
    };
    protocol::write(stream, &request).unwrap();
    protocol::read::<RpcResponse>(stream).unwrap()
}

/// Sends the handshake by hand, so a test can send a second one or read every
/// frame the server writes. Returns the stream and the welcome.
fn raw_session(
    dir: &Path,
    capabilities: &[Capability],
    approver: Option<&str>,
) -> (Box<dyn IpcStream>, Value) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        match connect_local(dir) {
            Ok(stream) => break stream,
            Err(_) => {
                assert!(Instant::now() < deadline, "daemon not ready");
                thread::sleep(Duration::from_millis(20));
            }
        }
    };
    stream.set_timeouts(Duration::from_secs(5)).unwrap();
    let params = handshake_params(dir, capabilities, approver);
    let response = raw_call(&mut stream, 1, "handshake", params);
    assert!(response.error.is_none(), "the handshake was refused");
    (stream, response.result.expect("a welcome"))
}

fn refusal_code(response: &RpcResponse) -> i32 {
    response.error.as_ref().expect("an error response").code
}

#[test]
fn a_secret_round_trips_through_hex_and_matches_only_itself() {
    let secret = ApproverSecret::from_wire(FIXED).unwrap();
    assert_eq!(secret.to_wire(), FIXED);
    assert!(secret.matches(FIXED));
    assert!(ApproverSecret::from_wire(&format!("{FIXED}\n")).is_some());

    let first = if FIXED.starts_with('0') { '1' } else { '0' };
    let flipped = format!("{first}{}", &FIXED[1..]);
    let short = &FIXED[..63];
    let long = format!("{FIXED}0");
    let upper = FIXED.to_uppercase();
    let non_hex = "g".repeat(64);
    let non_ascii = "é".repeat(32);
    let too_many_newlines = format!("{FIXED}\n\n");
    for rejected in [
        flipped.as_str(),
        short,
        long.as_str(),
        upper.as_str(),
        non_hex.as_str(),
        non_ascii.as_str(),
        "",
    ] {
        assert!(!secret.matches(rejected), "a different value matched");
    }
    for rejected in [
        short,
        long.as_str(),
        upper.as_str(),
        non_hex.as_str(),
        non_ascii.as_str(),
        too_many_newlines.as_str(),
        "",
    ] {
        assert!(
            ApproverSecret::from_wire(rejected).is_none(),
            "a malformed value parsed"
        );
    }
    let other = ApproverSecret::generate().unwrap();
    assert!(!secret.matches(&other.to_wire()), "two secrets matched");
}

#[test]
fn an_all_zero_secret_is_refused() {
    // A test vector, not a secret. `generate` never yields it, so only a
    // placeholder or a host bug can.
    let zeros = "0".repeat(64);
    assert!(
        ApproverSecret::from_wire(&zeros).is_none(),
        "an all-zero value parsed"
    );
    assert!(
        ApproverSecret::from_wire(&format!("{zeros}\n")).is_none(),
        "an all-zero value with a newline parsed"
    );
    let mut input = zeros.as_bytes();
    assert!(matches!(
        ApproverSecret::read_from(&mut input),
        Err(IpcError::Operation)
    ));
    // One non-zero digit is enough to be accepted.
    let almost = format!("{}1", "0".repeat(63));
    assert!(ApproverSecret::from_wire(&almost).is_some());
}

#[test]
fn generated_secrets_are_distinct_and_64_hex_characters() {
    let first = ApproverSecret::generate().unwrap().to_wire();
    let second = ApproverSecret::generate().unwrap().to_wire();
    assert!(first != second, "two generated secrets were equal");
    for wire in [&first, &second] {
        assert!(
            wire.len() == 64
                && wire
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "a generated secret was not 64 lowercase hex characters"
        );
    }
}

#[test]
fn read_from_accepts_one_trailing_newline_and_rejects_more_than_66_bytes() {
    fn read(bytes: &[u8]) -> Result<ApproverSecret, IpcError> {
        let mut input = bytes;
        ApproverSecret::read_from(&mut input)
    }
    let wire = FIXED.as_bytes();
    assert!(read(wire).is_ok());
    assert!(read(format!("{FIXED}\n").as_bytes()).is_ok());
    // Every one of these is a fixed error that echoes nothing.
    let cases: [Vec<u8>; 8] = [
        format!("{FIXED}\n\n\n").into_bytes(),
        format!("{FIXED}\n\n").into_bytes(),
        format!("{FIXED}\r\n").into_bytes(),
        format!("{FIXED}0").into_bytes(),
        wire[..63].to_vec(),
        vec![0xff; 64],
        vec![b'a'; 10_000],
        Vec::new(),
    ];
    for case in &cases {
        assert!(
            matches!(read(case), Err(IpcError::Operation)),
            "bad input was not refused"
        );
    }
    // An endless reader must end at the cap, not hang or fill memory.
    let mut endless = std::io::repeat(b'a');
    assert!(matches!(
        ApproverSecret::read_from(&mut endless),
        Err(IpcError::Operation)
    ));
}

#[test]
fn the_debug_text_of_a_secret_is_redacted() {
    let secret = new_secret();
    let wire = secret.to_wire();
    for text in [format!("{secret:?}"), format!("{secret:#?}")] {
        assert!(text.contains("REDACTED"), "the debug text lost its marker");
        assert!(!text.contains(&wire), "the debug text carried the secret");
    }
    assert_eq!(format!("{secret:?}"), "ApproverSecret([REDACTED])");
}

#[test]
fn a_session_with_only_the_token_is_refused_a_user_only_method() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    let mut client = connect(
        dir.path(),
        &[Capability::Health, Capability::OrchestratorActions],
        None,
    );
    assert!(!client.is_approver());
    assert!(matches!(
        client.actions_resolve("proposal-1", "approve", DIGEST, "key-1"),
        Err(IpcError::Forbidden)
    ));
    assert!(recorder.calls().is_empty());
    drop(client);
    stop(dir.path(), daemon);
}

#[test]
fn a_session_with_the_secret_reaches_the_handler() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    let mut client = connect(
        dir.path(),
        &[Capability::Health, Capability::OrchestratorActions],
        Some(&approver),
    );
    assert!(client.is_approver());
    let result = client
        .actions_resolve("proposal-1", "approve", DIGEST, "key-1")
        .unwrap();
    assert_eq!(result["method"], "actions.resolve");
    assert_eq!(recorder.calls().len(), 1);
    drop(client);
    stop(dir.path(), daemon);
}

/// Walks the constant, so a method added to it later is covered with no new
/// test. `method_capability(..).expect(..)` makes a listed name that no
/// capability routes fail here instead of protecting nothing.
#[test]
fn every_user_only_method_is_refused_to_a_token_only_session() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let wrong = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    for &method in USER_ONLY_METHODS {
        for proof in [None, Some(&wrong)] {
            let mut client = connect_for(dir.path(), method, proof);
            assert!(
                !client.is_approver(),
                "{method}: the session was an approver"
            );
            assert!(
                matches!(client.call(method, json!({})), Err(IpcError::Forbidden)),
                "{method} was not refused with Forbidden"
            );
        }
    }
    assert!(
        recorder.calls().is_empty(),
        "a refused call reached the handler"
    );
    stop(dir.path(), daemon);
}

/// The read-only MCP setting (DPES-1105 phase 6b) has no method of its own: it rides on
/// `settings.set`, so a token-only session must be refused with the `mcp` group in the patch.
#[test]
fn mcp_setting_is_refused_to_a_token_only_session() {
    assert!(USER_ONLY_METHODS.contains(&"settings.set"));
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    let mut client = connect_for(dir.path(), "settings.set", None);
    assert!(!client.is_approver());
    assert!(matches!(
        client.call(
            "settings.set",
            json!({"idempotency_key": "mcp-token", "mcp": {"enabled": true}})
        ),
        Err(IpcError::Forbidden)
    ));
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn every_user_only_method_reaches_the_handler_for_an_approver_session() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    for &method in USER_ONLY_METHODS {
        let mut client = connect_for(dir.path(), method, Some(&approver));
        assert!(
            client.is_approver(),
            "{method}: the session was not an approver"
        );
        let result = client.call(method, json!({})).unwrap();
        assert_eq!(result["method"], method);
    }
    assert_eq!(recorder.calls().len(), USER_ONLY_METHODS.len());
    stop(dir.path(), daemon);
}

#[test]
fn a_method_outside_the_user_only_list_is_not_refused_to_a_token_only_session() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    let mut client = connect(
        dir.path(),
        &[Capability::Health, Capability::Approvals],
        None,
    );
    assert!(!client.is_approver());
    // `approvals.list` shares a capability with `approvals.resolve`; the gate
    // keys on the method, not the capability.
    let result = client.approvals_list().unwrap();
    assert_eq!(result["method"], "approvals.list");
    client.health().unwrap();
    assert_eq!(recorder.calls(), ["approvals.list"]);
    drop(client);
    stop(dir.path(), daemon);
}

#[test]
fn a_session_with_a_wrong_approver_secret_connects_and_is_refused() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let wrong = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    let mut client = connect(dir.path(), &NEGOTIATED, Some(&wrong));
    assert!(!client.is_approver());
    assert!(matches!(
        client.actions_resolve("proposal-1", "approve", DIGEST, "key-1"),
        Err(IpcError::Forbidden)
    ));
    assert!(matches!(
        client.approvals_resolve("approval-1", "approve", "key-2"),
        Err(IpcError::Forbidden)
    ));
    client.health().unwrap();
    drop(client);
    // A value that is not even hex is no match either, and not an error.
    for malformed in ["not-hex", ""] {
        let (stream, welcome) = raw_session(dir.path(), &NEGOTIATED, Some(malformed));
        assert_eq!(welcome["approver"], json!(false));
        drop(stream);
    }
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn a_daemon_with_no_secret_refuses_a_session_that_presents_one() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, None);
    for &method in USER_ONLY_METHODS {
        let mut client = connect_for(dir.path(), method, Some(&approver));
        assert!(!client.is_approver());
        assert!(
            matches!(client.call(method, json!({})), Err(IpcError::Forbidden)),
            "{method} was not refused with Forbidden"
        );
    }
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn the_welcome_reports_whether_the_proof_held() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let wrong = new_secret();
    let (secret, supplied) = (approver.to_wire(), wrong.to_wire());
    let daemon = serve(dir.path(), &recorder, Some(&approver));

    // Always present: a session without the proof gets an explicit false.
    for (proof, expected) in [
        (Some(secret.as_str()), true),
        (Some(supplied.as_str()), false),
        (None, false),
    ] {
        let (stream, welcome) = raw_session(dir.path(), &NEGOTIATED, proof);
        assert_eq!(welcome["approver"], json!(expected));
        drop(stream);
    }
    assert!(connect(dir.path(), &NEGOTIATED, Some(&approver)).is_approver());
    assert!(!connect(dir.path(), &NEGOTIATED, None).is_approver());
    stop(dir.path(), daemon);
}

#[test]
fn a_second_handshake_cannot_upgrade_a_session() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let secret = approver.to_wire();
    let daemon = serve(dir.path(), &recorder, Some(&approver));

    let (mut stream, welcome) = raw_session(dir.path(), &NEGOTIATED, None);
    assert_eq!(welcome["approver"], json!(false));
    let params = handshake_params(dir.path(), &NEGOTIATED, Some(&secret));
    let second = raw_call(&mut stream, 2, "handshake", params);
    // `handshake` is not a routed method after the first one.
    assert_eq!(refusal_code(&second), -32601);
    for &method in USER_ONLY_METHODS {
        let refused = raw_call(&mut stream, 3, method, json!({}));
        assert_eq!(refusal_code(&refused), -32012, "{method} was not refused");
    }
    assert!(recorder.calls().is_empty());
    drop(stream);
    stop(dir.path(), daemon);
}

#[test]
fn a_session_without_the_capability_still_gets_capability_not_negotiated() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    // Token-only proves the order: the capability check comes before the
    // approver gate, so -32004 and not -32012. The approver proves the gate
    // never lets a session past a capability it did not negotiate.
    for proof in [None, Some(&approver)] {
        let mut client = connect(dir.path(), &[Capability::Health], proof);
        for &method in USER_ONLY_METHODS {
            assert!(
                matches!(
                    client.call(method, json!({})),
                    Err(IpcError::UnsupportedCapability)
                ),
                "{method} was not refused as an unnegotiated capability"
            );
        }
    }
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn the_user_only_list_is_exactly_the_reviewed_set() {
    // Changing this list changes who may approve. A change to the constant
    // fails here until a person edits this pin in the same diff.
    assert_eq!(
        USER_ONLY_METHODS,
        &[
            "actions.resolve",
            "approvals.resolve",
            "review.request_approval",
            "review.request_changes",
            "settings.set",
            "monitoring.set",
            "holds.set",
            "holds.release",
            "projects.set_defaults",
            "grants.create",
            "grants.revoke",
            "memory.set",
            "memory.remove",
            "memory.restore",
            "reports.get",
            "reports.relay",
            "decisions.list",
            "decisions.answer",
            "decisions.defer",
            "decisions.reconcile",
        ]
    );
    for method in [
        "decisions.list",
        "decisions.answer",
        "decisions.defer",
        "decisions.reconcile",
    ] {
        assert_eq!(
            method_capability(method),
            Some(Capability::Holds),
            "{method}"
        );
    }
    assert_eq!(
        method_capability("actions.resolve"),
        Some(Capability::OrchestratorActions)
    );
    assert_eq!(
        method_capability("approvals.resolve"),
        Some(Capability::Approvals)
    );
    // `review.evidence` only reads, so it stays open to a token-only session.
    assert_eq!(
        method_capability("review.request_approval"),
        Some(Capability::Review)
    );
    assert_eq!(
        method_capability("review.request_changes"),
        Some(Capability::Review)
    );
    assert!(!USER_ONLY_METHODS.contains(&"review.evidence"));
    assert_eq!(
        method_capability("settings.set"),
        Some(Capability::Settings)
    );
    // `settings.get` only reads, so it stays open to a token-only session.
    assert!(!USER_ONLY_METHODS.contains(&"settings.get"));
    for &method in USER_ONLY_METHODS {
        assert!(
            method_capability(method).is_some(),
            "{method} is not a routed method"
        );
    }
}

#[test]
fn settings_get_is_open_to_a_token_only_session_and_settings_set_is_not() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));

    let load = json!({"cpu_percent": 60});
    let mut token_only = connect(
        dir.path(),
        &[Capability::Health, Capability::Settings],
        None,
    );
    assert!(token_only.settings_get().is_ok());
    assert!(matches!(
        token_only.settings_set(load.clone(), "key-1"),
        Err(IpcError::Forbidden)
    ));
    assert_eq!(recorder.calls(), ["settings.get"]);

    let mut user = connect(
        dir.path(),
        &[Capability::Health, Capability::Settings],
        Some(&approver),
    );
    assert!(user.settings_set(load, "key-2").is_ok());
    assert_eq!(recorder.calls(), ["settings.get", "settings.set"]);
    stop(dir.path(), daemon);
}

#[test]
fn settings_methods_need_the_settings_capability() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));
    for proof in [None, Some(&approver)] {
        let mut client = connect(dir.path(), &[Capability::Health], proof);
        for method in ["settings.get", "settings.set"] {
            assert!(
                matches!(
                    client.call(method, json!({})),
                    Err(IpcError::UnsupportedCapability)
                ),
                "{method} was not refused as an unnegotiated capability"
            );
        }
        assert!(matches!(
            client.settings_get(),
            Err(IpcError::UnsupportedCapability)
        ));
    }
    assert!(recorder.calls().is_empty());
    stop(dir.path(), daemon);
}

#[test]
fn the_secret_never_appears_in_a_welcome_an_error_or_a_debug_dump() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let wrong = new_secret();
    let (secret, supplied) = (approver.to_wire(), wrong.to_wire());
    let daemon = serve(dir.path(), &recorder, Some(&approver));

    let mut seen = vec![format!("{approver:?} {wrong:?} {approver:#?}")];
    for proof in [None, Some(supplied.as_str()), Some(secret.as_str())] {
        let (mut stream, welcome) = raw_session(dir.path(), &NEGOTIATED, proof);
        seen.push(welcome.to_string());
        for &method in USER_ONLY_METHODS {
            let response = raw_call(&mut stream, 2, method, json!({}));
            seen.push(serde_json::to_string(&response).unwrap());
        }
    }
    for proof in [None, Some(&wrong)] {
        let mut client = connect(dir.path(), &NEGOTIATED, proof);
        let error = client.call("actions.resolve", json!({})).unwrap_err();
        seen.push(format!("{error:?} {error}"));
    }
    for text in &seen {
        assert!(!text.contains(&secret), "output carried the secret");
        assert!(
            !text.contains(&supplied),
            "output carried the supplied value"
        );
    }
    stop(dir.path(), daemon);
}

#[test]
fn a_reconnect_keeps_the_approver_role() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));

    // `reconnect` opens a new connection and replaces the old one whether or
    // not the old one is still alive, so this needs no wait for a drop.
    let mut client = connect(dir.path(), &NEGOTIATED, Some(&approver));
    assert!(client.is_approver());
    client.reconnect().unwrap();
    assert!(client.is_approver());
    let result = client
        .actions_resolve("proposal-1", "approve", DIGEST, "key-1")
        .unwrap();
    assert_eq!(result["method"], "actions.resolve");
    assert_eq!(recorder.calls(), ["actions.resolve"]);

    // A session that never had the role does not gain it by reconnecting.
    let mut token_only = connect(dir.path(), &NEGOTIATED, None);
    token_only.reconnect().unwrap();
    assert!(!token_only.is_approver());
    assert!(matches!(
        token_only.actions_resolve("proposal-1", "approve", DIGEST, "key-2"),
        Err(IpcError::Forbidden)
    ));
    assert_eq!(recorder.calls().len(), 1);
    drop(client);
    drop(token_only);
    stop(dir.path(), daemon);
}

#[test]
fn a_token_only_session_with_projects_cannot_set_defaults_but_reads_them() {
    let dir = TempDir::new().unwrap();
    let recorder = Arc::new(Recorder::default());
    let approver = new_secret();
    let daemon = serve(dir.path(), &recorder, Some(&approver));

    // Both methods share the `projects` capability; only the write is user-only.
    let mut token_only = connect(
        dir.path(),
        &[Capability::Health, Capability::Projects],
        None,
    );
    assert!(token_only
        .projects_get_defaults(json!({"project_id": "project-1"}))
        .is_ok());
    assert!(matches!(
        token_only.projects_set_defaults(json!({
            "project_id": "project-1",
            "mode": "restricted",
            "idempotency_key": "key-1"
        })),
        Err(IpcError::Forbidden)
    ));
    assert_eq!(recorder.calls(), ["projects.get_defaults"]);
    drop(token_only);
    stop(dir.path(), daemon);
}
