use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::Project;
use orchestrator_core::workstream::{
    AcceptanceGate, GateState, Objective, Workstream, WorkstreamRuleError, WorkstreamState,
};
use orchestrator_store::{SqliteStore, Store, WorkstreamStoreError};
use serde_json::json;

fn gate(id: &str) -> AcceptanceGate {
    AcceptanceGate {
        id: id.into(),
        description: format!("gate {id}"),
        state: GateState::Pending,
        evidence_task_id: None,
    }
}

fn contract(id: &str, project: &str) -> TaskContract {
    TaskContract {
        id: id.into(),
        project_id: project.into(),
        parent_id: None,
        objective: format!("work for {id}"),
        kind: TaskKind::Implement,
        agent: "codex".into(),
        dependencies: Vec::new(),
        verification: VerificationPolicy::None,
        delivery: DeliveryPolicy::LocalReview,
        autonomy: AutonomyPolicy::WorkspaceWrites,
        max_retries: 0,
        max_repairs: 0,
        model_selection: None,
    }
}

fn new_store() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    for id in ["p1", "p2"] {
        store
            .register_project(&Project {
                id: id.into(),
                root_path: format!("/work/{id}"),
                settings: json!({}),
            })
            .unwrap();
    }
    store.create(&contract("t1", "p1")).unwrap();
    store.create(&contract("t2", "p2")).unwrap();
    (directory, store)
}

fn seeded() -> (tempfile::TempDir, SqliteStore) {
    let (directory, store) = new_store();
    store
        .create_workstream(&Workstream::new("w1", "Initiative", "").unwrap(), "user")
        .unwrap();
    store
        .add_objective(
            &Objective::new("o1", "w1", "Outcome", "", vec![gate("g1")]).unwrap(),
            "user",
        )
        .unwrap();
    (directory, store)
}

#[test]
fn create_get_and_list_round_trip_in_creation_order() {
    let (_directory, store) = new_store();
    for id in ["wb", "wa"] {
        store
            .create_workstream(
                &Workstream::new(id, format!("Title {id}"), "s").unwrap(),
                "user",
            )
            .unwrap();
    }
    let got = store.get_workstream("wa").unwrap();
    assert_eq!(got.title, "Title wa");
    assert_eq!(got.revision, 1);
    let ids: Vec<_> = store
        .list_workstreams(None)
        .unwrap()
        .into_iter()
        .map(|w| w.id)
        .collect();
    assert_eq!(ids, ["wb", "wa"]);
    assert!(store
        .list_workstreams(Some(WorkstreamState::Archived))
        .unwrap()
        .is_empty());
}

#[test]
fn duplicate_and_missing_workstreams_are_typed_errors() {
    let (_directory, store) = seeded();
    assert!(matches!(
        store.create_workstream(&Workstream::new("w1", "Again", "").unwrap(), "user"),
        Err(WorkstreamStoreError::AlreadyExists {
            entity: "workstream",
            ..
        })
    ));
    assert!(matches!(
        store.get_workstream("nope"),
        Err(WorkstreamStoreError::NotFound {
            entity: "workstream",
            ..
        })
    ));
    assert!(matches!(
        store.add_objective(
            &Objective::new("o9", "nope", "t", "", vec![]).unwrap(),
            "user"
        ),
        Err(WorkstreamStoreError::NotFound {
            entity: "workstream",
            ..
        })
    ));
}

#[test]
fn update_rejects_stale_revision_and_writes_nothing() {
    let (_directory, store) = seeded();
    let updated = store
        .update_workstream("w1", 1, Some("Renamed"), None, None, "user")
        .unwrap();
    assert_eq!(updated.revision, 2);
    let stale = store.update_workstream("w1", 1, Some("Lost update"), None, None, "user");
    assert!(matches!(
        stale,
        Err(WorkstreamStoreError::Stale {
            expected: 1,
            actual: 2,
            ..
        })
    ));
    let stored = store.get_workstream("w1").unwrap();
    assert_eq!((stored.title.as_str(), stored.revision), ("Renamed", 2));
    assert_eq!(
        store
            .workstream_revisions("workstream", "w1")
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn rule_violations_are_rejected_without_a_revision() {
    let (_directory, store) = seeded();
    assert!(matches!(
        store.update_workstream("w1", 1, Some("  "), None, None, "user"),
        Err(WorkstreamStoreError::Rule(WorkstreamRuleError::EmptyField(
            "title"
        )))
    ));
    assert!(matches!(
        store.update_workstream("w1", 1, None, None, Some(WorkstreamState::Active), "user"),
        Err(WorkstreamStoreError::Rule(
            WorkstreamRuleError::IllegalWorkstreamTransition { .. }
        ))
    ));
    assert_eq!(store.get_workstream("w1").unwrap().revision, 1);
}

#[test]
fn archived_workstream_is_terminal_and_refuses_new_work() {
    let (_directory, store) = seeded();
    store
        .update_workstream("w1", 1, None, None, Some(WorkstreamState::Archived), "user")
        .unwrap();
    for result in [
        store
            .update_workstream("w1", 2, Some("x"), None, None, "user")
            .map(drop),
        store
            .add_objective(
                &Objective::new("o2", "w1", "t", "", vec![]).unwrap(),
                "user",
            )
            .map(drop),
        store
            .revise_objective("o1", 1, Some("x"), None, &[], "user")
            .map(drop),
        store.link_task("w1", "t1", None).map(drop),
    ] {
        assert!(matches!(
            result,
            Err(WorkstreamStoreError::Rule(
                WorkstreamRuleError::WorkstreamArchived
            ))
        ));
    }
}

#[test]
fn objectives_revise_append_only_with_stale_rejection() {
    let (_directory, store) = seeded();
    let revised = store
        .revise_objective("o1", 1, Some("Better"), Some("why"), &[gate("g2")], "user")
        .unwrap();
    assert_eq!(revised.revision, 2);
    assert_eq!(
        revised
            .gates
            .iter()
            .map(|g| g.id.as_str())
            .collect::<Vec<_>>(),
        ["g1", "g2"]
    );
    assert!(matches!(
        store.revise_objective("o1", 1, Some("Stale"), None, &[], "user"),
        Err(WorkstreamStoreError::Stale {
            entity: "objective",
            expected: 1,
            actual: 2,
            ..
        })
    ));
    assert!(matches!(
        store.revise_objective("o1", 2, None, None, &[gate("g1")], "user"),
        Err(WorkstreamStoreError::Rule(
            WorkstreamRuleError::DuplicateGateId(_)
        ))
    ));
    assert_eq!(store.get_objective("o1").unwrap(), revised);
    assert_eq!(store.list_objectives("w1").unwrap(), vec![revised]);
}

#[test]
fn gate_state_requires_a_task_linked_to_the_same_workstream() {
    let (_directory, store) = seeded();
    store
        .create_workstream(&Workstream::new("w2", "Other", "").unwrap(), "user")
        .unwrap();
    assert!(matches!(
        store.set_gate_state("o1", 1, "g1", GateState::Verified, Some("t1"), "user"),
        Err(WorkstreamStoreError::EvidenceNotLinked { .. })
    ));
    store.link_task("w2", "t1", None).unwrap();
    assert!(matches!(
        store.set_gate_state("o1", 1, "g1", GateState::Verified, Some("t1"), "user"),
        Err(WorkstreamStoreError::EvidenceNotLinked { .. })
    ));
    store.unlink_task("w2", "t1").unwrap();
    store.link_task("w1", "t1", Some("o1")).unwrap();
    let verified = store
        .set_gate_state("o1", 1, "g1", GateState::Verified, Some("t1"), "user")
        .unwrap();
    assert_eq!(verified.revision, 2);
    assert_eq!(verified.gates[0].state, GateState::Verified);
    assert!(matches!(
        store.set_gate_state("o1", 1, "g1", GateState::Pending, None, "user"),
        Err(WorkstreamStoreError::Stale { .. })
    ));
    assert!(matches!(
        store.unlink_task("w1", "t1"),
        Err(WorkstreamStoreError::TaskIsGateEvidence { .. })
    ));
    store
        .set_gate_state("o1", 2, "g1", GateState::Pending, None, "user")
        .unwrap();
    assert!(store.unlink_task("w1", "t1").unwrap());
}

#[test]
fn linking_derives_projects_and_leaves_the_task_untouched() {
    let (_directory, store) = seeded();
    let before = store.get("t1").unwrap();
    let link = store.link_task("w1", "t1", Some("o1")).unwrap();
    assert_eq!(link.project_id, "p1");
    store.link_task("w1", "t2", None).unwrap();
    assert_eq!(store.get("t1").unwrap(), before);
    let projects: Vec<_> = store
        .list_workstream_tasks("w1")
        .unwrap()
        .into_iter()
        .map(|task| (task.task_id, task.project_id))
        .collect();
    assert_eq!(
        projects,
        [
            ("t1".to_owned(), "p1".to_owned()),
            ("t2".to_owned(), "p2".to_owned())
        ]
    );
    assert_eq!(store.get_workstream("w1").unwrap().revision, 1);
    assert_eq!(
        store
            .workstream_task_link("t1")
            .unwrap()
            .unwrap()
            .objective_id
            .as_deref(),
        Some("o1")
    );
    assert_eq!(store.workstream_task_link("none").unwrap(), None);
}

#[test]
fn link_is_idempotent_exclusive_and_validated() {
    let (_directory, store) = seeded();
    store
        .create_workstream(&Workstream::new("w2", "Other", "").unwrap(), "user")
        .unwrap();
    store
        .add_objective(
            &Objective::new("o2", "w2", "t", "", vec![]).unwrap(),
            "user",
        )
        .unwrap();
    let first = store.link_task("w1", "t1", None).unwrap();
    assert_eq!(store.link_task("w1", "t1", None).unwrap(), first);
    for (workstream, objective) in [("w2", None), ("w1", Some("o1"))] {
        assert!(matches!(
            store.link_task(workstream, "t1", objective),
            Err(WorkstreamStoreError::TaskAlreadyLinked { .. })
        ));
    }
    assert!(matches!(
        store.link_task("w1", "t2", Some("o2")),
        Err(WorkstreamStoreError::ObjectiveMismatch { .. })
    ));
    assert!(matches!(
        store.link_task("w1", "t2", Some("missing")),
        Err(WorkstreamStoreError::NotFound {
            entity: "objective",
            ..
        })
    ));
    assert!(matches!(
        store.link_task("w1", "missing", None),
        Err(WorkstreamStoreError::NotFound { entity: "task", .. })
    ));
    assert!(store.unlink_task("w1", "t1").unwrap());
    assert!(!store.unlink_task("w1", "t1").unwrap());
    assert!(!store.unlink_task("w2", "t2").unwrap());
    assert_eq!(
        store.link_task("w2", "t1", None).unwrap().workstream_id,
        "w2"
    );
}

#[test]
fn every_accepted_change_appends_an_immutable_revision() {
    let (_directory, store) = seeded();
    store
        .revise_objective("o1", 1, Some("Better"), None, &[], "alice")
        .unwrap();
    let history = store.workstream_revisions("objective", "o1").unwrap();
    assert_eq!(
        history
            .iter()
            .map(|r| (r.revision, r.actor.as_str()))
            .collect::<Vec<_>>(),
        [(1, "user"), (2, "alice")]
    );
    assert_eq!(history[1].snapshot["title"], "Better");
    assert_eq!(history[0].snapshot["title"], "Outcome");
}

#[test]
fn state_survives_reopening_the_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    {
        let store = SqliteStore::new(&path).unwrap();
        store
            .register_project(&Project {
                id: "p1".into(),
                root_path: "/work/p1".into(),
                settings: json!({}),
            })
            .unwrap();
        store.create(&contract("t1", "p1")).unwrap();
        store
            .create_workstream(&Workstream::new("w1", "Initiative", "s").unwrap(), "user")
            .unwrap();
        store
            .add_objective(
                &Objective::new("o1", "w1", "O", "", vec![gate("g1")]).unwrap(),
                "user",
            )
            .unwrap();
        store.link_task("w1", "t1", Some("o1")).unwrap();
    }
    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(store.get_workstream("w1").unwrap().summary, "s");
    assert_eq!(
        store.list_objectives("w1").unwrap()[0].gates,
        vec![gate("g1")]
    );
    assert_eq!(store.list_workstream_tasks("w1").unwrap().len(), 1);
}

#[test]
fn concurrent_writers_cannot_both_win_the_same_revision() {
    let (directory, store) = seeded();
    let path = directory.path().join("store.sqlite");
    let other = SqliteStore::new(&path).unwrap();
    let results: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = [&store, &other]
            .into_iter()
            .enumerate()
            .map(|(index, handle)| {
                scope.spawn(move || {
                    handle.update_workstream(
                        "w1",
                        1,
                        Some(&format!("writer {index}")),
                        None,
                        None,
                        "user",
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .any(|r| matches!(r, Err(WorkstreamStoreError::Stale { .. }))));
    assert_eq!(store.get_workstream("w1").unwrap().revision, 2);
}
