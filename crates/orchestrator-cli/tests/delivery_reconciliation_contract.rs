//! M3-06 acceptance: partial-delivery reconciliation is read-only against a LIVE
//! `SqliteStore`, not just a supplied record snapshot.
//!
//! `orchestrator-git` has no store dependency, so its own no-write test can only
//! snapshot refs, worktree bytes and a supplied record list. This file lives in
//! the CLI crate (which depends on both) and proves the missing half: real
//! `delivery_steps`/`delivery_step_events`, durable events, task/attempt/session
//! rows and workspaces are byte-identical before reconciliation, after it, and
//! after the store is reopened from disk, while Git refs, `worktree list` and
//! dirty/untracked worktree bytes are unchanged too.
//!
//! The store-row -> `RecordedDeliveryStep` mapping below mirrors the private
//! `inspect_task_delivery` in `workflow.rs` (which stays private); everything
//! else uses only public surfaces: `SqliteStore` reads,
//! `reconcile_delivery_steps` and `delivery_evidence`.
//!
//! Nothing here is Unix-gated. Every recorded destination comes from
//! `Repository::path()` (Git's toplevel), never `fs::canonicalize`, so it also
//! runs natively on Windows. Windows is only claimed after a native run.

use orchestrator_core::approval::{ApprovalAction, ApprovalActionKind, ParameterDigest};
use orchestrator_core::authorization::ActionKind;
use orchestrator_core::contract::{
    AutonomyPolicy, DeliveryPolicy, TaskContract, TaskKind, VerificationPolicy,
};
use orchestrator_core::records::{Project, Workspace};
use orchestrator_core::task::{Attempt, AttemptState, Session, SessionState, TaskEvent};
use orchestrator_git::delivery::DeliveryAction;
use orchestrator_git::delivery_reconcile::{
    reconcile_delivery_steps, DeliveryClassification, DeliveryStepDecision, RecordedDeliveryStep,
};
use orchestrator_git::Repository;
use orchestrator_runtime::reconcile::{delivery_evidence, DeliveryEvidence};
use orchestrator_store::{
    DeliveryOutcome, DeliveryStepRecord, EventJournalEntry, SqliteStore, Store, StreamEvent,
    TaskRecord,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use DeliveryClassification::{Ambiguous, Confirmed, NotApplied};

const REVIEW_PREFIX: &str = "refs/heads/orchestrator/review/";

fn pin_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/nonexistent-git-global")
        .env("GIT_CONFIG_SYSTEM", "/nonexistent-git-system")
}

/// Runs Git and returns `(exit code, trimmed stdout)` without asserting success.
fn git_raw(repo: &Path, args: &[&str]) -> (Option<i32>, String) {
    let mut command = Command::new("git");
    let output = pin_git(&mut command)
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run Git");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
    )
}

fn git(repo: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    let output = pin_git(&mut command)
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn init_repo(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    git(&repo, &["config", "user.name", "Reconcile test"]);
    git(
        &repo,
        &["config", "user.email", "reconcile@example.invalid"],
    );
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    fs::write(repo.join("sample.txt"), "base\n").unwrap();
    fs::write(repo.join("other.txt"), "other base\n").unwrap();
    git(&repo, &["add", "sample.txt", "other.txt"]);
    git(&repo, &["commit", "-m", "initial"]);
    repo
}

/// Git's own toplevel for `repo`, exactly what production records as a
/// delivery destination and a project id. Never `fs::canonicalize`.
fn toplevel(repo: &Path) -> String {
    Repository::new(repo)
        .expect("open repository")
        .path()
        .display()
        .to_string()
}

struct Fixture {
    _root: TempDir,
    repo: PathBuf,
    /// `Repository::path()` of `repo`: the project id and the local destination.
    project: String,
    /// `Repository::path()` of a second, unrelated repository.
    foreign: String,
    worktree: PathBuf,
    store_path: PathBuf,
    candidate: String,
    unrelated: String,
}

fn review_ref(task: &str) -> String {
    format!("{REVIEW_PREFIX}{task}")
}

fn step_id(task: &str, suffix: &str) -> String {
    format!("{task}-{suffix}")
}

/// What reconciliation must say about one seeded step.
struct Expected {
    id: String,
    classification: DeliveryClassification,
    observed_ref: Option<String>,
}

fn seed_task(store: &SqliteStore, project: &str, task: &str) {
    store
        .create(&TaskContract {
            id: task.to_owned(),
            project_id: project.to_owned(),
            parent_id: None,
            objective: "delivery reconciliation".to_owned(),
            kind: TaskKind::Implement,
            agent: "codex".to_owned(),
            dependencies: Vec::new(),
            verification: VerificationPolicy::None,
            delivery: DeliveryPolicy::LocalReview,
            autonomy: AutonomyPolicy::WorkspaceWrites,
            max_retries: 0,
            max_repairs: 0,
        })
        .unwrap();
    store.transition(task, TaskEvent::Admit).unwrap();
}

/// Persists one delivery journal step. Seeding only: nothing here runs once the
/// first snapshot has been taken.
fn seed_step(
    store: &SqliteStore,
    fixture: &Fixture,
    task: &str,
    id: &str,
    kind: ActionKind,
    target_ref: Option<&str>,
    destination: &str,
) {
    let action = ApprovalAction::new(
        ApprovalActionKind::Delivery(kind),
        ParameterDigest::from_sha256([7; 32]),
        fixture.candidate.clone(),
        task,
    )
    .expect("valid delivery action");
    let outcome = match kind {
        ActionKind::ReportPublish => DeliveryOutcome::Reported,
        _ => DeliveryOutcome::ReviewRefCreated,
    };
    store
        .record_delivery_step(&DeliveryStepRecord {
            id,
            approval_id: "approval-for-journal",
            verification_run_id: "verification-for-journal",
            action: &action,
            destination,
            target_ref,
            outcome,
            diff_summary: None,
        })
        .expect("persist delivery step");
}

/// A fixture with a dirty primary checkout, a registered dirty task worktree, a
/// live store and one task per classification case. Returns the expectations.
fn fixture() -> (Fixture, Vec<Expected>) {
    let root = tempfile::tempdir().expect("create unique fixture directory");
    let repo = init_repo(root.path(), "repo");
    let foreign_repo = init_repo(root.path(), "foreign");
    // A dirty primary checkout: everything below must leave it untouched.
    fs::write(repo.join("other.txt"), "other base\nlocal edit\n").unwrap();
    fs::write(repo.join("primary-untracked.txt"), "operator scratch\n").unwrap();

    let candidate = git(&repo, &["rev-parse", "main"]);
    let tree = git(&repo, &["rev-parse", "main^{tree}"]);
    let unrelated = git(
        &repo,
        &["commit-tree", &tree, "-m", "unrelated review target"],
    );

    // A registered task worktree with dirty tracked and untracked bytes.
    let worktree = root.path().join("worktrees").join("t-running");
    fs::create_dir_all(worktree.parent().unwrap()).unwrap();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "orchestrator/t-running",
            worktree.to_str().unwrap(),
            "main",
        ],
    );
    fs::write(worktree.join("sample.txt"), "base\nagent edit\n").unwrap();
    fs::write(worktree.join("agent-untracked.txt"), "agent scratch\n").unwrap();

    let fixture = Fixture {
        project: toplevel(&repo),
        foreign: toplevel(&foreign_repo),
        store_path: root.path().join("data").join("store.sqlite"),
        _root: root,
        repo,
        worktree,
        candidate,
        unrelated,
    };

    let repo = &fixture.repo;
    let candidate = &fixture.candidate;
    let local = fixture.project.as_str();

    // Git state per case.
    git(repo, &["update-ref", &review_ref("t-confirmed"), candidate]);
    git(
        repo,
        &["update-ref", &review_ref("t-ambiguous"), &fixture.unrelated],
    );
    git(
        repo,
        &["update-ref", &review_ref("t-partial-landed"), candidate],
    );
    // DWIM lookalike: only `refs/heads/<review ref>` exists, so the real ref is absent.
    git(
        repo,
        &[
            "update-ref",
            &format!("refs/heads/{}", review_ref("t-dwim")),
            candidate,
        ],
    );
    // A symbolic review ref resolving to the candidate, and a dangling one.
    git(
        repo,
        &["symbolic-ref", &review_ref("t-symref"), "refs/heads/main"],
    );
    git(
        repo,
        &[
            "symbolic-ref",
            &review_ref("t-dangling"),
            "refs/heads/victim",
        ],
    );
    // The destination-mismatch task has its ref at the candidate locally; the
    // record names another repository, so it must still not be Confirmed.
    git(repo, &["update-ref", &review_ref("t-foreign"), candidate]);

    fs::create_dir_all(fixture.store_path.parent().unwrap()).unwrap();
    let store = SqliteStore::new(&fixture.store_path).expect("open store");
    store
        .register_project(&Project {
            id: fixture.project.clone(),
            root_path: fixture.project.clone(),
            settings: serde_json::json!({}),
        })
        .unwrap();

    // A running task with an attempt, a session and a workspace: task-state rows
    // that reconciliation must not touch.
    seed_task(&store, &fixture.project, "t-running");
    let attempt = Attempt {
        id: "t-running-attempt-1".to_owned(),
        task_id: "t-running".to_owned(),
        attempt_number: 1,
        state: AttemptState::Pending,
    };
    store.add_attempt("t-running", &attempt).unwrap();
    store
        .transition("t-running", TaskEvent::ExecutionStarted)
        .unwrap();
    store
        .update_attempt_state("t-running", &attempt.id, AttemptState::Running)
        .unwrap();
    let session = Session {
        id: "t-running-session-1".to_owned(),
        attempt_id: attempt.id,
        adapter: "codex".to_owned(),
        state: SessionState::Spawning,
    };
    store.add_session("t-running", &session).unwrap();
    store
        .update_session_state("t-running", &session.id, SessionState::Running)
        .unwrap();
    store
        .add_workspace(&Workspace {
            id: "t-running-workspace-1".to_owned(),
            task_id: "t-running".to_owned(),
            path: fixture.worktree.display().to_string(),
            branch: "orchestrator/t-running".to_owned(),
            base_commit: fixture.candidate.clone(),
        })
        .unwrap();

    let mut expected = Vec::new();
    let mut local_review = |task: &str,
                            suffix: &str,
                            target: Option<String>,
                            destination: &str,
                            classification,
                            observed: Option<String>| {
        seed_task_once(&store, &fixture.project, task);
        let id = step_id(task, suffix);
        seed_step(
            &store,
            &fixture,
            task,
            &id,
            ActionKind::LocalReviewMerge,
            target.as_deref(),
            destination,
        );
        expected.push(Expected {
            id,
            classification,
            observed_ref: observed,
        });
    };
    let candidate = Some(fixture.candidate.clone());
    local_review(
        "t-confirmed",
        "review",
        Some(review_ref("t-confirmed")),
        local,
        Confirmed,
        candidate.clone(),
    );
    local_review(
        "t-missing",
        "review",
        Some(review_ref("t-missing")),
        local,
        NotApplied,
        None,
    );
    local_review(
        "t-ambiguous",
        "review",
        Some(review_ref("t-ambiguous")),
        local,
        Ambiguous,
        Some(fixture.unrelated.clone()),
    );
    local_review(
        "t-dwim",
        "review",
        Some(review_ref("t-dwim")),
        local,
        NotApplied,
        None,
    );
    local_review(
        "t-symref",
        "review",
        Some(review_ref("t-symref")),
        local,
        Ambiguous,
        candidate.clone(),
    );
    local_review(
        "t-dangling",
        "review",
        Some(review_ref("t-dangling")),
        local,
        Ambiguous,
        None,
    );
    local_review(
        "t-foreign",
        "review",
        Some(review_ref("t-foreign")),
        &fixture.foreign,
        Ambiguous,
        None,
    );
    local_review(
        "t-partial",
        "landed",
        Some(review_ref("t-partial-landed")),
        local,
        Confirmed,
        candidate.clone(),
    );
    local_review(
        "t-partial",
        "missing",
        Some(review_ref("t-partial-missing")),
        local,
        NotApplied,
        None,
    );
    // Report-only: no review ref to apply, so NotApplied.
    seed_task_once(&store, &fixture.project, "t-report");
    let report = step_id("t-report", "report");
    seed_step(
        &store,
        &fixture,
        "t-report",
        &report,
        ActionKind::ReportPublish,
        None,
        local,
    );
    expected.push(Expected {
        id: report,
        classification: NotApplied,
        observed_ref: None,
    });
    drop(store);
    (fixture, expected)
}

/// `create` fails on a duplicate id, so a task with several steps is seeded once.
fn seed_task_once(store: &SqliteStore, project: &str, task: &str) {
    if store.get(task).is_err() {
        seed_task(store, project, task);
    }
}

/// Mirrors the private `inspect_task_delivery` mapping in `workflow.rs`: store
/// row JSON (`delivery_mode`, `destination`, `target_ref`, `candidate_sha`) to the
/// classifier's record. Unknown or missing modes map to no action (Ambiguous).
fn recorded(rows: &[Value]) -> Vec<RecordedDeliveryStep> {
    let text = |step: &Value, key: &str| step.get(key).and_then(Value::as_str).map(str::to_owned);
    rows.iter()
        .map(|step| RecordedDeliveryStep {
            id: text(step, "id").unwrap_or_default(),
            task_id: text(step, "task_id"),
            action: match text(step, "delivery_mode").as_deref() {
                Some("report_only") => Some(DeliveryAction::ReportOnly),
                Some("local_review") => Some(DeliveryAction::LocalReview),
                _ => None,
            },
            candidate_sha: text(step, "candidate_sha"),
            destination: text(step, "destination").map(PathBuf::from),
            target_ref: text(step, "target_ref"),
        })
        .collect()
}

fn task_ids(rows: &[Value]) -> Vec<String> {
    let mut ids: Vec<String> = rows
        .iter()
        .map(|row| row["task_id"].as_str().expect("task_id").to_owned())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Every durable row reconciliation could conceivably touch.
#[derive(Debug, PartialEq)]
struct StoreSnapshot {
    steps: Vec<Value>,
    step_events: Vec<(String, Vec<Value>)>,
    stream: Vec<StreamEvent>,
    journal: Vec<EventJournalEntry>,
    tasks: Vec<TaskRecord>,
    workspaces: Vec<Workspace>,
}

fn store_snapshot(store: &SqliteStore) -> StoreSnapshot {
    let steps = store.all_delivery_steps().unwrap();
    let step_events = steps
        .iter()
        .map(|step| {
            let id = step["id"].as_str().unwrap().to_owned();
            let events = store.delivery_events(&id).unwrap();
            (id, events)
        })
        .collect();
    let mut tasks = store.list().unwrap();
    tasks.sort_by(|a, b| a.contract.id.cmp(&b.contract.id));
    StoreSnapshot {
        steps,
        step_events,
        stream: store.stream_events_after(0, 1024).unwrap(),
        journal: store.event_journal(0).unwrap(),
        tasks,
        workspaces: store.list_workspaces().unwrap(),
    }
}

/// Git refs (including symbolic targets), per-ref symbolic probes (a dangling
/// symref is invisible to `for-each-ref`), the worktree list and the dirty and
/// untracked bytes of the primary checkout and the task worktree.
#[derive(Debug, PartialEq)]
struct GitSnapshot {
    refs: String,
    symbolic_probes: Vec<(String, Option<i32>, String)>,
    worktree_list: String,
    primary: CheckoutSnapshot,
    task_worktree: CheckoutSnapshot,
}

#[derive(Debug, PartialEq)]
struct CheckoutSnapshot {
    head: (Option<i32>, String),
    status: (Option<i32>, String),
    tracked_dirty: Option<Vec<u8>>,
    untracked: Option<Vec<u8>>,
}

/// Tolerant of a missing checkout, so a removed worktree is a snapshot
/// difference instead of a panic.
fn checkout_snapshot(dir: &Path, tracked: &str, untracked: &str) -> CheckoutSnapshot {
    CheckoutSnapshot {
        head: git_raw(dir, &["rev-parse", "HEAD"]),
        status: git_raw(
            dir,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
        ),
        tracked_dirty: fs::read(dir.join(tracked)).ok(),
        untracked: fs::read(dir.join(untracked)).ok(),
    }
}

fn git_snapshot(fixture: &Fixture) -> GitSnapshot {
    let repo = &fixture.repo;
    let mut probed: Vec<String> = [
        "t-confirmed",
        "t-missing",
        "t-ambiguous",
        "t-symref",
        "t-dangling",
        "t-foreign",
        "t-partial-landed",
        "t-partial-missing",
        "t-dwim",
    ]
    .iter()
    .map(|task| review_ref(task))
    .collect();
    probed.push("refs/heads/victim".to_owned());
    probed.push(format!("refs/heads/{}", review_ref("t-dwim")));
    let symbolic_probes = probed
        .into_iter()
        .map(|name| {
            let (code, out) = git_raw(repo, &["symbolic-ref", "--quiet", &name]);
            (name, code, out)
        })
        .collect();
    GitSnapshot {
        refs: git(
            repo,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname) %(symref)",
            ],
        ),
        symbolic_probes,
        worktree_list: git(repo, &["worktree", "list", "--porcelain"]),
        primary: checkout_snapshot(repo, "other.txt", "primary-untracked.txt"),
        task_worktree: checkout_snapshot(&fixture.worktree, "sample.txt", "agent-untracked.txt"),
    }
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    store: StoreSnapshot,
    git: GitSnapshot,
}

/// The tolerant snapshot must not let an identical baseline failure pass
/// vacuously: every Git producer succeeded and every dirty byte file was read.
fn assert_baseline_sound(snapshot: &Snapshot) {
    for (name, checkout) in [
        ("primary", &snapshot.git.primary),
        ("task worktree", &snapshot.git.task_worktree),
    ] {
        assert_eq!(checkout.head.0, Some(0), "{name} HEAD producer");
        assert_eq!(checkout.status.0, Some(0), "{name} status producer");
        assert!(checkout.tracked_dirty.is_some(), "{name} tracked bytes");
        assert!(checkout.untracked.is_some(), "{name} untracked bytes");
    }
}

fn snapshot(fixture: &Fixture, store: &SqliteStore) -> Snapshot {
    Snapshot {
        store: store_snapshot(store),
        git: git_snapshot(fixture),
    }
}

fn open_store(fixture: &Fixture) -> SqliteStore {
    SqliteStore::new(&fixture.store_path).expect("open store")
}

/// Reconciles every task's persisted steps, one task at a time as production
/// does, through the live store's read APIs.
fn reconcile_all(
    fixture: &Fixture,
    store: &SqliteStore,
) -> Vec<(String, Vec<DeliveryStepDecision>)> {
    let repository = Repository::new(Path::new(&fixture.project)).expect("open repository");
    let all = store.all_delivery_steps().unwrap();
    task_ids(&all)
        .into_iter()
        .map(|task| {
            let rows = store.delivery_steps_for_task(&task).unwrap();
            let plan = reconcile_delivery_steps(&repository, &recorded(&rows)).unwrap();
            (task, plan.steps)
        })
        .collect()
}

/// Seeds nothing: snapshot, reconcile, let `between` act (a no-op for the real
/// proof, a deliberate writer for the mutation checks), then snapshot again, and
/// once more after reopening the store from disk.
fn run_proof(
    fixture: &Fixture,
    between: impl FnOnce(&Fixture, &SqliteStore),
) -> (
    Snapshot,
    Snapshot,
    Snapshot,
    Vec<(String, Vec<DeliveryStepDecision>)>,
) {
    let store = open_store(fixture);
    let before = snapshot(fixture, &store);
    let decisions = reconcile_all(fixture, &store);
    between(fixture, &store);
    let after = snapshot(fixture, &store);
    drop(store);
    let reopened = open_store(fixture);
    let reopened_snapshot = snapshot(fixture, &reopened);
    (before, after, reopened_snapshot, decisions)
}

#[test]
fn reconciliation_leaves_live_store_git_and_worktrees_unchanged_including_after_reopen() {
    let (fixture, expected) = fixture();
    let (before, after, reopened, decisions) = run_proof(&fixture, |_, _| {});

    // Sanity: the snapshot is not vacuous.
    assert_baseline_sound(&before);
    assert_eq!(before.store.steps.len(), expected.len());
    assert!(before
        .store
        .step_events
        .iter()
        .all(|(_, events)| !events.is_empty()));
    assert!(!before.store.stream.is_empty());
    assert!(!before.store.journal.is_empty());
    assert!(before.store.tasks.len() > 1);
    assert_eq!(before.store.workspaces.len(), 1);
    assert!(before.git.refs.contains(&fixture.unrelated));
    assert!(before.git.worktree_list.contains("t-running"));

    assert_eq!(after, before, "reconciliation changed live state");
    assert_eq!(reopened, before, "reopened store differs from the original");

    // Genuine classifications through the live store rows, literally.
    let flat: Vec<&DeliveryStepDecision> = decisions.iter().flat_map(|(_, steps)| steps).collect();
    assert_eq!(flat.len(), expected.len());
    for want in &expected {
        let got = flat
            .iter()
            .find(|decision| decision.record.id == want.id)
            .unwrap_or_else(|| panic!("no decision for {}", want.id));
        assert_eq!(got.classification, want.classification, "step {}", want.id);
        assert_eq!(
            got.observed_ref, want.observed_ref,
            "observed ref of {}",
            want.id
        );
    }
}

#[test]
fn ambiguous_records_stay_in_the_plan_and_in_the_store() {
    let (fixture, _) = fixture();
    let (before, after, reopened, decisions) = run_proof(&fixture, |_, _| {});
    for task in ["t-ambiguous", "t-symref", "t-dangling", "t-foreign"] {
        let steps = &decisions.iter().find(|(id, _)| id == task).expect(task).1;
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].classification, Ambiguous, "{task}");
        // The persisted row is carried into the plan unchanged, including an
        // invalid/foreign destination, so the artifact stays visible for review.
        let row = before
            .store
            .steps
            .iter()
            .find(|row| row["task_id"] == task)
            .unwrap();
        assert_eq!(steps[0].record.id, row["id"].as_str().unwrap());
        assert_eq!(
            steps[0].record.target_ref.as_deref(),
            row["target_ref"].as_str()
        );
        assert_eq!(
            steps[0].record.destination.as_deref(),
            row["destination"].as_str().map(Path::new)
        );
        assert!(after.store.steps.iter().any(|kept| kept == row));
        assert!(reopened.store.steps.iter().any(|kept| kept == row));
    }
    // Neither the lookalike nor the symbolic refs were repaired or removed, and
    // a dangling symref target was not created.
    assert!(after
        .git
        .refs
        .contains(&format!("refs/heads/{}", review_ref("t-dwim"))));
    assert!(after.git.refs.contains(&review_ref("t-symref")));
    assert!(!after.git.refs.contains("refs/heads/victim"));
    assert!(after
        .git
        .symbolic_probes
        .iter()
        .any(|(name, code, target)| name == &review_ref("t-dangling")
            && *code == Some(0)
            && target == "refs/heads/victim"));
}

#[test]
fn each_task_reconciles_only_its_own_steps() {
    let (fixture, expected) = fixture();
    let store = open_store(&fixture);
    let all = store.all_delivery_steps().unwrap();
    let decisions = reconcile_all(&fixture, &store);
    assert_eq!(decisions.len(), task_ids(&all).len());
    let mut seen = 0;
    for (task, steps) in &decisions {
        assert!(!steps.is_empty(), "{task}");
        for step in steps {
            assert_eq!(step.record.task_id.as_deref(), Some(task.as_str()));
            assert!(step.record.id.starts_with(task.as_str()));
        }
        let persisted = store.delivery_steps_for_task(task).unwrap();
        assert_eq!(steps.len(), persisted.len(), "{task}");
        seen += steps.len();
    }
    assert_eq!(seen, expected.len());
    let task_with_two = decisions
        .iter()
        .find(|(task, _)| task == "t-partial")
        .unwrap();
    assert_eq!(task_with_two.1.len(), 2);
}

#[test]
fn per_task_evidence_summarises_the_live_rows() {
    let (fixture, _) = fixture();
    let store = open_store(&fixture);
    let evidence: Vec<(String, DeliveryEvidence)> = reconcile_all(&fixture, &store)
        .into_iter()
        .map(|(task, steps)| (task, delivery_evidence(&steps)))
        .collect();
    let of = |task: &str| evidence.iter().find(|(id, _)| id == task).unwrap().1;
    assert_eq!(of("t-confirmed"), DeliveryEvidence::Confirmed);
    assert_eq!(of("t-missing"), DeliveryEvidence::NotApplied);
    assert_eq!(of("t-report"), DeliveryEvidence::NotApplied);
    assert_eq!(of("t-dwim"), DeliveryEvidence::NotApplied);
    assert_eq!(of("t-partial"), DeliveryEvidence::Partial);
    for task in ["t-ambiguous", "t-symref", "t-dangling", "t-foreign"] {
        assert_eq!(of(task), DeliveryEvidence::Ambiguous, "{task}");
    }
    // A task with no steps has no delivery evidence.
    assert_eq!(delivery_evidence(&[]), DeliveryEvidence::None);
}

#[test]
fn store_row_json_maps_to_the_classifier_record() {
    let (fixture, _) = fixture();
    let store = open_store(&fixture);
    let rows = store.delivery_steps_for_task("t-confirmed").unwrap();
    let mapped = recorded(&rows);
    assert_eq!(
        mapped,
        vec![RecordedDeliveryStep {
            id: "t-confirmed-review".to_owned(),
            task_id: Some("t-confirmed".to_owned()),
            action: Some(DeliveryAction::LocalReview),
            candidate_sha: Some(fixture.candidate.clone()),
            destination: Some(PathBuf::from(&fixture.project)),
            target_ref: Some(review_ref("t-confirmed")),
        }]
    );
    let report = recorded(&store.delivery_steps_for_task("t-report").unwrap());
    assert_eq!(report[0].action, Some(DeliveryAction::ReportOnly));
    assert_eq!(report[0].target_ref, None);

    // A missing or unknown mode in the row JSON fails closed to Ambiguous, with
    // the observed ref still visible. Mutated in memory only, never in the store.
    let repository = Repository::new(Path::new(&fixture.project)).unwrap();
    for mode in [Value::Null, Value::String("apply".to_owned())] {
        let mut row = rows[0].clone();
        row["delivery_mode"] = mode.clone();
        let plan = reconcile_delivery_steps(&repository, &recorded(&[row])).unwrap();
        assert_eq!(plan.steps[0].classification, Ambiguous, "mode {mode}");
        assert_eq!(plan.steps[0].observed_ref, Some(fixture.candidate.clone()));
        assert!(plan.has_ambiguous_steps());
    }
    let mut row = rows[0].clone();
    row.as_object_mut().unwrap().remove("delivery_mode");
    let plan = reconcile_delivery_steps(&repository, &recorded(&[row])).unwrap();
    assert_eq!(plan.steps[0].classification, Ambiguous);
}

/// Mutation checks: the same snapshot comparison must FAIL when a deliberately
/// writing variant runs between the snapshots, so a green proof above means
/// something. Each writer stands for a side effect reconciliation must never have.
fn assert_detected(label: &str, between: impl FnOnce(&Fixture, &SqliteStore)) {
    let (fixture, _) = fixture();
    let (before, after, _, _) = run_proof(&fixture, between);
    assert_baseline_sound(&before);
    assert_ne!(after, before, "mutation not detected: {label}");
}

#[test]
fn snapshot_detects_a_created_ref() {
    assert_detected("update-ref create", |fixture, _| {
        git(
            &fixture.repo,
            &["update-ref", &review_ref("t-missing"), &fixture.candidate],
        );
    });
}

#[test]
fn snapshot_detects_a_moved_ref() {
    assert_detected("update-ref move", |fixture, _| {
        git(
            &fixture.repo,
            &["update-ref", &review_ref("t-ambiguous"), &fixture.candidate],
        );
    });
}

#[test]
fn snapshot_detects_a_deleted_ref() {
    assert_detected("update-ref -d", |fixture, _| {
        git(
            &fixture.repo,
            &["update-ref", "-d", &review_ref("t-confirmed")],
        );
    });
}

#[test]
fn snapshot_detects_a_dangling_symref_write_through() {
    assert_detected("write through dangling symref", |fixture, _| {
        git(
            &fixture.repo,
            &["update-ref", &review_ref("t-dangling"), &fixture.candidate],
        );
    });
}

#[test]
fn snapshot_detects_a_removed_worktree() {
    assert_detected("worktree remove", |fixture, _| {
        git(
            &fixture.repo,
            &[
                "worktree",
                "remove",
                "--force",
                fixture.worktree.to_str().unwrap(),
            ],
        );
    });
}

#[test]
fn snapshot_detects_changed_worktree_bytes() {
    assert_detected("worktree byte change", |fixture, _| {
        fs::write(fixture.worktree.join("agent-untracked.txt"), "changed\n").unwrap();
    });
    assert_detected("primary byte change", |fixture, _| {
        fs::write(fixture.repo.join("other.txt"), "other base\nreverted\n").unwrap();
    });
}

#[test]
fn snapshot_detects_a_store_write() {
    assert_detected("extra delivery step", |fixture, store| {
        seed_step(
            store,
            fixture,
            "t-missing",
            "t-missing-extra",
            ActionKind::LocalReviewMerge,
            Some(&review_ref("t-missing")),
            &fixture.project,
        );
    });
    assert_detected("session state change", |_, store| {
        store
            .update_session_state("t-running", "t-running-session-1", SessionState::Aborted)
            .unwrap();
    });
}
