//! The orchestrator's settings (DPES-1105 phase 2c migration 0039, phase 3 migration 0041).
//!
//! One row holds the three machine load limits, the supervision intervals, the wake limits
//! and the monitoring mode. The database keeps that row single and each column in range, and
//! [`SqliteStore::update_orchestrator_settings`] checks the same ranges first, so a bad value
//! is a typed error and never a raw constraint message. A load limit of 100 turns that
//! meter's check off. The rule `minimum interval <= base interval` lives only in
//! [`IntervalPolicy::new`], because `ALTER TABLE` cannot add a table-level CHECK.
//!
//! The wake cursor shares the row but is not a setting: it has its own functions in
//! `sqlite_wake.rs`, and no write here names its column.

use std::collections::BTreeMap;
use std::io;
use std::time::Duration;

use orchestrator_core::contract::ModelSelection;
use orchestrator_core::interval::{IntervalPolicy, IntervalPolicyError};
use orchestrator_core::load_gate::{LoadLimits, LoadMeter};
use orchestrator_core::supervision::{MonitoringMode, WakeLimits};
use sqlx::{Row, SqliteConnection};

use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

/// The settings row. Later phases add fields at the end; none are removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrchestratorSettings {
    pub load: LoadLimits,
    pub supervision: SupervisionSettings,
    pub wake_limits: WakeLimits,
    /// Read and written only through `monitoring.*`.
    pub mode: MonitoringMode,
    /// Phase 4b: chat may propose a task that ends in a pull request. Off by default.
    pub chat_pr_delivery: bool,
    /// Phase 6b: the read-only task tools for the orchestrator chat. Off by default.
    pub mcp: McpSettings,
    /// Migration 0055: the directory a task runs in when it is not clear which project it
    /// belongs to. The daemon validates the path before it is stored. `None` means none is set.
    pub home_directory: Option<String>,
    pub dispatch_defaults: DispatchDefaults,
    /// Migration 0066 (DPES-1108 phase 5): the orchestrator runs autonomous-class actions on its
    /// own. On by default; off makes every action a card for the user again.
    pub autonomous_actions: bool,
}

/// Defaults for new tasks only. Provider keys are canonical agent adapter ids.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchDefaults {
    pub default_agent: Option<String>,
    pub providers: BTreeMap<String, ModelSelection>,
}

impl DispatchDefaults {
    pub fn validate(&self) -> Result<(), StoreError> {
        let valid_agent = |agent: &str| {
            !agent.is_empty()
                && agent.len() <= 64
                && agent.bytes().all(|c| c.is_ascii_lowercase() || c == b'-')
        };
        if self
            .default_agent
            .as_deref()
            .is_some_and(|agent| !valid_agent(agent))
            || self.providers.len() > 16
            || self
                .providers
                .iter()
                .any(|(agent, selection)| !valid_agent(agent) || selection.validate().is_err())
        {
            return Err(invalid_input(
                "dispatch defaults contain an invalid agent or selection",
            ));
        }
        Ok(())
    }
}

/// The read-only MCP server setting (migration 0049). Only `settings.set` changes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct McpSettings {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupervisionSettings {
    pub base_interval_secs: u32,
    pub min_interval_secs: u32,
    /// 5 to 1440.
    pub attempt_wall_minutes: u32,
}

impl OrchestratorSettings {
    /// The check interval policy. A stored row always validates, because the store checks it
    /// on every read; the default policy is the fallback that never applies.
    pub fn interval_policy(&self) -> IntervalPolicy {
        IntervalPolicy::new(
            Duration::from_secs(u64::from(self.supervision.base_interval_secs)),
            Duration::from_secs(u64::from(self.supervision.min_interval_secs)),
        )
        .unwrap_or_default()
    }
}

/// The away window and the return brief window (phase 5b). Not a setting: `monitoring.set`
/// and `monitoring.brief_ack` write it through [`SqliteStore::set_monitoring_mode`] and
/// [`SqliteStore::ack_brief`], and `settings.set` never does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AwayWindows {
    /// Set while the mode is `away`.
    pub away_since_ms: Option<u64>,
    /// The closed window of the last away period, until the user acknowledges it.
    pub brief_from_ms: Option<u64>,
    pub brief_to_ms: Option<u64>,
}

/// `None` leaves a limit as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadPatch {
    pub cpu: Option<u8>,
    pub memory: Option<u8>,
    pub disk: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct McpPatch {
    pub enabled: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SupervisionPatch {
    pub base_interval_secs: Option<u32>,
    pub min_interval_secs: Option<u32>,
    pub attempt_wall_minutes: Option<u32>,
}

/// `None` leaves a field alone. For each limit, `Some(None)` sets NULL (no cap).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WakeLimitsPatch {
    pub hourly_cap: Option<Option<u32>>,
    pub daily_cap: Option<Option<u32>>,
    pub daily_token_budget: Option<Option<u64>>,
}

/// A change to the settings row. A field that is not named stays as it is. Later phases add
/// fields at the end; none are removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsPatch {
    pub load: LoadPatch,
    pub supervision: SupervisionPatch,
    pub wake_limits: WakeLimitsPatch,
    /// `settings.set` never sets it; `monitoring.set` sets only it.
    pub mode: Option<MonitoringMode>,
    pub chat_pr_delivery: Option<bool>,
    pub mcp: McpPatch,
    /// `Some(None)` clears the home directory.
    pub home_directory: Option<Option<String>>,
    /// Replaces this group; an absent group leaves it unchanged.
    pub dispatch_defaults: Option<DispatchDefaults>,
    pub autonomous_actions: Option<bool>,
}

impl SettingsPatch {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A value to bind to a named column.
enum Bound {
    Integer(i64),
    Null,
    Text(&'static str),
    Owned(String),
}

impl Bound {
    fn optional(value: Option<i64>) -> Self {
        value.map_or(Self::Null, Self::Integer)
    }
}

const SETTINGS_COLUMNS: &str = "load_cpu_percent, load_memory_percent, load_disk_percent, \
    monitoring_mode, base_interval_secs, min_interval_secs, wake_hourly_cap, wake_daily_cap, \
    wake_daily_token_budget, attempt_wall_minutes, chat_pr_delivery, mcp_server_enabled, \
    home_directory, dispatch_defaults, autonomous_actions";

impl SqliteStore {
    /// The settings row. A new store holds the defaults: load limits 80, 85 and 90, mode on,
    /// base interval 600 s, minimum 60 s, caps 6 and 40, no token budget, wall time 120
    /// minutes. A row whose intervals do not form a valid policy is reported as damaged.
    pub fn orchestrator_settings(&self) -> Result<OrchestratorSettings, StoreError> {
        self.call(|pool| async move {
            let mut connection = pool.acquire().await?;
            read_settings(&mut connection).await
        })
    }

    /// Changes the fields `patch` names, in one transaction: reads the row, merges the patch,
    /// checks the named groups (the load limits together, the intervals together through
    /// [`IntervalPolicy::new`], each cap and the wall time by range), then sets only the
    /// columns the patch names and bumps `updated_at`. Every other column keeps its value,
    /// the wake cursor included. A value out of range fails with an `InvalidInput` I/O error
    /// before anything is written. An empty patch writes nothing. Returns the row as stored.
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
            let current = read_settings(&mut transaction).await?;
            let mut named: Vec<(&'static str, Bound)> = Vec::new();

            if patch.load != LoadPatch::default() {
                LoadLimits::new(
                    patch.load.cpu.unwrap_or(current.load.cpu),
                    patch.load.memory.unwrap_or(current.load.memory),
                    patch.load.disk.unwrap_or(current.load.disk),
                )
                .map_err(|error| out_of_range(error.0))?;
                for (column, value) in [
                    ("load_cpu_percent", patch.load.cpu),
                    ("load_memory_percent", patch.load.memory),
                    ("load_disk_percent", patch.load.disk),
                ] {
                    if let Some(value) = value {
                        named.push((column, Bound::Integer(i64::from(value))));
                    }
                }
            }

            let supervision = patch.supervision;
            if supervision.base_interval_secs.is_some() || supervision.min_interval_secs.is_some() {
                let base = supervision
                    .base_interval_secs
                    .unwrap_or(current.supervision.base_interval_secs);
                let minimum = supervision
                    .min_interval_secs
                    .unwrap_or(current.supervision.min_interval_secs);
                IntervalPolicy::new(
                    Duration::from_secs(u64::from(base)),
                    Duration::from_secs(u64::from(minimum)),
                )
                .map_err(interval_refused)?;
            }
            for (column, value) in [
                ("base_interval_secs", supervision.base_interval_secs),
                ("min_interval_secs", supervision.min_interval_secs),
            ] {
                if let Some(value) = value {
                    named.push((column, Bound::Integer(i64::from(value))));
                }
            }
            if let Some(minutes) = supervision.attempt_wall_minutes {
                if !(5..=1440).contains(&minutes) {
                    return Err(invalid_input(
                        "the attempt wall time must be from 5 to 1440 minutes",
                    ));
                }
                named.push(("attempt_wall_minutes", Bound::Integer(i64::from(minutes))));
            }

            let limits = patch.wake_limits;
            if let Some(cap) = limits.hourly_cap {
                if matches!(cap, Some(value) if !(1..=1000).contains(&value)) {
                    return Err(invalid_input("the hourly wake cap must be from 1 to 1000"));
                }
                named.push(("wake_hourly_cap", Bound::optional(cap.map(i64::from))));
            }
            if let Some(cap) = limits.daily_cap {
                if matches!(cap, Some(value) if !(1..=10_000).contains(&value)) {
                    return Err(invalid_input("the daily wake cap must be from 1 to 10000"));
                }
                named.push(("wake_daily_cap", Bound::optional(cap.map(i64::from))));
            }
            if let Some(budget) = limits.daily_token_budget {
                let stored = match budget {
                    None => None,
                    Some(0) => {
                        return Err(invalid_input("the daily token budget must be above zero"))
                    }
                    Some(value) => Some(i64::try_from(value).map_err(|_| {
                        invalid_input("the daily token budget is above the storable range")
                    })?),
                };
                named.push(("wake_daily_token_budget", Bound::optional(stored)));
            }
            if let Some(mode) = patch.mode {
                named.push(("monitoring_mode", Bound::Text(mode.as_str())));
            }
            if let Some(value) = patch.chat_pr_delivery {
                named.push(("chat_pr_delivery", Bound::Integer(i64::from(value))));
            }
            if let Some(value) = patch.autonomous_actions {
                named.push(("autonomous_actions", Bound::Integer(i64::from(value))));
            }
            if let Some(value) = patch.mcp.enabled {
                named.push(("mcp_server_enabled", Bound::Integer(i64::from(value))));
            }
            if let Some(directory) = &patch.home_directory {
                let bound = match directory {
                    None => Bound::Null,
                    Some(path) if path.is_empty() || path.len() > 4096 || path.contains('\0') => {
                        return Err(invalid_input(
                            "the home directory must be a path of 1 to 4096 bytes",
                        ))
                    }
                    Some(path) => Bound::Owned(path.clone()),
                };
                named.push(("home_directory", bound));
            }
            if let Some(defaults) = &patch.dispatch_defaults {
                defaults.validate()?;
                let json = serde_json::to_string(defaults).map_err(|_| damaged())?;
                named.push(("dispatch_defaults", Bound::Owned(json)));
            }

            // Set only the columns the patch names. A write must never touch a column it was
            // not given, the wake cursor above all.
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
                update = match value {
                    Bound::Integer(value) => update.bind(*value),
                    Bound::Null => update.bind(None::<i64>),
                    Bound::Text(value) => update.bind(*value),
                    Bound::Owned(value) => update.bind(value.as_str()),
                };
            }
            update.execute(&mut *transaction).await?;
            let saved = read_settings(&mut transaction).await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// The away window columns (migration 0048).
    pub fn away_windows(&self) -> Result<AwayWindows, StoreError> {
        self.call(|pool| async move {
            let mut connection = pool.acquire().await?;
            read_away(&mut connection).await
        })
    }

    /// Sets the monitoring mode and keeps the away window in step, in one transaction. Entering
    /// `away` stores `away_since_ms = now_ms` when none is set. Leaving `away` closes the
    /// window into `brief_from_ms` and `brief_to_ms` (an earlier brief the user never
    /// acknowledged keeps its start) and clears `away_since_ms`. Away to away changes nothing.
    /// `now_ms` comes from the caller, so a test injects the clock. Returns the mode that was
    /// stored before. Only `monitoring.set` calls this; no other column is touched.
    pub fn set_monitoring_mode(
        &self,
        mode: MonitoringMode,
        now_ms: u64,
    ) -> Result<MonitoringMode, StoreError> {
        let now = i64::try_from(now_ms)
            .map_err(|_| invalid_input("the time is above the storable range"))?;
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let before_text: String = sqlx::query_scalar(
                "SELECT monitoring_mode FROM orchestrator_settings WHERE id = 1",
            )
            .fetch_one(&mut *transaction)
            .await?;
            let before = MonitoringMode::parse(&before_text).ok_or_else(damaged)?;
            let windows = read_away(&mut transaction).await?;
            let to_i64 = |value: u64| i64::try_from(value).map_err(|_| damaged());
            let mut away_since = windows.away_since_ms.map(to_i64).transpose()?;
            let mut brief_from = windows.brief_from_ms.map(to_i64).transpose()?;
            // `None` leaves the stored end as it is.
            let mut brief_to: Option<i64> = None;
            if mode == MonitoringMode::Away {
                if away_since.is_none() {
                    away_since = Some(now);
                }
            } else if let Some(since) = away_since.take() {
                let from = *brief_from.get_or_insert(since);
                brief_to = Some(now.max(from));
            }
            sqlx::query(
                "UPDATE orchestrator_settings SET monitoring_mode = ?, away_since_ms = ?, \
                 brief_from_ms = ?, brief_to_ms = COALESCE(?, brief_to_ms), \
                 updated_at = CURRENT_TIMESTAMP WHERE id = 1",
            )
            .bind(mode.as_str())
            .bind(away_since)
            .bind(brief_from)
            .bind(brief_to)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            Ok(before)
        })
    }

    /// Clears the closed brief window. The user has seen the brief. Idempotent; it changes no
    /// other column and the open away window stays.
    pub fn ack_brief(&self) -> Result<(), StoreError> {
        self.call(|pool| async move {
            sqlx::query(
                "UPDATE orchestrator_settings SET brief_from_ms = NULL, brief_to_ms = NULL, \
                 updated_at = CURRENT_TIMESTAMP WHERE id = 1 \
                 AND (brief_from_ms IS NOT NULL OR brief_to_ms IS NOT NULL)",
            )
            .execute(&pool)
            .await?;
            Ok(())
        })
    }
}

async fn read_away(connection: &mut SqliteConnection) -> Result<AwayWindows, StoreError> {
    let row = sqlx::query(
        "SELECT away_since_ms, brief_from_ms, brief_to_ms FROM orchestrator_settings WHERE id = 1",
    )
    .fetch_one(connection)
    .await?;
    let read = |column: &str| -> Result<Option<u64>, StoreError> {
        row.try_get::<Option<i64>, _>(column)?
            .map(|value| u64::try_from(value).map_err(|_| damaged()))
            .transpose()
    };
    Ok(AwayWindows {
        away_since_ms: read("away_since_ms")?,
        brief_from_ms: read("brief_from_ms")?,
        brief_to_ms: read("brief_to_ms")?,
    })
}

async fn read_settings(
    connection: &mut SqliteConnection,
) -> Result<OrchestratorSettings, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe to assert: a constant list of column names.
        "SELECT {SETTINGS_COLUMNS} FROM orchestrator_settings WHERE id = 1"
    )))
    .fetch_one(connection)
    .await?;
    // The table's CHECKs keep these in range, so a row that is not is a damaged database.
    let load_value = |column: &str| -> Result<u8, StoreError> {
        u8::try_from(row.try_get::<i64, _>(column)?).map_err(|_| damaged())
    };
    let small = |column: &str| -> Result<u32, StoreError> {
        u32::try_from(row.try_get::<i64, _>(column)?).map_err(|_| damaged())
    };
    let optional = |column: &str| -> Result<Option<i64>, StoreError> {
        Ok(row.try_get::<Option<i64>, _>(column)?)
    };
    let load = LoadLimits::new(
        load_value("load_cpu_percent")?,
        load_value("load_memory_percent")?,
        load_value("load_disk_percent")?,
    )
    .map_err(|_| damaged())?;
    let supervision = SupervisionSettings {
        base_interval_secs: small("base_interval_secs")?,
        min_interval_secs: small("min_interval_secs")?,
        attempt_wall_minutes: small("attempt_wall_minutes")?,
    };
    IntervalPolicy::new(
        Duration::from_secs(u64::from(supervision.base_interval_secs)),
        Duration::from_secs(u64::from(supervision.min_interval_secs)),
    )
    .map_err(|_| damaged())?;
    let cap = |column: &str| -> Result<Option<u32>, StoreError> {
        optional(column)?
            .map(|value| u32::try_from(value).map_err(|_| damaged()))
            .transpose()
    };
    let wake_limits = WakeLimits {
        hourly: cap("wake_hourly_cap")?,
        daily: cap("wake_daily_cap")?,
        daily_tokens: optional("wake_daily_token_budget")?
            .map(|value| u64::try_from(value).map_err(|_| damaged()))
            .transpose()?,
    };
    let mode_text: String = row.try_get("monitoring_mode")?;
    let mode = MonitoringMode::parse(&mode_text).ok_or_else(damaged)?;
    let dispatch_defaults: DispatchDefaults =
        serde_json::from_str(row.try_get("dispatch_defaults")?).map_err(|_| damaged())?;
    dispatch_defaults.validate().map_err(|_| damaged())?;
    Ok(OrchestratorSettings {
        load,
        supervision,
        wake_limits,
        mode,
        chat_pr_delivery: row.try_get::<i64, _>("chat_pr_delivery")? != 0,
        mcp: McpSettings {
            enabled: row.try_get::<i64, _>("mcp_server_enabled")? != 0,
        },
        home_directory: row.try_get("home_directory")?,
        dispatch_defaults,
        autonomous_actions: row.try_get::<i64, _>("autonomous_actions")? != 0,
    })
}

fn damaged() -> StoreError {
    corrupt("the stored orchestrator settings are not valid".to_owned())
}

fn invalid_input(message: &'static str) -> StoreError {
    StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, message))
}

fn out_of_range(meter: LoadMeter) -> StoreError {
    StoreError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("the {} load limit must be from 50 to 100", meter.label()),
    ))
}

fn interval_refused(error: IntervalPolicyError) -> StoreError {
    invalid_input(match error {
        IntervalPolicyError::BaseOutOfRange => {
            "the base check interval must be from 1 to 240 minutes"
        }
        IntervalPolicyError::MinimumBelowFloor => {
            "the minimum check interval must be at least 10 seconds"
        }
        IntervalPolicyError::MinimumAboveBase => {
            "the minimum check interval must not be above the base interval"
        }
    })
}
