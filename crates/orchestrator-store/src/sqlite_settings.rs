//! The orchestrator's settings (DPES-1105 phase 2c, migration 0039).
//!
//! One row holds the three machine load limits. The database keeps that row single and
//! each limit between 50 and 100, and [`SqliteStore::update_orchestrator_settings`] checks
//! the same range first, so a bad value is a typed error and never a raw constraint
//! message. A limit of 100 turns that meter's check off.

use std::io;

use orchestrator_core::load_gate::{LoadLimits, LoadMeter};
use sqlx::SqliteConnection;

use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

/// The settings row. Later phases add fields at the end; none are removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrchestratorSettings {
    pub load: LoadLimits,
}

/// `None` leaves a limit as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadPatch {
    pub cpu: Option<u8>,
    pub memory: Option<u8>,
    pub disk: Option<u8>,
}

/// A change to the settings row. A field that is not named stays as it is. Later phases
/// add fields at the end; none are removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsPatch {
    pub load: LoadPatch,
}

impl SettingsPatch {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl SqliteStore {
    /// The settings row. A new store holds the defaults: 80, 85 and 90.
    pub fn orchestrator_settings(&self) -> Result<OrchestratorSettings, StoreError> {
        self.call(|pool| async move {
            let mut connection = pool.acquire().await?;
            let load = read_limits(&mut connection).await?;
            Ok(OrchestratorSettings { load })
        })
    }

    /// Changes the limits `patch` names, in one transaction: reads the row, merges the
    /// patch, checks the three limits together, then sets only the columns the patch
    /// names and bumps `updated_at`. Every other column keeps its value. A limit outside
    /// 50 to 100 fails with an `InvalidInput` I/O error that names the meter, before
    /// anything is written. An empty patch writes nothing. Returns the row as stored.
    pub fn update_orchestrator_settings(
        &self,
        patch: &SettingsPatch,
    ) -> Result<OrchestratorSettings, StoreError> {
        if patch.is_empty() {
            return self.orchestrator_settings();
        }
        let patch = patch.clone();
        self.call(move |pool| async move {
            // IMMEDIATE takes the write lock before the read, so two updates cannot merge
            // into the same old row and lose one of the changes.
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let current = read_limits(&mut transaction).await?;
            let load = LoadLimits::new(
                patch.load.cpu.unwrap_or(current.cpu),
                patch.load.memory.unwrap_or(current.memory),
                patch.load.disk.unwrap_or(current.disk),
            )
            .map_err(|error| out_of_range(error.0))?;
            // Set only the columns the patch names. Later phases add columns to this row,
            // and a write must never touch one it was not given.
            let named = [
                ("load_cpu_percent", patch.load.cpu),
                ("load_memory_percent", patch.load.memory),
                ("load_disk_percent", patch.load.disk),
            ]
            .into_iter()
            .filter_map(|(column, value)| Some((column, value?)))
            .collect::<Vec<_>>();
            let assignments = named
                .iter()
                .map(|(column, _)| format!("{column} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "UPDATE orchestrator_settings SET {assignments}, \
                 updated_at = CURRENT_TIMESTAMP WHERE id = 1"
            );
            // Safe to assert: `assignments` is built only from the fixed column names above, and
            // every value is bound.
            let mut update = sqlx::query(sqlx::AssertSqlSafe(sql));
            for (_, value) in &named {
                update = update.bind(i64::from(*value));
            }
            update.execute(&mut *transaction).await?;
            transaction.commit().await?;
            Ok(OrchestratorSettings { load })
        })
    }
}

async fn read_limits(connection: &mut SqliteConnection) -> Result<LoadLimits, StoreError> {
    let (cpu, memory, disk) = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT load_cpu_percent, load_memory_percent, load_disk_percent \
         FROM orchestrator_settings WHERE id = 1",
    )
    .fetch_one(connection)
    .await?;
    // The table's CHECKs keep these in range, so a row that is not is a damaged database.
    let stored = |value: i64| u8::try_from(value).map_err(|_| damaged());
    LoadLimits::new(stored(cpu)?, stored(memory)?, stored(disk)?).map_err(|_| damaged())
}

fn damaged() -> StoreError {
    corrupt("the stored load limits are not three percentages from 50 to 100".to_owned())
}

fn out_of_range(meter: LoadMeter) -> StoreError {
    StoreError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("the {} load limit must be from 50 to 100", meter.label()),
    ))
}
