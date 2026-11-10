//! The supervisor starts children with an allowlisted environment, never the
//! whole parent environment (plan section 13). One test function: it mutates
//! this test binary's own environment, so nothing else may run beside it.

use std::path::PathBuf;
use std::time::Duration;

use orchestrator_runtime::{ChildSupervisor, LaunchRequest, SupervisionEvent};

fn env_dump_request(env_names: &[&str]) -> LaunchRequest {
    #[cfg(unix)]
    let (program, args) = ("/usr/bin/env", Vec::new());
    #[cfg(windows)]
    let (program, args) = ("cmd.exe", vec!["/C".to_owned(), "set".to_owned()]);
    LaunchRequest {
        program: PathBuf::from(program),
        args,
        working_dir: std::env::current_dir().expect("current directory"),
        env_names: env_names.iter().map(|name| (*name).to_owned()).collect(),
    }
}

async fn child_environment(request: &LaunchRequest) -> Vec<String> {
    let mut supervisor = ChildSupervisor::launch(request).expect("launch");
    let mut names = Vec::new();
    let deadline = Duration::from_secs(20);
    loop {
        let event = tokio::time::timeout(deadline, supervisor.next_event())
            .await
            .expect("child output within deadline");
        match event {
            Some(SupervisionEvent::StdoutLine(line)) => {
                if let Some((name, _)) = line.split_once('=') {
                    names.push(name.to_owned());
                }
            }
            Some(SupervisionEvent::StderrLine(_)) => {}
            Some(SupervisionEvent::Exited { .. }) | None => break,
        }
    }
    names
}

fn has(names: &[String], wanted: &str) -> bool {
    names.iter().any(|name| name.eq_ignore_ascii_case(wanted))
}

#[tokio::test]
async fn children_receive_only_the_allowlisted_environment() {
    std::env::set_var("M6_UNLISTED_API_TOKEN", "must-not-leak");
    std::env::set_var("M6_PROVIDER_KEY", "provider");
    std::env::set_var("M6_USER_PASSED", "user");
    std::env::set_var("AGENT_ORCH_DATA_DIR", "/should/not/leak");
    std::env::set_var(
        "AGENT_ORCH_ENV_PASSTHROUGH",
        "M6_USER_PASSED, AGENT_ORCH_DATA_DIR",
    );

    let names = child_environment(&env_dump_request(&["M6_PROVIDER_KEY"])).await;

    assert!(has(&names, "PATH"), "baseline PATH is forwarded: {names:?}");
    assert!(
        has(&names, "M6_PROVIDER_KEY"),
        "launcher-listed name: {names:?}"
    );
    assert!(
        has(&names, "M6_USER_PASSED"),
        "user passthrough name: {names:?}"
    );
    assert!(
        !has(&names, "M6_UNLISTED_API_TOKEN"),
        "unlisted secret must not be inherited: {names:?}"
    );
    assert!(
        !has(&names, "AGENT_ORCH_DATA_DIR"),
        "orchestrator configuration is never forwarded: {names:?}"
    );

    std::env::remove_var("AGENT_ORCH_ENV_PASSTHROUGH");
    let names = child_environment(&env_dump_request(&[])).await;
    assert!(!has(&names, "M6_PROVIDER_KEY"), "{names:?}");
    assert!(!has(&names, "M6_USER_PASSED"), "{names:?}");
    assert!(!has(&names, "M6_UNLISTED_API_TOKEN"), "{names:?}");
}
