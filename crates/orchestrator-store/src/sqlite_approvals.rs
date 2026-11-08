//! SQLite persistence for the approval state machine.
//!
//! Approval state changes and their append-only events commit together. The
//! task `events` table remains task-only because `Store::get` decodes each
//! payload as a task audit event.
//!
//! Updated: 2026-11-08T00:04:58-07:00.

use std::fmt;
use std::future::Future;

use orchestrator_core::approval::{
    self, Actor, Approval, ApprovalAction, ApprovalActionKind, ApprovalError, ApprovalEvent,
    ApprovalEventKind, ApprovalState, ApproverAllowlist,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::sqlite::{Sqlite, SqlitePool, SqliteRow};
use sqlx::{Row, Transaction};

use crate::SqliteStore;

impl SqliteStore {
    /// Create a pending approval and supersede changed active actions for its task.
    ///
    /// Agents may request delivery approvals. Policy changes require a user or
    /// allowlisted actor. Replacing an active action and recording its event
    /// happen in the same transaction as creating the new request.
    pub fn request_approval(
        &self,
        id: &str,
        action: &ApprovalAction,
        requested_by: &Actor,
        allowlist: &ApproverAllowlist,
    ) -> Result<Approval, ApprovalError> {
        validate_approval_id(id)?;
        validate_actor(requested_by)?;
        if action.kind() == ApprovalActionKind::PolicyChange && !allowlist.allows(requested_by) {
            return Err(ApprovalError::UnauthorizedPolicyChange {
                actor_id: requested_by.id().to_owned(),
            });
        }

        let id = id.to_owned();
        let action = action.clone();
        let requested_by = requested_by.clone();
        approval_call(self, move |pool| async move {
            let mut transaction = pool.begin().await.map_err(persistence)?;
            let active = sqlx::query(
                "SELECT id, task_id, action, status, granted_by FROM approvals \
                 WHERE task_id = ? AND status IN ('requested', 'approved') ORDER BY id",
            )
            .bind(action.task_id())
            .fetch_all(&mut *transaction)
            .await
            .map_err(persistence)?;

            for row in active {
                let existing = decode_approval(row)?;
                if existing.action != action {
                    let event = ApprovalEvent::Superseded {
                        by: requested_by.clone(),
                    };
                    let next = approval::transition(Some(&existing.state), &event)?;
                    update_approval(&mut transaction, &existing, &event, &next).await?;
                }
            }

            let event = ApprovalEvent::Requested { by: requested_by };
            let state = approval::transition(None, &event)?;
            sqlx::query(
                "INSERT INTO approvals (id, task_id, action, status, granted_by) \
                 VALUES (?, ?, ?, ?, NULL)",
            )
            .bind(&id)
            .bind(action.task_id())
            .bind(encode(&action)?)
            .bind(state_status(&state))
            .execute(&mut *transaction)
            .await
            .map_err(persistence)?;
            insert_event(&mut transaction, &id, action.task_id(), &event).await?;
            transaction.commit().await.map_err(persistence)?;

            Ok(Approval { id, action, state })
        })
    }

    /// Approve a pending action as an allowlisted non-agent identity.
    pub fn approve_approval(
        &self,
        id: &str,
        actor: &Actor,
        allowlist: &ApproverAllowlist,
    ) -> Result<Approval, ApprovalError> {
        validate_approval_id(id)?;
        validate_actor(actor)?;
        authorize_decision(actor, allowlist)?;
        let id = id.to_owned();
        let actor = actor.clone();
        approval_call(self, move |pool| async move {
            let mut transaction = pool.begin().await.map_err(persistence)?;
            let current = load_approval(&mut transaction, &id).await?;
            let event = ApprovalEvent::Approved { by: actor };
            let next = approval::transition(Some(&current.state), &event)?;
            let updated = update_approval(&mut transaction, &current, &event, &next).await?;
            transaction.commit().await.map_err(persistence)?;
            Ok(updated)
        })
    }

    /// Reject a pending action as an allowlisted non-agent identity.
    pub fn reject_approval(
        &self,
        id: &str,
        actor: &Actor,
        allowlist: &ApproverAllowlist,
    ) -> Result<Approval, ApprovalError> {
        validate_approval_id(id)?;
        validate_actor(actor)?;
        authorize_decision(actor, allowlist)?;
        let id = id.to_owned();
        let actor = actor.clone();
        approval_call(self, move |pool| async move {
            let mut transaction = pool.begin().await.map_err(persistence)?;
            let current = load_approval(&mut transaction, &id).await?;
            let event = ApprovalEvent::Rejected { by: actor };
            let next = approval::transition(Some(&current.state), &event)?;
            let updated = update_approval(&mut transaction, &current, &event, &next).await?;
            transaction.commit().await.map_err(persistence)?;
            Ok(updated)
        })
    }

    /// Load one persisted approval.
    pub fn get_approval(&self, id: &str) -> Result<Approval, ApprovalError> {
        validate_approval_id(id)?;
        let id = id.to_owned();
        approval_call(self, move |pool| async move {
            let row = sqlx::query(
                "SELECT id, task_id, action, status, granted_by FROM approvals WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(&pool)
            .await
            .map_err(persistence)?
            .ok_or_else(|| ApprovalError::NotFound {
                approval_id: id.clone(),
            })?;
            decode_approval(row)
        })
    }

    /// Check approval state and all bound action dimensions against a proposal.
    pub fn check_approval(&self, id: &str, proposed: &ApprovalAction) -> Result<(), ApprovalError> {
        let approval = self.get_approval(id)?;
        approval::check(&approval, proposed)
    }

    /// Load approval events in database sequence order.
    pub fn approval_events(&self, id: &str) -> Result<Vec<ApprovalEvent>, ApprovalError> {
        validate_approval_id(id)?;
        let id = id.to_owned();
        approval_call(self, move |pool| async move {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS (SELECT 1 FROM approvals WHERE id = ?)",
            )
            .bind(&id)
            .fetch_one(&pool)
            .await
            .map_err(persistence)?;
            if exists == 0 {
                return Err(ApprovalError::NotFound { approval_id: id });
            }

            let rows = sqlx::query(
                "SELECT kind, payload FROM approval_events WHERE approval_id = ? ORDER BY seq",
            )
            .bind(&id)
            .fetch_all(&pool)
            .await
            .map_err(persistence)?;
            rows.into_iter()
                .map(|row| {
                    let kind: String = row.try_get("kind").map_err(persistence)?;
                    let payload: String = row.try_get("payload").map_err(persistence)?;
                    let event: ApprovalEvent = decode(&payload)?;
                    if kind != event.kind() {
                        return Err(persistence(format!(
                            "approval event kind {kind:?} does not match payload"
                        )));
                    }
                    Ok(event)
                })
                .collect()
        })
    }
}

fn approval_call<T, F, Fut>(store: &SqliteStore, operation: F) -> Result<T, ApprovalError>
where
    T: Send + 'static,
    F: FnOnce(SqlitePool) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, ApprovalError>> + Send + 'static,
{
    store
        .call(move |pool| async move { Ok(operation(pool).await) })
        .map_err(persistence)?
}

fn validate_approval_id(id: &str) -> Result<(), ApprovalError> {
    if id.is_empty() {
        Err(ApprovalError::InvalidApprovalId)
    } else {
        Ok(())
    }
}

fn validate_actor(actor: &Actor) -> Result<(), ApprovalError> {
    if actor.id().is_empty() {
        Err(ApprovalError::InvalidActor)
    } else {
        Ok(())
    }
}

fn authorize_decision(actor: &Actor, allowlist: &ApproverAllowlist) -> Result<(), ApprovalError> {
    if !allowlist.allows(actor) {
        return Err(ApprovalError::UnauthorizedActor {
            actor_id: actor.id().to_owned(),
        });
    }
    Ok(())
}

async fn load_approval(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> Result<Approval, ApprovalError> {
    let row =
        sqlx::query("SELECT id, task_id, action, status, granted_by FROM approvals WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(persistence)?
            .ok_or_else(|| ApprovalError::NotFound {
                approval_id: id.to_owned(),
            })?;
    decode_approval(row)
}

fn decode_approval(row: SqliteRow) -> Result<Approval, ApprovalError> {
    let id: String = row.try_get("id").map_err(persistence)?;
    let task_id: String = row.try_get("task_id").map_err(persistence)?;
    let action_json: String = row.try_get("action").map_err(persistence)?;
    let status: String = row.try_get("status").map_err(persistence)?;
    let granted_by: Option<String> = row.try_get("granted_by").map_err(persistence)?;
    let action: ApprovalAction = decode(&action_json)?;
    if action.task_id() != task_id {
        return Err(persistence(format!(
            "approval {id:?} action task id does not match row"
        )));
    }
    let actor = granted_by
        .map(|value| decode::<Actor>(&value))
        .transpose()?;
    let state = match (status.as_str(), actor) {
        ("requested", None) => ApprovalState::Requested,
        ("approved", Some(actor)) if !matches!(actor, Actor::Agent(_)) => {
            ApprovalState::Approved { by: actor }
        }
        ("rejected", None) => ApprovalState::Rejected,
        ("superseded", Some(actor)) if !matches!(actor, Actor::Agent(_)) => {
            ApprovalState::Superseded {
                approved_by: Some(actor),
            }
        }
        ("superseded", None) => ApprovalState::Superseded { approved_by: None },
        _ => {
            return Err(persistence(format!(
                "approval {id:?} has invalid state or actor"
            )))
        }
    };
    Ok(Approval { id, action, state })
}

async fn update_approval(
    transaction: &mut Transaction<'_, Sqlite>,
    current: &Approval,
    event: &ApprovalEvent,
    next: &ApprovalState,
) -> Result<Approval, ApprovalError> {
    let updated =
        sqlx::query("UPDATE approvals SET status = ?, granted_by = ? WHERE id = ? AND status = ?")
            .bind(state_status(next))
            .bind(stored_actor(next)?)
            .bind(&current.id)
            .bind(state_status(&current.state))
            .execute(&mut **transaction)
            .await
            .map_err(persistence)?;
    if updated.rows_affected() != 1 {
        return Err(ApprovalError::IllegalTransition {
            from: Some(current.state.kind()),
            event: event_kind(event),
        });
    }
    insert_event(transaction, &current.id, current.action.task_id(), event).await?;
    Ok(Approval {
        id: current.id.clone(),
        action: current.action.clone(),
        state: next.clone(),
    })
}

async fn insert_event(
    transaction: &mut Transaction<'_, Sqlite>,
    approval_id: &str,
    task_id: &str,
    event: &ApprovalEvent,
) -> Result<(), ApprovalError> {
    sqlx::query(
        "INSERT INTO approval_events (approval_id, task_id, kind, payload) VALUES (?, ?, ?, ?)",
    )
    .bind(approval_id)
    .bind(task_id)
    .bind(event.kind())
    .bind(encode(event)?)
    .execute(&mut **transaction)
    .await
    .map_err(persistence)?;
    Ok(())
}

fn state_status(state: &ApprovalState) -> &'static str {
    match state {
        ApprovalState::Requested => "requested",
        ApprovalState::Approved { .. } => "approved",
        ApprovalState::Rejected => "rejected",
        ApprovalState::Superseded { .. } => "superseded",
    }
}

fn stored_actor(state: &ApprovalState) -> Result<Option<String>, ApprovalError> {
    match state {
        ApprovalState::Approved { by }
        | ApprovalState::Superseded {
            approved_by: Some(by),
        } => encode(by).map(Some),
        ApprovalState::Requested
        | ApprovalState::Rejected
        | ApprovalState::Superseded { approved_by: None } => Ok(None),
    }
}

fn event_kind(event: &ApprovalEvent) -> ApprovalEventKind {
    match event {
        ApprovalEvent::Requested { .. } => ApprovalEventKind::Requested,
        ApprovalEvent::Approved { .. } => ApprovalEventKind::Approved,
        ApprovalEvent::Rejected { .. } => ApprovalEventKind::Rejected,
        ApprovalEvent::Superseded { .. } => ApprovalEventKind::Superseded,
    }
}

fn encode<T: Serialize>(value: &T) -> Result<String, ApprovalError> {
    serde_json::to_string(value).map_err(persistence)
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, ApprovalError> {
    serde_json::from_str(value).map_err(persistence)
}

fn persistence(error: impl fmt::Display) -> ApprovalError {
    ApprovalError::Persistence(error.to_string())
}
