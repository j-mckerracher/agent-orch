//! The orchestrator settings row: machine load limits (DPES-1105 phase 2c, migration 0039).
//!
//! These tests use the store's public API. A second raw connection reads the table and
//! tries the writes the database itself must refuse, as another process would.

use std::fs;
use std::io;
use std::path::Path;
use std::thread;

use orchestrator_store::{LoadPatch, SettingsPatch, SqliteStore, StoreError};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Connection;

const PARKED: &str = "2000-01-01 00:00:00";
const UPDATED_AT: &str = "SELECT updated_at FROM orchestrator_settings";
const PROPOSAL_DUMP: &str = "SELECT json_array(rowid, id, scope, turn_id, idx, kind, params_json, \
    digest, subject, depends_on, state, result_ref, error_code, created_at, updated_at) \
    FROM action_proposals ORDER BY rowid";

fn open(directory: &Path) -> SqliteStore {
    SqliteStore::new(&directory.join("store.sqlite")).unwrap()
}

#[test]
fn dispatch_defaults_persist_and_partial_updates_preserve_them() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    assert_eq!(
        store.orchestrator_settings().unwrap().dispatch_defaults,
        Default::default()
    );
    let defaults = serde_json::from_value(serde_json::json!({
        "default_agent": "codex",
        "providers": {"codex": {"model": "gpt-6-luna", "effort": "low"}, "claude-code": {"model": "sonnet"}}
    })).unwrap();
    store
        .update_orchestrator_settings(&SettingsPatch {
            dispatch_defaults: Some(defaults),
            ..Default::default()
        })
        .unwrap();
    updated(&store, &cpu(75));
    let saved = store.orchestrator_settings().unwrap();
    drop(store);
    let store = open(directory.path());
    assert_eq!(store.orchestrator_settings().unwrap(), saved);
    store
        .update_orchestrator_settings(&SettingsPatch {
            dispatch_defaults: Some(Default::default()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        store.orchestrator_settings().unwrap().dispatch_defaults,
        Default::default()
    );
    assert_eq!(stored(&store).0, 75);
    let path = directory.path().join("store.sqlite");
    try_sql(
        &path,
        "UPDATE orchestrator_settings SET dispatch_defaults = 'broken'",
        &[],
    )
    .unwrap();
    assert!(
        store.orchestrator_settings().is_err(),
        "corruption must not silently clear defaults"
    );
}

/// The three limits the store holds now, as (cpu, memory, disk).
fn stored(store: &SqliteStore) -> (u8, u8, u8) {
    let settings = store.orchestrator_settings().unwrap();
    (settings.load.cpu, settings.load.memory, settings.load.disk)
}

/// Applies `patch` and returns the limits it left, as (cpu, memory, disk).
fn updated(store: &SqliteStore, patch: &SettingsPatch) -> (u8, u8, u8) {
    let settings = store.update_orchestrator_settings(patch).unwrap();
    (settings.load.cpu, settings.load.memory, settings.load.disk)
}

fn is_invalid_input(error: &StoreError) -> bool {
    matches!(error, StoreError::Io(cause) if cause.kind() == io::ErrorKind::InvalidInput)
}

fn cpu(value: u8) -> SettingsPatch {
    SettingsPatch {
        load: LoadPatch {
            cpu: Some(value),
            ..LoadPatch::default()
        },
        ..SettingsPatch::default()
    }
}

fn memory(value: u8) -> SettingsPatch {
    SettingsPatch {
        load: LoadPatch {
            memory: Some(value),
            ..LoadPatch::default()
        },
        ..SettingsPatch::default()
    }
}

fn disk(value: u8) -> SettingsPatch {
    SettingsPatch {
        load: LoadPatch {
            disk: Some(value),
            ..LoadPatch::default()
        },
        ..SettingsPatch::default()
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs one statement on a second connection, as another process would, with
/// `binds` bound in order, and reports whether the database took it.
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

fn run_sql(path: &Path, sql: &str) {
    try_sql(path, sql, &[]).unwrap();
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

/// Sets `updated_at` to a value the clock cannot produce now, so a write shows even
/// within one second.
fn park_updated_at(path: &Path) {
    run_sql(
        path,
        "UPDATE orchestrator_settings SET updated_at = '2000-01-01 00:00:00'",
    );
    assert_eq!(scalars(path, UPDATED_AT), vec![PARKED]);
}

/// Builds the database as 0038 left it: only the migrations up to 0038 applied, holding
/// two pending workstream proposals. Returns the proposals as dumped before 0039 runs.
fn database_at_0038(directory: &Path) -> Vec<String> {
    let legacy_dir = directory.join("legacy-migrations");
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
        if version <= 38 {
            fs::copy(&entry, legacy_dir.join(entry.file_name().unwrap())).unwrap();
        }
    }
    runtime().block_on(async {
        let options = SqliteConnectOptions::new()
            .filename(directory.join("store.sqlite"))
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
        assert_eq!(newest, 38);
        sqlx::query(
            "INSERT INTO conversations (id, target_kind) VALUES ('conversation:global', 'global')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO conversation_turns (id, conversation_id, state, runtime_name, runtime_version) \
             VALUES ('t1', 'conversation:global', 'completed', 'test-runtime', '1.0.0')",
        )
        .execute(&mut connection)
        .await
        .unwrap();
        for (idx, id) in ["proposal-a", "proposal-b"].iter().enumerate() {
            sqlx::query(
                "INSERT INTO action_proposals \
                 (id, scope, turn_id, idx, kind, params_json, digest, subject) \
                 VALUES (?, 'conversation:global', 't1', ?, 'workstream.create', ?, ?, ?)",
            )
            .bind(*id)
            .bind(i64::try_from(idx).unwrap())
            .bind(format!(r#"{{"kind":"workstream.create","title":"{id}"}}"#))
            .bind(format!("{:064x}", idx + 1))
            .bind(format!("Create {id}"))
            .execute(&mut connection)
            .await
            .unwrap();
        }
        let proposals = sqlx::query_scalar::<_, String>(PROPOSAL_DUMP)
            .fetch_all(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        proposals
    })
}

#[test]
fn migration_0039_applies_to_a_0038_database_and_creates_the_default_row() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    database_at_0038(directory.path());
    let tables = scalars(
        &path,
        "SELECT name FROM sqlite_master WHERE name = 'orchestrator_settings'",
    );
    assert!(tables.is_empty());

    // Opening the store applies 0039 to the database that already holds proposals.
    let store = open(directory.path());
    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(success AS TEXT) FROM _sqlx_migrations WHERE version = 39"
        ),
        vec!["1"]
    );
    assert_eq!(
        scalars(
            &path,
            "SELECT json_array(id, load_cpu_percent, load_memory_percent, load_disk_percent) \
             FROM orchestrator_settings"
        ),
        vec!["[1,80,85,90]"]
    );
    assert_eq!(stored(&store), (80, 85, 90));
}

#[test]
fn migration_0039_keeps_the_proposals_of_a_0038_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let proposals_before = database_at_0038(directory.path());
    assert_eq!(proposals_before.len(), 2);

    let store = open(directory.path());
    assert_eq!(scalars(&path, PROPOSAL_DUMP), proposals_before);
    assert!(scalars(&path, "PRAGMA foreign_key_check").is_empty());
    let pending = store.pending_action_proposals(10).unwrap();
    assert_eq!(pending.len(), 2);
}

#[test]
fn a_fresh_store_holds_the_default_limits_80_85_90() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    assert_eq!(stored(&store), (80, 85, 90));
}

#[test]
fn the_settings_row_cannot_be_deleted_or_duplicated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());

    for sql in [
        "DELETE FROM orchestrator_settings",
        "DELETE FROM orchestrator_settings WHERE id = 1",
    ] {
        let error = try_sql(&path, sql, &[]).unwrap_err();
        assert!(
            error.to_string().contains("never deleted"),
            "{sql}: {error}"
        );
    }
    for sql in [
        "INSERT INTO orchestrator_settings (id) VALUES (1)",
        "INSERT INTO orchestrator_settings (id) VALUES (2)",
        "INSERT INTO orchestrator_settings DEFAULT VALUES",
        "UPDATE orchestrator_settings SET id = 2",
    ] {
        assert!(try_sql(&path, sql, &[]).is_err(), "{sql}");
    }

    assert_eq!(
        scalars(
            &path,
            "SELECT CAST(COUNT(*) AS TEXT) FROM orchestrator_settings"
        ),
        vec!["1"]
    );
    assert_eq!(stored(&store), (80, 85, 90));
}

#[test]
fn limits_outside_50_to_100_are_refused_by_the_update_and_by_the_table() {
    const REFUSED_BY_THE_TABLE: [&str; 12] = [
        "UPDATE orchestrator_settings SET load_cpu_percent = 49",
        "UPDATE orchestrator_settings SET load_cpu_percent = 101",
        "UPDATE orchestrator_settings SET load_cpu_percent = -1",
        "UPDATE orchestrator_settings SET load_cpu_percent = NULL",
        "UPDATE orchestrator_settings SET load_memory_percent = 49",
        "UPDATE orchestrator_settings SET load_memory_percent = 101",
        "UPDATE orchestrator_settings SET load_memory_percent = -1",
        "UPDATE orchestrator_settings SET load_memory_percent = NULL",
        "UPDATE orchestrator_settings SET load_disk_percent = 49",
        "UPDATE orchestrator_settings SET load_disk_percent = 101",
        "UPDATE orchestrator_settings SET load_disk_percent = -1",
        "UPDATE orchestrator_settings SET load_disk_percent = NULL",
    ];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());

    // The update refuses each value with a typed error that names the meter and carries
    // no database text. A patch cannot hold a negative number, so the table's own CHECK
    // is what refuses -1 below.
    for value in [0, 49, 101, u8::MAX] {
        for (label, patch) in [
            ("CPU", cpu(value)),
            ("memory", memory(value)),
            ("disk", disk(value)),
        ] {
            let error = store.update_orchestrator_settings(&patch).unwrap_err();
            assert!(is_invalid_input(&error), "{label} {value}: {error:?}");
            let text = error.to_string();
            assert!(
                text.contains(label) && text.contains("50 to 100"),
                "{label} {value}: {text}"
            );
            assert!(!text.contains("constraint"), "{label} {value}: {text}");
        }
    }

    for sql in REFUSED_BY_THE_TABLE {
        assert!(try_sql(&path, sql, &[]).is_err(), "{sql}");
    }
    assert_eq!(stored(&store), (80, 85, 90));
}

#[test]
fn the_limits_50_and_100_are_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    let lowest = SettingsPatch {
        load: LoadPatch {
            cpu: Some(50),
            memory: Some(50),
            disk: Some(50),
        },
        ..Default::default()
    };
    assert_eq!(updated(&store, &lowest), (50, 50, 50));
    let highest = SettingsPatch {
        load: LoadPatch {
            cpu: Some(100),
            memory: Some(100),
            disk: Some(100),
        },
        ..Default::default()
    };
    assert_eq!(updated(&store, &highest), (100, 100, 100));
}

#[test]
fn a_patch_changes_only_the_named_limits_and_survives_a_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());

    assert_eq!(updated(&store, &memory(70)), (80, 70, 90));
    let cpu_and_disk = SettingsPatch {
        load: LoadPatch {
            cpu: Some(60),
            memory: None,
            disk: Some(95),
        },
        ..Default::default()
    };
    assert_eq!(updated(&store, &cpu_and_disk), (60, 70, 95));
    assert_eq!(stored(&store), (60, 70, 95));

    drop(store);
    let reopened = open(directory.path());
    assert_eq!(stored(&reopened), (60, 70, 95));
}

#[test]
fn an_empty_patch_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    store.update_orchestrator_settings(&memory(70)).unwrap();
    park_updated_at(&path);

    assert_eq!(updated(&store, &SettingsPatch::default()), (80, 70, 90));
    assert_eq!(scalars(&path, UPDATED_AT), vec![PARKED]);
}

#[test]
fn a_patch_that_names_a_limit_bumps_updated_at() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    park_updated_at(&path);

    store.update_orchestrator_settings(&cpu(70)).unwrap();
    assert_ne!(scalars(&path, UPDATED_AT), vec![PARKED]);
}

#[test]
fn a_refused_patch_writes_nothing_even_for_its_valid_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    park_updated_at(&path);

    let patch = SettingsPatch {
        load: LoadPatch {
            cpu: Some(60),
            memory: Some(101),
            disk: Some(95),
        },
        ..Default::default()
    };
    assert!(store.update_orchestrator_settings(&patch).is_err());
    assert_eq!(stored(&store), (80, 85, 90));
    assert_eq!(scalars(&path, UPDATED_AT), vec![PARKED]);
}

#[test]
fn concurrent_patches_to_different_limits_do_not_overwrite_each_other() {
    let directory = tempfile::tempdir().unwrap();
    // Two handles are two connections, so their updates really do race.
    let writers: Vec<_> = [cpu as fn(u8) -> SettingsPatch, memory]
        .into_iter()
        .map(|patch| {
            let store = open(directory.path());
            thread::spawn(move || {
                for value in 51..=70 {
                    store.update_orchestrator_settings(&patch(value)).unwrap();
                }
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap();
    }

    let store = open(directory.path());
    assert_eq!(stored(&store), (70, 70, 90));
}

#[test]
fn an_update_changes_only_the_supplied_columns() {
    const DUMP: &str = "SELECT json_array(load_cpu_percent, load_memory_percent, \
        load_disk_percent, later_phase_cursor) FROM orchestrator_settings";
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite");
    let store = open(directory.path());
    // A later phase adds a column to the one row. This one stands in for it.
    run_sql(
        &path,
        "ALTER TABLE orchestrator_settings \
         ADD COLUMN later_phase_cursor TEXT NOT NULL DEFAULT 'start'",
    );
    run_sql(
        &path,
        "UPDATE orchestrator_settings SET later_phase_cursor = 'cursor-42'",
    );
    assert_eq!(scalars(&path, DUMP), vec![r#"[80,85,90,"cursor-42"]"#]);

    let cases = [
        (
            cpu(60),
            ["load_memory_percent", "load_disk_percent"],
            r#"[60,85,90,"cursor-42"]"#,
        ),
        (
            memory(70),
            ["load_cpu_percent", "load_disk_percent"],
            r#"[60,70,90,"cursor-42"]"#,
        ),
        (
            disk(95),
            ["load_cpu_percent", "load_memory_percent"],
            r#"[60,70,95,"cursor-42"]"#,
        ),
    ];
    for (patch, others, expected) in cases {
        // An UPDATE that names one of these columns aborts, even one that writes the
        // value the column already holds, so the patch passes only if its SET leaves
        // them out.
        let guard = format!(
            "CREATE TRIGGER only_the_supplied_columns \
             BEFORE UPDATE OF {}, {}, later_phase_cursor ON orchestrator_settings \
             BEGIN SELECT RAISE(ABORT, 'a column the patch did not name was written'); END",
            others[0], others[1]
        );
        run_sql(&path, &guard);
        store.update_orchestrator_settings(&patch).unwrap();
        run_sql(&path, "DROP TRIGGER only_the_supplied_columns");
        assert_eq!(scalars(&path, DUMP), vec![expected]);
    }
    assert_eq!(stored(&store), (60, 70, 95));
}

#[test]
fn the_home_directory_is_unset_by_default_and_changes_only_by_a_patch_that_names_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = open(directory.path());
    assert_eq!(store.orchestrator_settings().unwrap().home_directory, None);

    let set = |value: Option<&str>| SettingsPatch {
        home_directory: Some(value.map(str::to_owned)),
        ..SettingsPatch::default()
    };
    let saved = store
        .update_orchestrator_settings(&set(Some("/Users/me/Code")))
        .unwrap();
    assert_eq!(saved.home_directory.as_deref(), Some("/Users/me/Code"));

    // A patch that does not name it leaves it alone.
    store.update_orchestrator_settings(&cpu(60)).unwrap();
    let kept = store.orchestrator_settings().unwrap();
    assert_eq!(kept.home_directory.as_deref(), Some("/Users/me/Code"));
    assert_eq!(kept.load.cpu, 60);

    for bad in ["", "bad\0path", &"x".repeat(4097)] {
        let error = store
            .update_orchestrator_settings(&set(Some(bad)))
            .unwrap_err();
        assert!(is_invalid_input(&error), "{bad:?}");
    }
    assert_eq!(
        store
            .orchestrator_settings()
            .unwrap()
            .home_directory
            .as_deref(),
        Some("/Users/me/Code")
    );

    assert_eq!(
        store
            .update_orchestrator_settings(&set(None))
            .unwrap()
            .home_directory,
        None
    );
}
