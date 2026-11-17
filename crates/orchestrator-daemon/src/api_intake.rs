//! DPES-673: work-item intake. `intake.normalize` turns pasted text, a Jira key
//! or a GitHub issue reference into a draft work item so a client can show it
//! for confirmation.
//!
//! The call is pure: it reads nothing, writes nothing (no store row, no event,
//! no file) and makes no network request. A bare key is only recorded as the
//! source. It sits behind the same capability as `tasks.submit`. Input that
//! breaks an intake limit is refused as `InvalidParams` (an RPC error carries no
//! detail); nothing is truncated.

use orchestrator_core::work_item;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalizeParams {
    input: String,
}

impl DaemonApi {
    pub(super) fn intake_normalize(&self, params: Value) -> Result<Value, RpcError> {
        let params: NormalizeParams = parse_params(params)?;
        let work_item = work_item::normalize(&params.input).map_err(|_| RpcError::InvalidParams)?;
        Ok(json!({ "work_item": work_item }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::work_item::{MAX_INPUT_BYTES, MAX_TITLE_CHARS};
    use std::collections::BTreeSet;
    use std::fs;

    fn new_api() -> (tempfile::TempDir, DaemonApi) {
        let directory = tempfile::tempdir().unwrap();
        let api = DaemonApi::new(directory.path()).unwrap();
        (directory, api)
    }

    fn files(directory: &Path) -> BTreeSet<(String, u64)> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    entry.metadata().unwrap().len(),
                )
            })
            .collect()
    }

    #[test]
    fn normalize_returns_the_draft_work_item() {
        let (_directory, api) = new_api();
        let result = api
            .dispatch(
                "intake.normalize",
                json!({"input": "Add intake DPES-673\nPaste it.\nAC1: Keys kept\n- [ ] Text bounded"}),
            )
            .unwrap();
        assert_eq!(
            result,
            json!({"work_item": {
                "source": {"kind": "jira", "reference": "DPES-673"},
                "title": "Add intake DPES-673",
                "ask": "Paste it.",
                "acceptance_criteria": [
                    {"text": "Keys kept", "drafted": false, "confirmed": false},
                    {"text": "Text bounded", "drafted": false, "confirmed": false},
                ],
            }})
        );
    }

    #[test]
    fn normalize_drafts_a_criterion_for_a_bare_key() {
        let (_directory, api) = new_api();
        let result = api
            .dispatch("intake.normalize", json!({"input": "acme/widgets#7"}))
            .unwrap();
        let item = &result["work_item"];
        assert_eq!(
            item["source"],
            json!({"kind": "github_issue", "reference": "acme/widgets#7"})
        );
        assert_eq!(item["acceptance_criteria"][0]["drafted"], json!(true));
        assert_eq!(item["acceptance_criteria"][0]["confirmed"], json!(false));
    }

    #[test]
    fn normalize_refuses_over_limit_empty_and_malformed_requests() {
        let (_directory, api) = new_api();
        let refused = |params: Value| api.dispatch("intake.normalize", params);
        assert_eq!(
            refused(json!({"input": "a".repeat(MAX_INPUT_BYTES + 1)})),
            Err(RpcError::InvalidParams)
        );
        assert_eq!(
            refused(json!({"input": "t".repeat(MAX_TITLE_CHARS + 1)})),
            Err(RpcError::InvalidParams)
        );
        assert_eq!(
            refused(json!({"input": " \u{0} "})),
            Err(RpcError::InvalidParams)
        );
        for params in [
            json!({}),
            Value::Null,
            json!({"input": 5}),
            json!({"input": "x", "extra": true}),
        ] {
            assert_eq!(refused(params), Err(RpcError::InvalidParams));
        }
    }

    #[test]
    fn normalize_has_no_side_effects() {
        let (directory, api) = new_api();
        let before = (
            files(directory.path()),
            api.store.latest_event_sequence().unwrap(),
            api.store.list().unwrap().len(),
            api.store.list_projects().unwrap().len(),
        );
        for input in [
            "DPES-1",
            "a/b#2\n1. x",
            "free text",
            &"z".repeat(MAX_INPUT_BYTES + 1),
        ] {
            let _ = api.dispatch("intake.normalize", json!({"input": input}));
        }
        let after = (
            files(directory.path()),
            api.store.latest_event_sequence().unwrap(),
            api.store.list().unwrap().len(),
            api.store.list_projects().unwrap().len(),
        );
        assert_eq!(before, after);
        assert_eq!((after.2, after.3), (0, 0));
    }

    fn register(api: &DaemonApi) {
        api.store
            .register_project(&Project {
                id: "project-1".into(),
                root_path: "/private/project-root".into(),
                settings: json!({}),
            })
            .unwrap();
    }

    fn submit_params(key: &str, work_item: Option<Value>) -> Value {
        let mut contract = json!({
            "project_id": "project-1",
            "parent_id": null,
            "objective": "Implement the intake",
            "kind": "Implement",
            "agent": "codex",
            "dependencies": [],
            "verification": "None",
            "delivery": "ReportOnly",
            "autonomy": "WorkspaceWrites",
            "max_retries": 0,
            "max_repairs": 0
        });
        if let Some(item) = work_item {
            contract["work_item"] = item;
        }
        json!({"idempotency_key": key, "contract": contract})
    }

    /// A draft from `intake.normalize` with every criterion confirmed, as a client
    /// sends it after the user accepts the criteria.
    fn confirmed(api: &DaemonApi, input: &str) -> Value {
        let mut item = api
            .dispatch("intake.normalize", json!({"input": input}))
            .unwrap()["work_item"]
            .clone();
        for criterion in item["acceptance_criteria"].as_array_mut().unwrap() {
            criterion["confirmed"] = json!(true);
        }
        item
    }

    #[test]
    fn a_confirmed_work_item_is_persisted_and_read_back_across_a_restart() {
        let (directory, api) = new_api();
        register(&api);
        let item = confirmed(
            &api,
            "DPES-673 Add intake\nAsk.\n1. Source kept\n2. Bounded",
        );
        let params = submit_params("k1", Some(item.clone()));
        let submitted = api.dispatch("tasks.submit", params.clone()).unwrap();
        assert_eq!(submitted["state"], "Queued");
        assert_eq!(submitted["contract"]["work_item"], item);
        let id = submitted["contract"]["id"].as_str().unwrap().to_owned();
        drop(api);

        let reopened = DaemonApi::new(directory.path()).unwrap();
        let got = reopened
            .dispatch("tasks.get", json!({"task_id": id}))
            .unwrap();
        assert_eq!(got["contract"]["work_item"], item);
        let listed = reopened.dispatch("tasks.list", Value::Null).unwrap();
        assert_eq!(listed[0]["contract"]["work_item"], item);
        // The same request replays to the same task; a changed work item under the
        // same key is a conflict, not a second task.
        assert_eq!(
            reopened.dispatch("tasks.submit", params).unwrap(),
            submitted
        );
        let mut other = item;
        other["acceptance_criteria"][0]["text"] = json!("Something else");
        assert_eq!(
            reopened.dispatch("tasks.submit", submit_params("k1", Some(other))),
            Err(RpcError::IdempotencyConflict)
        );
        assert_eq!(reopened.store.list().unwrap().len(), 1);
    }

    #[test]
    fn submit_refuses_an_unconfirmed_empty_or_malformed_work_item_and_creates_nothing() {
        let (_directory, api) = new_api();
        register(&api);
        let draft = api
            .dispatch("intake.normalize", json!({"input": "Title\n1. a\n2. b"}))
            .unwrap()["work_item"]
            .clone();
        let mut partly = confirmed(&api, "Title\n1. a\n2. b");
        partly["acceptance_criteria"][1]["confirmed"] = json!(false);
        let mut empty = confirmed(&api, "Title\n1. a");
        empty["acceptance_criteria"] = json!([]);
        let mut too_long = confirmed(&api, "Title\n1. a");
        too_long["acceptance_criteria"][0]["text"] = json!("c".repeat(501));
        let mut unknown_field = confirmed(&api, "Title\n1. a");
        unknown_field["extra"] = json!(true);

        for (name, item) in [
            ("unconfirmed draft", draft),
            ("partly confirmed", partly),
            ("no criteria", empty),
            ("criterion too long", too_long),
            ("unknown field", unknown_field),
            ("not an object", json!("text")),
        ] {
            assert_eq!(
                api.dispatch("tasks.submit", submit_params(name, Some(item))),
                Err(RpcError::InvalidParams),
                "{name}"
            );
        }
        assert!(api.store.list().unwrap().is_empty());
        assert_eq!(api.store.stream_events_after(0, 20).unwrap().len(), 0);
    }

    #[test]
    fn a_submission_without_a_work_item_is_unchanged() {
        let (_directory, api) = new_api();
        register(&api);
        let params = submit_params("plain", None);
        // The fingerprint hashes the re-serialized request, so an absent work item
        // must serialize to nothing.
        let parsed: super::super::TaskSubmitParams = parse_params(params.clone()).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), params);

        let task = api.dispatch("tasks.submit", params).unwrap();
        let contract = task["contract"].as_object().unwrap();
        let mut keys: Vec<&str> = contract.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "agent",
                "autonomy",
                "delivery",
                "dependencies",
                "id",
                "kind",
                "max_repairs",
                "max_retries",
                "objective",
                "parent_id",
                "project_id",
                "verification",
            ]
        );
    }
}
