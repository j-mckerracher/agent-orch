//! Projection of task journal rows into the released `mcao.audit-envelope.v1` activity shape.
//!
//! Authority: motif-audit-event-contracts v1.0.0, `schemas/v1/event.schema.json` (`audit.activity`)
//! and `spec/siem-event-v1.md`, section "MCAO activity projection". The journal records no
//! writer-owned, identity, role, or sub-second timestamp fields. Those enter only through
//! [`TrustedContext`] and [`WriterFields`]. This module mints no IDs, hashes, timestamps, or
//! identities, and it refuses rather than guesses.

use std::str::FromStr;

use jiff::{tz::TimeZone, Timestamp};
use orchestrator_core::contract::TaskContract;
use serde::Serialize;
use thiserror::Error;

use crate::TaskDurableEvent;

/// Schema identifier carried by every projected event.
pub const SCHEMA: &str = "mcao.siem.event/v1";
/// Released `source_route` for `audit.activity`. It names this envelope version.
pub const SOURCE_ROUTE: &str = "mcao.audit-envelope.v1";

const EVENT_TYPE: &str = "audit.activity";
const PLATFORM: &str = "mcao";

/// Agent roles the released activity contract accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Firstmate,
    Secondmate,
    Ship,
    Scout,
}

/// How the user identity was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentitySource {
    GithubLogin,
    GitEmail,
    LocalAccount,
}

/// Actor kinds the released activity contract accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorType {
    User,
    Agent,
    System,
}

/// Already-resolved user identity. Never raw identity text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIdentity {
    /// `hmac-sha256:<64 lowercase hex>` pseudonym supplied by the identity owner.
    pub user_id: String,
    pub source: IdentitySource,
}

/// Explicit trusted materialization context. The journal records none of these values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedContext {
    /// Stable home identity from home-private config. UUIDv4.
    pub home_id: String,
    /// `None` means `identity_unresolved`: refuse to emit and alert.
    pub identity: Option<ResolvedIdentity>,
    /// IANA timezone of the runtime user. `day_key` is derived from it.
    pub timezone: String,
    /// Role of the agent that ran the session. Not recorded by the journal.
    pub role: AgentRole,
}

/// Writer-owned fields. The journal has no source for any of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterFields {
    /// Source UUIDv7 event ID.
    pub event_id: String,
    /// Writer-assigned sequence per home, nonnegative. Unrelated to the store's global
    /// `TaskDurableEvent::sequence` cursor; never derive it from that cursor.
    pub seq: u64,
    /// `sha256:<64 lowercase hex>`; `None` exactly at sequence zero.
    pub prev_hash: Option<String>,
    /// Observed envelope instant, supplied by the writer: RFC 3339 UTC, exactly three fractional
    /// digits, `Z`. Never projection or read time, and never the journal `created_at`, which is
    /// whole-second write time. `occurred_at_utc` and `day_key` both derive from this value.
    pub ts: String,
    pub actor_type: ActorType,
    pub actor_id: String,
}

/// Why a journal row cannot be projected. Each variant is a refusal, never a partial event.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectionError {
    /// `audit.activity` requires a non-empty session; this row has none.
    #[error("{kind} has no session; audit.activity requires one")]
    SessionRequired { kind: String },
    /// A session-scoped row kind with no released category mapping.
    #[error("no category mapping for {0}")]
    UnmappedKind(String),
    /// No resolved identity. The caller must alert and must not emit.
    #[error("identity_unresolved")]
    IdentityUnresolved,
    #[error("home_id is not a UUIDv4")]
    InvalidHomeId,
    #[error("user id is not an hmac-sha256 pseudonym")]
    InvalidUserId,
    #[error("unknown IANA timezone {0}")]
    InvalidTimezone(String),
    #[error("event_id is not a UUIDv7")]
    InvalidEventId,
    #[error("prev_hash is inconsistent with seq {0}")]
    InvalidPrevHash(u64),
    #[error("ts is not RFC 3339 UTC with milliseconds")]
    InvalidTimestamp,
    #[error("actor is empty or, for a user actor, is not the resolved user")]
    InvalidActor,
    #[error("agent harness or model is empty")]
    InvalidAgent,
    #[error("contract {contract} does not own journal task {task}")]
    TaskMismatch { contract: String, task: String },
}

/// Released `audit.activity` event. Field order follows the schema's required list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditActivity {
    pub schema: &'static str,
    pub event_type: &'static str,
    pub event_id: String,
    pub occurred_at_utc: String,
    pub source_route: &'static str,
    pub data: ActivityData,
    pub lineage: Lineage,
}

/// Closed `data` object of an activity event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityData {
    pub event: String,
    pub category: &'static str,
    pub outcome: &'static str,
    pub platform: &'static str,
    pub home_id: String,
    pub day_key: String,
    pub session_id: String,
    pub task_id: Option<String>,
    pub agent: Agent,
    pub actor: Actor,
    pub repo: Option<String>,
    pub pr: Option<String>,
    pub commit: Option<String>,
    pub user: User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Agent {
    pub role: AgentRole,
    pub harness: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Actor {
    #[serde(rename = "type")]
    pub kind: ActorType,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: String,
    pub identity_source: IdentitySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lineage {
    pub event_id: String,
    pub seq: u64,
    pub prev_hash: Option<String>,
}

/// Projects one session-scoped journal row into a released `audit.activity` event.
///
/// Fails closed: rows without a session, unmapped kinds, unresolved identity, and malformed
/// trusted or writer inputs all return an error instead of a partial event.
pub fn project_session_activity(
    row: &TaskDurableEvent,
    contract: &TaskContract,
    context: &TrustedContext,
    writer: &WriterFields,
) -> Result<AuditActivity, ProjectionError> {
    if contract.id != row.task_id {
        return Err(ProjectionError::TaskMismatch {
            contract: contract.id.clone(),
            task: row.task_id.clone(),
        });
    }
    let session_id = match row.session_id.as_deref() {
        Some(session_id) if !session_id.is_empty() => session_id.to_owned(),
        _ => {
            return Err(ProjectionError::SessionRequired {
                kind: row.kind.clone(),
            });
        }
    };
    let category = category_for(&row.kind)?;
    let identity = context
        .identity
        .as_ref()
        .ok_or(ProjectionError::IdentityUnresolved)?;
    if !is_uuid(&context.home_id, b'4') {
        return Err(ProjectionError::InvalidHomeId);
    }
    if !is_user_id(&identity.user_id) {
        return Err(ProjectionError::InvalidUserId);
    }
    let day = derive_day_key(&writer.ts, &context.timezone)?;
    check_writer(writer, identity)?;
    if contract.agent.is_empty() {
        return Err(ProjectionError::InvalidAgent);
    }
    let model = contract
        .model_selection
        .as_ref()
        .and_then(|selection| selection.model.clone());
    if model.as_deref() == Some("") {
        return Err(ProjectionError::InvalidAgent);
    }
    Ok(AuditActivity {
        schema: SCHEMA,
        event_type: EVENT_TYPE,
        event_id: format!("{EVENT_TYPE}:{}", writer.event_id),
        occurred_at_utc: writer.ts.clone(),
        source_route: SOURCE_ROUTE,
        data: ActivityData {
            event: row.kind.clone(),
            category,
            outcome: "not_applicable",
            platform: PLATFORM,
            home_id: context.home_id.clone(),
            day_key: day,
            session_id,
            task_id: Some(row.task_id.clone()),
            agent: Agent {
                role: context.role,
                harness: contract.agent.clone(),
                model,
            },
            actor: Actor {
                kind: writer.actor_type,
                id: writer.actor_id.clone(),
            },
            repo: None,
            pr: None,
            commit: None,
            user: User {
                id: identity.user_id.clone(),
                identity_source: identity.source,
            },
        },
        lineage: Lineage {
            event_id: writer.event_id.clone(),
            seq: writer.seq,
            prev_hash: writer.prev_hash.clone(),
        },
    })
}

/// Released category per session-scoped kind. The release names categories, not kinds; each
/// choice is recorded in `docs/audit-envelope/mcao.audit-envelope.v1.md`.
fn category_for(kind: &str) -> Result<&'static str, ProjectionError> {
    match kind {
        "session.created" | "session.state_changed" => Ok("lifecycle"),
        "cancellation.owner_manually_released" => Ok("supervision"),
        _ => Err(ProjectionError::UnmappedKind(kind.to_owned())),
    }
}

/// Derives the local calendar date of `ts` in `timezone`. The instant is never shifted to UTC.
fn derive_day_key(ts: &str, timezone: &str) -> Result<String, ProjectionError> {
    if !is_activity_timestamp(ts) {
        return Err(ProjectionError::InvalidTimestamp);
    }
    let instant = Timestamp::from_str(ts).map_err(|_| ProjectionError::InvalidTimestamp)?;
    let zone = TimeZone::get(timezone)
        .map_err(|_| ProjectionError::InvalidTimezone(timezone.to_owned()))?;
    Ok(instant.to_zoned(zone).date().to_string())
}

fn check_writer(writer: &WriterFields, identity: &ResolvedIdentity) -> Result<(), ProjectionError> {
    if !is_uuid(&writer.event_id, b'7') {
        return Err(ProjectionError::InvalidEventId);
    }
    let prev_hash_ok = match writer.prev_hash.as_deref() {
        None => writer.seq == 0,
        Some(hash) => writer.seq > 0 && is_sha256_ref(hash),
    };
    if !prev_hash_ok {
        return Err(ProjectionError::InvalidPrevHash(writer.seq));
    }
    let user_actor_mismatch =
        writer.actor_type == ActorType::User && writer.actor_id != identity.user_id;
    if writer.actor_id.is_empty() || user_actor_mismatch {
        return Err(ProjectionError::InvalidActor);
    }
    Ok(())
}

/// Canonical lowercase UUID with the given version digit and RFC 4122 variant.
fn is_uuid(value: &str, version: u8) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes[14] == version
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => is_lower_hex(*byte),
        })
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`, the released `occurred_at_utc` pattern.
fn is_activity_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 24
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            19 => *byte == b'.',
            23 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

fn is_user_id(value: &str) -> bool {
    value
        .strip_prefix("hmac-sha256:")
        .is_some_and(is_lower_hex64)
}

fn is_sha256_ref(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(is_lower_hex64)
}

fn is_lower_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(is_lower_hex)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}
