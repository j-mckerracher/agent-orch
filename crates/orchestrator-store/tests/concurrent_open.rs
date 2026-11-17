//! Concurrent first opens of one store file. Every opener must succeed, and the
//! file must end with each migration recorded exactly once.

use orchestrator_store::SqliteStore;
use sqlx::Row;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread;

const OPENERS: usize = 8;
const ROUNDS: usize = 10;

#[test]
fn concurrent_first_opens_all_succeed_and_record_each_migration_once() {
    let migrator = sqlx::migrate!("./migrations");
    let expected_count = migrator.iter().count() as i64;
    let expected_version = migrator
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap();

    for round in 0..ROUNDS {
        let directory = tempfile::tempdir().expect("temporary data directory");
        let path = directory.path().join("store.sqlite");
        let barrier = Arc::new(Barrier::new(OPENERS));
        let openers: Vec<_> = (0..OPENERS)
            .map(|_| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    SqliteStore::new(&path)
                        .map(drop)
                        .map_err(|error| error.to_string())
                })
            })
            .collect();
        for (opener, handle) in openers.into_iter().enumerate() {
            let outcome = handle.join().expect("opener thread");
            assert!(
                outcome.is_ok(),
                "round {round}, opener {opener}: concurrent first open failed: {outcome:?}"
            );
        }
        let (count, version) = recorded_migrations(&path);
        assert_eq!(
            count, expected_count,
            "round {round}: migrations recorded once each"
        );
        assert_eq!(version, expected_version, "round {round}: schema version");
    }
}

fn recorded_migrations(path: &Path) -> (i64, i64) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let url = format!("sqlite://{}", path.display());
        let pool = sqlx::SqlitePool::connect(&url).await.expect("connect");
        let row = sqlx::query("SELECT COUNT(*) AS n, MAX(version) AS v FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .expect("read migration table");
        pool.close().await;
        (row.get("n"), row.get("v"))
    })
}
