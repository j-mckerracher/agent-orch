//! The counts behind the return brief (DPES-1105 phase 5b).
//!
//! Read-only. Every figure is a count or a short field the daemon wrote itself: ids, states,
//! a proposal kind, a task objective cut to one line of 80 bytes, a project id. No report body,
//! task output, failure reason or objective text beyond that line is read. The project id is a
//! raw id here; the daemon turns it into a code and redacts every title before anything leaves
//! the process. A window is `[from_ms, to_ms]`; the figures marked "now" ignore it.

use orchestrator_core::return_brief::{Brief, BriefCounts, BriefGrant, BriefItem};
use orchestrator_core::task::TaskState;
use sqlx::SqliteConnection;

use crate::sqlite_supervision::one_line;
use crate::{SqliteStore, StoreError};

/// One list row: a task or proposal id, a title, a state word, a project id and a time.
type Row = (String, String, i64, Option<String>, String);

impl SqliteStore {
    /// The brief for `[from_ms, to_ms]`, lists capped and titles cut. `open` is true while the
    /// user is still away. `now_ms` decides each listed grant's status. Titles are raw one-line
    /// objectives and project fields are raw ids: the caller redacts and codes them.
    pub fn return_brief(
        &self,
        from_ms: u64,
        to_ms: u64,
        open: bool,
        now_ms: u64,
    ) -> Result<Brief, StoreError> {
        let from = i64::try_from(from_ms).unwrap_or(i64::MAX);
        let to = i64::try_from(to_ms).unwrap_or(i64::MAX);
        let running = [
            serde_json::to_string(&TaskState::Running)?,
            serde_json::to_string(&TaskState::Verifying)?,
        ];
        let (counts, waiting, attention, grant_rows) = self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            let connection = &mut *connection;
            let mut counts = BriefCounts {
                tasks_succeeded: transitions(connection, &["Succeeded"], from, to).await?,
                tasks_failed: transitions(connection, &["Failed", "VerificationFailed"], from, to)
                    .await?,
                tasks_stalled: windowed(
                    connection,
                    "SELECT COUNT(DISTINCT subject) FROM wake_records \
                     WHERE event_kind = 'task_stalled' AND created_at BETWEEN \
                     datetime(? / 1000, 'unixepoch') AND datetime(? / 1000, 'unixepoch')",
                    from,
                    to,
                )
                .await?,
                proposals_approved_by_user: windowed(
                    connection,
                    "SELECT COUNT(*) FROM action_proposal_events \
                     WHERE to_state = 'approved' AND actor = 'user' AND created_at BETWEEN \
                     datetime(? / 1000, 'unixepoch') AND datetime(? / 1000, 'unixepoch')",
                    from,
                    to,
                )
                .await?,
                proposals_approved_by_grant: by_grant(connection, "approved", from, to).await?,
                executed_by_grant: by_grant(connection, "executed", from, to).await?,
                failed_under_grant: by_grant(connection, "failed", from, to).await?,
                grant_uses: windowed(
                    connection,
                    "SELECT COUNT(*) FROM standing_grant_events WHERE event = 'used' \
                     AND at_ms >= ? AND at_ms <= ?",
                    from,
                    to,
                )
                .await?,
                grants_expired: windowed(
                    connection,
                    "SELECT COUNT(*) FROM standing_grants WHERE expires_at_ms >= ? \
                     AND expires_at_ms <= ? AND (revoked_at_ms IS NULL \
                     OR revoked_at_ms >= expires_at_ms)",
                    from,
                    to,
                )
                .await?,
                grants_revoked: windowed(
                    connection,
                    "SELECT COUNT(*) FROM standing_grants WHERE revoked_at_ms >= ? \
                     AND revoked_at_ms <= ?",
                    from,
                    to,
                )
                .await?,
                quiet_updates: windowed(
                    connection,
                    "SELECT COUNT(*) FROM wake_records WHERE state = 'handled' \
                     AND outcome = 'quiet' AND created_at BETWEEN \
                     datetime(? / 1000, 'unixepoch') AND datetime(? / 1000, 'unixepoch')",
                    from,
                    to,
                )
                .await?,
                replies_paused_by_limit: windowed(
                    connection,
                    "SELECT COUNT(*) FROM wake_records WHERE state = 'skipped' \
                     AND skip_reason IN ('limit_hourly', 'limit_daily', 'limit_tokens') \
                     AND created_at BETWEEN datetime(? / 1000, 'unixepoch') \
                     AND datetime(? / 1000, 'unixepoch')",
                    from,
                    to,
                )
                .await?,
                ..BriefCounts::default()
            };
            counts.tasks_running = now_count(
                connection,
                "SELECT COUNT(*) FROM tasks WHERE state IN (?, ?)",
                &running,
            )
            .await?;
            counts.proposals_waiting = now_count(
                connection,
                "SELECT COUNT(*) FROM action_proposals WHERE state = 'pending'",
                &[],
            )
            .await?;
            counts.deliveries_waiting = now_count(
                connection,
                "SELECT COUNT(*) FROM approvals WHERE status = 'requested'",
                &[],
            )
            .await?;
            counts.decisions_held = now_count(
                connection,
                "SELECT COUNT(*) FROM decision_holds WHERE state = 'active'",
                &[],
            )
            .await?;

            let mut waiting = Vec::new();
            let proposals: Vec<(String, String, i64)> = sqlx::query_as(
                "SELECT id, kind, CAST(strftime('%s', created_at) AS INTEGER) * 1000 \
                 FROM action_proposals WHERE state = 'pending' \
                 ORDER BY created_at, id LIMIT 10",
            )
            .fetch_all(&mut *connection)
            .await?;
            for (id, kind, at) in proposals {
                // The title is the kind label, never a parameter.
                waiting.push(item("proposal_waiting", id, kind, "pending", None, at));
            }
            let deliveries: Vec<Row> = sqlx::query_as(
                "SELECT a.task_id, COALESCE(json_extract(t.contract_json, '$.objective'), ''), \
                 COALESCE((SELECT CAST(strftime('%s', MIN(e.created_at)) AS INTEGER) * 1000 \
                           FROM approval_events e WHERE e.approval_id = a.id), 0), \
                 t.project_id, 'requested' \
                 FROM approvals a JOIN tasks t ON t.id = a.task_id \
                 WHERE a.status = 'requested' ORDER BY a.id LIMIT 10",
            )
            .fetch_all(&mut *connection)
            .await?;
            for (id, objective, at, project, state) in deliveries {
                waiting.push(item(
                    "delivery_waiting",
                    id,
                    one_line(&objective),
                    &state,
                    project,
                    at,
                ));
            }
            waiting.sort_by_key(|entry: &BriefItem| entry.at_ms);

            let mut attention = Vec::new();
            let failed: Vec<Row> = sqlx::query_as(
                "SELECT t.id, COALESCE(json_extract(t.contract_json, '$.objective'), ''), \
                 MAX(x.ms), t.project_id, x.st \
                 FROM (SELECT e.task_id AS task_id, json_extract(e.payload, '$.state') AS st, \
                              CAST(strftime('%s', e.created_at) AS INTEGER) * 1000 AS ms \
                       FROM durable_events e WHERE e.kind = 'task.transition' \
                       AND e.created_at BETWEEN datetime(? / 1000, 'unixepoch') \
                       AND datetime(? / 1000, 'unixepoch')) x \
                 JOIN tasks t ON t.id = x.task_id \
                 WHERE x.st IN ('Failed', 'VerificationFailed') \
                 GROUP BY t.id ORDER BY MAX(x.ms) DESC, t.id LIMIT 10",
            )
            .bind(from)
            .bind(to)
            .fetch_all(&mut *connection)
            .await?;
            for (id, objective, at, project, state) in failed {
                attention.push(item(
                    "task_failed",
                    id,
                    one_line(&objective),
                    &state,
                    project,
                    at,
                ));
            }
            let noticed: Vec<(String, String, String, i64, Option<String>)> = sqlx::query_as(
                "SELECT w.event_kind, w.subject, \
                 COALESCE(json_extract(t.contract_json, '$.objective'), ''), \
                 MAX(CAST(strftime('%s', w.created_at) AS INTEGER) * 1000), t.project_id \
                 FROM wake_records w LEFT JOIN tasks t ON t.id = w.subject \
                 WHERE w.event_kind IN ('task_stalled', 'sign_in_needed') \
                 AND w.created_at BETWEEN datetime(? / 1000, 'unixepoch') \
                 AND datetime(? / 1000, 'unixepoch') \
                 GROUP BY w.event_kind, w.subject \
                 ORDER BY 4 DESC, w.subject LIMIT 10",
            )
            .bind(from)
            .bind(to)
            .fetch_all(&mut *connection)
            .await?;
            for (kind, id, objective, at, project) in noticed {
                let state = if kind == "task_stalled" {
                    "stalled"
                } else {
                    "sign_in_needed"
                };
                attention.push(item(&kind, id, one_line(&objective), state, project, at));
            }
            attention.sort_by_key(|entry: &BriefItem| std::cmp::Reverse(entry.at_ms));

            let grant_rows: Vec<(String, i64)> = sqlx::query_as(
                "SELECT id, uses FROM (SELECT g.id AS id, g.created_at_ms AS created, \
                        g.expires_at_ms AS expires, g.revoked_at_ms AS revoked, \
                        (SELECT COUNT(*) FROM standing_grant_events e WHERE e.grant_id = g.id \
                         AND e.event = 'used' AND e.at_ms >= ? AND e.at_ms <= ?) AS uses \
                       FROM standing_grants g) \
                 WHERE uses > 0 \
                 OR (expires >= ? AND expires <= ? AND (revoked IS NULL OR revoked >= expires)) \
                 OR (revoked >= ? AND revoked <= ?) \
                 ORDER BY created DESC, id LIMIT 10",
            )
            .bind(from)
            .bind(to)
            .bind(from)
            .bind(to)
            .bind(from)
            .bind(to)
            .fetch_all(&mut *connection)
            .await?;
            Ok((counts, waiting, attention, grant_rows))
        })?;

        let mut grants = Vec::new();
        for (id, uses) in grant_rows {
            let Some(grant) = self.standing_grant(&id)? else {
                continue;
            };
            let spent = self.grant_tokens_spent(&id)?;
            grants.push(BriefGrant {
                id,
                scope_kind: grant.scope.kind_str().to_owned(),
                // The daemon swaps a workstream id for its redacted title.
                scope_label: grant.scope.id().to_owned(),
                uses_in_window: u32::try_from(uses).unwrap_or(u32::MAX),
                uses_used: grant.uses_used,
                max_uses: grant.max_uses,
                status: grant.status(now_ms, spent).as_str().to_owned(),
            });
        }
        Ok(Brief {
            from_ms,
            to_ms,
            open,
            counts,
            grants,
            waiting,
            attention,
        }
        .capped())
    }
}

fn item(
    kind: &str,
    id: String,
    title: String,
    state: &str,
    project: Option<String>,
    at: i64,
) -> BriefItem {
    BriefItem {
        kind: kind.to_owned(),
        id,
        title,
        state: state.to_owned(),
        project,
        at_ms: u64::try_from(at).unwrap_or(0),
    }
}

fn small(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A count whose statement binds the window twice, `from` then `to`.
async fn windowed(
    connection: &mut SqliteConnection,
    sql: &'static str,
    from: i64,
    to: i64,
) -> Result<u32, StoreError> {
    let count: i64 = sqlx::query_scalar(sql)
        .bind(from)
        .bind(to)
        .fetch_one(connection)
        .await?;
    Ok(small(count))
}

/// A count of what is true now: the statement binds only `binds`.
async fn now_count(
    connection: &mut SqliteConnection,
    sql: &'static str,
    binds: &[String],
) -> Result<u32, StoreError> {
    let mut query = sqlx::query_scalar::<_, i64>(sql);
    for value in binds {
        query = query.bind(value);
    }
    Ok(small(query.fetch_one(connection).await?))
}

/// Tasks that moved to one of `states` in the window, each task once.
async fn transitions(
    connection: &mut SqliteConnection,
    states: &[&str],
    from: i64,
    to: i64,
) -> Result<u32, StoreError> {
    // Every statement is a literal; the one or two state words are bound.
    let sql: &'static str = if states.len() == 1 {
        "SELECT COUNT(DISTINCT task_id) FROM durable_events WHERE kind = 'task.transition' \
         AND json_extract(payload, '$.state') = ? AND created_at BETWEEN \
         datetime(? / 1000, 'unixepoch') AND datetime(? / 1000, 'unixepoch')"
    } else {
        "SELECT COUNT(DISTINCT task_id) FROM durable_events WHERE kind = 'task.transition' \
         AND json_extract(payload, '$.state') IN (?, ?) AND created_at BETWEEN \
         datetime(? / 1000, 'unixepoch') AND datetime(? / 1000, 'unixepoch')"
    };
    let mut query = sqlx::query_scalar::<_, i64>(sql);
    for state in states {
        query = query.bind(*state);
    }
    Ok(small(
        query.bind(from).bind(to).fetch_one(connection).await?,
    ))
}

/// Proposal events in the window that moved a proposal to `to_state` under a grant.
async fn by_grant(
    connection: &mut SqliteConnection,
    to_state: &'static str,
    from: i64,
    to: i64,
) -> Result<u32, StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM action_proposal_events WHERE to_state = ? \
         AND actor GLOB 'grant:?*' AND created_at BETWEEN \
         datetime(? / 1000, 'unixepoch') AND datetime(? / 1000, 'unixepoch')",
    )
    .bind(to_state)
    .bind(from)
    .bind(to)
    .fetch_one(connection)
    .await?;
    Ok(small(count))
}
