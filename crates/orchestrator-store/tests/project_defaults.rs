//! Per-project defaults and the settings switch (DPES-1105 phase 4b, migration 0043).
//!
//! These tests use the store's public API. A second raw connection reads the tables and tries
//! the writes the database itself must refuse, as another process would.

use std::fs;
use std::io;
use std::path::Path;

use orchestrator_core::Project;
use orchestrator_store::{
    LoadPatch, ProjectDefaultsError, ProjectDefaultsPatch, ProjectDefaultsRow, SettingsPatch,
    SqliteStore, StoreError,
};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection, as another process would, with `binds` bound in
/// order, and reports whether the database took it.
fn try_sql(path: &Path, sql: &str, binds: &[&str]) -> Result<(), sqlx::Error> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        // Test-only helper: every statement passed in is a literal written in this file.
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for value in binds {
            query = query.bind(*value);
        }
        query.execute(&mut connection).await?;
        Ok::<(), sqlx::Error>(())
    })
}

/// The first column of every row `sql` returns, as text, read on a second connection.
fn scalars(path: &Path, sql: &'static str) -> Vec<String> {
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&mut connection)
            .await
            .unwrap()
    })
}

/// A store with the project `p1` registered. Returns the database path with it.
fn store_with_project(directory: &Path) -> (SqliteStore, std::path::PathBuf) {
    let path = directory.join("store.sqlite");
    let store = SqliteStore::new(&path).unwrap();
    register(&store, "p1");
    (store, path)
}

fn register(store: &SqliteStore, id: &str) {
    store
        .register_project(&Project {
            id: id.to_owned(),
            root_path: format!("/work/{id}"),
            settings: serde_json::json!({}),
        })
        .unwrap();
}

fn refusal(error: &StoreError) -> Option<ProjectDefaultsError> {
    match error {
        StoreError::Io(cause) if cause.kind() == io::ErrorKind::InvalidInput => cause
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ProjectDefaultsError>())
            .copied(),
        _ => None,
    }
}

const ROW_COUNT: &str = "SELECT CAST(COUNT(*) AS TEXT) FROM project_defaults";

#[test]
fn migration_0043_applies_to_a_0042_database_and_keeps_projects_and_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let legacy_dir = directory.path().join("legacy-migrations");
    fs::create_dir(&legacy_dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    for entry in fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap().path();
        let version: u32 = entry
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split('_')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        if version <= 42 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        Migrator::new(legacy_dir.as_path())
            .await
            .unwrap()
            .run(&mut connection)
            .await
            .unwrap();
        let newest: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(newest, 42);
        sqlx::query(
            "INSERT INTO projects (id, root_path, settings) VALUES ('old', '/work/old', '{}')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query("UPDATE orchestrator_settings SET load_cpu_percent = 70")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
    });

    // Opening the store applies 0043 to the database that already holds a project.
    let store = SqliteStore::new(&path).unwrap();
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 43"
        ),
        vec!["1"]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(id, root_path, archived_at) FROM projects"
        ),
        vec![r#"["old","/work/old",null]"#]
    );
    assert_eq!(scalars(&path, ROW_COUNT), vec!["0"]);
    let settings = store.orchestrator_settings().unwrap();
    assert_eq!(settings.load.cpu, 70);
    assert!(!settings.chat_pr_delivery);
    assert_eq!(
        store.project_defaults_or_local("old").unwrap(),
        ProjectDefaultsRow::local("old")
    );
}

#[test]
fn a_project_with_no_row_reads_as_local_only_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());

    assert_eq!(store.project_defaults("p1").unwrap(), None);
    let defaults = store.project_defaults_or_local("p1").unwrap();
    assert_eq!(defaults.mode, "local-only");
    assert_eq!(defaults.base_branch, None);
    assert_eq!(defaults.branch_prefix, None);
    assert!(defaults.checks.is_empty());
    assert_eq!(defaults.merge_method, "squash");
    // An empty patch and a list read write nothing either.
    assert_eq!(
        store
            .update_project_defaults("p1", &ProjectDefaultsPatch::default())
            .unwrap(),
        defaults
    );
    assert!(store.list_project_defaults().unwrap().is_empty());
    assert_eq!(scalars(&path, ROW_COUNT), vec!["0"]);
}

#[test]
fn a_patch_changes_only_the_named_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    let checks = vec![vec!["cargo".to_owned(), "test".to_owned()]];

    let first = store
        .update_project_defaults(
            "p1",
            &ProjectDefaultsPatch {
                mode: Some("direct-pr".to_owned()),
                base_branch: Some(Some("main".to_owned())),
                branch_prefix: Some(Some("team/".to_owned())),
                checks: Some(checks.clone()),
                merge_method: Some("rebase".to_owned()),
            },
        )
        .unwrap();
    assert_eq!(first.mode, "direct-pr");
    assert_eq!(first.checks, checks);
    assert_eq!(scalars(&path, ROW_COUNT), vec!["1"]);

    // Park the timestamp so a write shows even within one second.
    try_sql(
        &path,
        "UPDATE project_defaults SET updated_at = '2000-01-01 00:00:00'",
        &[],
    )
    .unwrap();
    let second = store
        .update_project_defaults(
            "p1",
            &ProjectDefaultsPatch {
                merge_method: Some("merge".to_owned()),
                ..ProjectDefaultsPatch::default()
            },
        )
        .unwrap();
    let mut expected = first.clone();
    expected.merge_method = "merge".to_owned();
    assert_ne!(second.updated_at, "2000-01-01 00:00:00");
    expected.updated_at.clone_from(&second.updated_at);
    assert_eq!(second, expected);

    // Clearing a branch is a named change; the other fields stay.
    let third = store
        .update_project_defaults(
            "p1",
            &ProjectDefaultsPatch {
                base_branch: Some(None),
                ..ProjectDefaultsPatch::default()
            },
        )
        .unwrap();
    assert_eq!(third.base_branch, None);
    assert_eq!(third.branch_prefix.as_deref(), Some("team/"));
    assert_eq!(third.mode, "direct-pr");

    // A refused patch writes nothing, and an unregistered project is not found.
    let error = store
        .update_project_defaults(
            "p1",
            &ProjectDefaultsPatch {
                mode: Some("local-only".to_owned()),
                branch_prefix: Some(Some("x".to_owned())),
                ..ProjectDefaultsPatch::default()
            },
        )
        .unwrap_err();
    assert_eq!(refusal(&error), Some(ProjectDefaultsError::BranchPrefix));
    assert_eq!(
        store.project_defaults("p1").unwrap().unwrap().mode,
        "direct-pr"
    );
    assert!(matches!(
        store.update_project_defaults(
            "ghost",
            &ProjectDefaultsPatch {
                mode: Some("direct-pr".to_owned()),
                ..ProjectDefaultsPatch::default()
            }
        ),
        Err(StoreError::RecordNotFound { .. })
    ));

    // The full write replaces the row, the list shows it, a reset keeps it, delete is refused.
    let mut replacement = ProjectDefaultsRow::local("p1");
    replacement.mode = "no-mistakes".to_owned();
    store.set_project_defaults(&replacement).unwrap();
    let stored = store.project_defaults("p1").unwrap().unwrap();
    assert_eq!(stored.mode, "no-mistakes");
    assert_eq!(stored.branch_prefix, None);
    assert_eq!(store.list_project_defaults().unwrap().len(), 1);
    store.reset_project_defaults("p1").unwrap();
    assert_eq!(
        store.project_defaults("p1").unwrap().unwrap().mode,
        "local-only"
    );
    assert!(try_sql(&path, "DELETE FROM project_defaults", &[]).is_err());
    assert_eq!(scalars(&path, ROW_COUNT), vec!["1"]);
}

#[test]
fn the_mode_check_refuses_an_unknown_mode() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());

    // The database refuses it, a value the store would also refuse.
    assert!(try_sql(
        &path,
        "INSERT INTO project_defaults (project_id, mode) VALUES ('p1', 'direct-PR')",
        &[]
    )
    .is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO project_defaults (project_id, merge_method) VALUES ('p1', 'fast-forward')",
        &[]
    )
    .is_err());
    // The database refuses a control character and a prefix that is too short.
    assert!(try_sql(
        &path,
        "INSERT INTO project_defaults (project_id, base_branch) VALUES ('p1', ?)",
        &["ma\u{1}in"]
    )
    .is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO project_defaults (project_id, branch_prefix) VALUES ('p1', 'x')",
        &[]
    )
    .is_err());
    assert!(try_sql(
        &path,
        "INSERT INTO project_defaults (project_id, base_branch) VALUES ('p1', 'main')",
        &[]
    )
    .is_ok());
    assert_eq!(scalars(&path, ROW_COUNT), vec!["1"]);

    // The store refuses each with a typed error before it writes.
    let refused = |mutate: fn(&mut ProjectDefaultsRow)| {
        let mut row = ProjectDefaultsRow::local("p1");
        mutate(&mut row);
        refusal(&store.set_project_defaults(&row).unwrap_err())
    };
    assert_eq!(
        refused(|row| row.mode = "direct-PR".to_owned()),
        Some(ProjectDefaultsError::UnknownMode)
    );
    assert_eq!(
        refused(|row| row.merge_method = "ff".to_owned()),
        Some(ProjectDefaultsError::UnknownMergeMethod)
    );
    assert_eq!(
        refused(|row| row.base_branch = Some("ma\nin".to_owned())),
        Some(ProjectDefaultsError::BaseBranch)
    );
    assert_eq!(
        refused(|row| row.base_branch = Some("b".repeat(101))),
        Some(ProjectDefaultsError::BaseBranch)
    );
    assert_eq!(
        refused(|row| row.branch_prefix = Some("p".repeat(65))),
        Some(ProjectDefaultsError::BranchPrefix)
    );
    assert_eq!(
        refused(|row| row.checks = vec![Vec::new()]),
        Some(ProjectDefaultsError::Checks)
    );
    assert_eq!(
        refused(|row| row.checks = vec![vec!["a\0b".to_owned()]]),
        Some(ProjectDefaultsError::Checks)
    );
    assert_eq!(
        refused(|row| row.checks = vec![vec!["a".to_owned()]; 9]),
        Some(ProjectDefaultsError::Checks)
    );
    assert_eq!(
        refused(|row| row.checks = vec![vec!["a".repeat(256); 16]; 2]),
        Some(ProjectDefaultsError::Checks)
    );
    // Only the raw insert above wrote a row, and it kept the default mode.
    assert_eq!(
        store.project_defaults("p1").unwrap().unwrap().mode,
        "local-only"
    );
}

#[test]
fn chat_pr_delivery_is_off_by_default_and_changes_only_by_a_patch() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    assert!(!store.orchestrator_settings().unwrap().chat_pr_delivery);

    // Setting a project to a PR mode does not turn it on.
    store
        .update_project_defaults(
            "p1",
            &ProjectDefaultsPatch {
                mode: Some("direct-pr".to_owned()),
                ..ProjectDefaultsPatch::default()
            },
        )
        .unwrap();
    assert!(!store.orchestrator_settings().unwrap().chat_pr_delivery);
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(chat_pr_delivery AS TEXT) FROM orchestrator_settings"
        ),
        vec!["0"]
    );

    let on = SettingsPatch {
        chat_pr_delivery: Some(true),
        ..SettingsPatch::default()
    };
    assert!(
        store
            .update_orchestrator_settings(&on)
            .unwrap()
            .chat_pr_delivery
    );
    assert!(store.orchestrator_settings().unwrap().chat_pr_delivery);
    // A patch that does not name it leaves it on, and a patch can turn it off.
    store
        .update_orchestrator_settings(&SettingsPatch {
            load: LoadPatch {
                cpu: Some(75),
                ..LoadPatch::default()
            },
            ..SettingsPatch::default()
        })
        .unwrap();
    assert!(store.orchestrator_settings().unwrap().chat_pr_delivery);
    let off = SettingsPatch {
        chat_pr_delivery: Some(false),
        ..SettingsPatch::default()
    };
    assert!(
        !store
            .update_orchestrator_settings(&off)
            .unwrap()
            .chat_pr_delivery
    );
    assert!(try_sql(
        &path,
        "UPDATE orchestrator_settings SET chat_pr_delivery = 2",
        &[]
    )
    .is_err());
}

#[test]
fn chat_pr_delivery_is_off_by_default_and_a_settings_patch_leaves_other_columns_alone() {
    let directory = tempfile::tempdir().unwrap();
    let (store, path) = store_with_project(directory.path());
    let before = store.orchestrator_settings().unwrap();
    assert!(!before.chat_pr_delivery);

    // A change to a load limit leaves the switch off.
    store
        .update_orchestrator_settings(&SettingsPatch {
            load: LoadPatch {
                cpu: Some(70),
                ..LoadPatch::default()
            },
            ..SettingsPatch::default()
        })
        .unwrap();
    let after_load = store.orchestrator_settings().unwrap();
    assert!(!after_load.chat_pr_delivery);

    // Naming the switch changes that column and nothing else.
    let after_on = store
        .update_orchestrator_settings(&SettingsPatch {
            chat_pr_delivery: Some(true),
            ..SettingsPatch::default()
        })
        .unwrap();
    let mut expected = after_load.clone();
    expected.chat_pr_delivery = true;
    assert_eq!(after_on, expected);
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(chat_pr_delivery, load_cpu_percent) FROM orchestrator_settings"
        ),
        vec!["[1,70]"]
    );
}

#[test]
fn clearing_in_place_removes_only_the_flag_from_the_project_settings() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
    store
        .register_project(&Project {
            id: "p1".to_owned(),
            root_path: "/work/p1".to_owned(),
            settings: serde_json::json!({"in_place": true, "keep": 1}),
        })
        .unwrap();

    store.clear_in_place("p1").unwrap();
    store.clear_in_place("unknown").unwrap();

    let project = store.get_project("p1").unwrap();
    assert!(!project.is_in_place());
    assert_eq!(project.settings, serde_json::json!({"keep": 1}));
}
