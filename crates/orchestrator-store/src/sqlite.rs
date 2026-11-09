//! SQLite connection lifecycle for [`SqliteStore`].
//!
//! `SqliteStore::new` opens one SQLx pool connection, configures WAL, foreign
//! keys, and a five-second busy timeout, then applies embedded migrations.
//! Synchronous store calls run on one Tokio worker per handle. SQLite process
//! transactions do not establish a power-loss durability guarantee.
//! Updated: 2026-11-07T22:33:30-07:00.

use std::future::Future;
use std::path::Path;
use std::sync::mpsc::{self, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

use crate::StoreError;
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

type DbJob = Box<dyn FnOnce(&tokio::runtime::Runtime, &SqlitePool) + Send + 'static>;

/// A SQLite database that implements the synchronous [`crate::Store`] contract.
///
/// Register each task's project and any parent or dependency tasks before
/// creating that task. Foreign keys reject missing references.
///
/// Calls on one handle run in order on its worker thread. Separate handles use
/// separate SQLite connections and rely on SQLite's busy timeout to serialize
/// writers. Dropping the last handle closes its pool and joins its worker.
///
/// Updated: 2026-11-07T22:33:30-07:00.
pub struct SqliteStore {
    sender: Option<SyncSender<DbJob>>,
    worker: Option<JoinHandle<()>>,
}

impl SqliteStore {
    /// Opens a database file, creates its parent if needed, and applies migrations.
    pub fn new(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let path = path.to_path_buf();
        let (sender, receiver) = mpsc::sync_channel(0);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("orchestrator-sqlite".to_owned())
            .spawn(move || run_worker(path, receiver, ready_sender))?;

        match ready_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
        {
            Ok(()) => Ok(Self {
                sender: Some(sender),
                worker: Some(worker),
            }),
            Err(error) => {
                drop(sender);
                let _ = worker.join();
                Err(error)
            }
        }
    }

    pub(crate) fn call<T, F, Fut>(&self, operation: F) -> Result<T, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(SqlitePool) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, StoreError>> + Send + 'static,
    {
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        let job = Box::new(
            move |runtime: &tokio::runtime::Runtime, pool: &SqlitePool| {
                let result = runtime.block_on(operation(pool.clone()));
                let _ = response_sender.send(result);
            },
        );
        self.sender
            .as_ref()
            .ok_or(StoreError::WorkerStopped)?
            .send(job)
            .map_err(|_| StoreError::WorkerStopped)?;
        response_receiver
            .recv()
            .map_err(|_| StoreError::WorkerStopped)?
    }
}

impl Drop for SqliteStore {
    fn drop(&mut self) {
        drop(self.sender.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    path: std::path::PathBuf,
    receiver: mpsc::Receiver<DbJob>,
    ready_sender: SyncSender<Result<(), StoreError>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready_sender.send(Err(StoreError::Io(error)));
            return;
        }
    };
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        // Every store transaction starts with `BEGIN IMMEDIATE`. A deferred
        // transaction that reads and then writes fails at once with
        // SQLITE_BUSY_SNAPSHOT when another process commits in between, and
        // the busy timeout does not apply. Taking the write lock up front makes
        // concurrent run owners and the daemon queue for it instead.
        .busy_timeout(Duration::from_secs(5));
    let pool = match runtime.block_on(
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options),
    ) {
        Ok(pool) => pool,
        Err(error) => {
            let _ = ready_sender.send(Err(StoreError::Database(error)));
            return;
        }
    };
    if let Err(error) = runtime.block_on(MIGRATOR.run(&pool)) {
        let _ = ready_sender.send(Err(StoreError::Migration(error)));
        runtime.block_on(pool.close());
        return;
    }
    if ready_sender.send(Ok(())).is_err() {
        runtime.block_on(pool.close());
        return;
    }

    while let Ok(job) = receiver.recv() {
        job(&runtime, &pool);
    }
    runtime.block_on(pool.close());
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    fn migration_versions(store: &SqliteStore) -> Result<Vec<i64>, StoreError> {
        store.call(|pool| async move {
            let table_exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
            )
            .fetch_one(&pool)
            .await?;
            if table_exists == 0 {
                return Ok(Vec::new());
            }
            Ok(sqlx::query("SELECT version FROM _sqlx_migrations WHERE success = 1 ORDER BY version")
                .fetch_all(&pool)
                .await?
                .iter()
                .map(|row| row.get::<i64, _>("version"))
                .collect())
        })
    }

    #[test]
    fn opening_applies_embedded_migrations_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.sqlite");
        let expected: Vec<_> = MIGRATOR.iter().map(|migration| migration.version).collect();

        let store = SqliteStore::new(&path).unwrap();
        assert_eq!(migration_versions(&store).unwrap(), expected);
        drop(store);

        let reopened = SqliteStore::new(&path).unwrap();
        assert_eq!(migration_versions(&reopened).unwrap(), expected);
    }

    #[test]
    fn new_database_uses_wal_mode() {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(&directory.path().join("store.sqlite")).unwrap();
        let mode = store
            .call(|pool| async move {
                Ok(sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                    .fetch_one(&pool)
                    .await?)
            })
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
    }

    #[test]
    fn foreign_keys_reject_orphan_attempts_on_each_handle() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("store.sqlite");
        let first = SqliteStore::new(&path).unwrap();
        let second = SqliteStore::new(&path).unwrap();

        for (store, suffix) in [(&first, "first"), (&second, "second")] {
            let (foreign_keys, rejected) = store
                .call(move |pool| async move {
                    let foreign_keys =
                        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
                            .fetch_one(&pool)
                            .await?;
                    let error = sqlx::query(
                        "INSERT INTO attempts (id, task_id, attempt_number, state) VALUES (?, ?, 1, ?)",
                    )
                    .bind(format!("orphan-{suffix}"))
                    .bind(format!("missing-{suffix}"))
                    .bind("Pending")
                    .execute(&pool)
                    .await
                    .unwrap_err();
                    let rejected = matches!(
                        error,
                        sqlx::Error::Database(database)
                            if database.message().contains("FOREIGN KEY constraint failed")
                    );
                    Ok((foreign_keys, rejected))
                })
                .unwrap();
            assert_eq!(foreign_keys, 1);
            assert!(rejected);
        }
    }
}
