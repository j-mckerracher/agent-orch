//! Standing grants and their audit trail (DPES-1105 phase 5a, migration 0047).
//!
//! A grant lets an action run without a per-action approval, so every write here fails closed
//! and every refusal is typed. The store reads no clock: the caller passes `now_ms`. A grant
//! row is created once and never edited; only its use count (up by one per use) and its
//! revoke time (set once) change, and the migration's triggers refuse anything else. A grant
//! is never deleted, and `standing_grant_events` is append-only, so the audit survives a
//! history clear.
//!
//! [`SqliteStore::consume_grant_use`] is the one atomic claim: in a single transaction it
//! refuses a grant that is revoked, expired, out of uses or out of token budget, adds one use,
//! and writes the `used` event. The use is never refunded.
//!
//! The token budget counts tokens the usage receipts report for the tasks the grant started
//! (its `executed` events), since the grant was created. A figure a provider did not report
//! counts as zero, so the budget blocks new uses only and never cancels a task.

use std::fmt;
use std::io;

use orchestrator_core::orchestrator_action::ActionKind;
use orchestrator_core::standing_grant::{GrantRefusal, GrantScope, GrantTerms, StandingGrant};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::sqlite_actions::{
    approve_pending, fail_approved, load_proposal, ActionProposal, ProposalState, ResolveError,
};
use crate::sqlite_conversations::corrupt;
use crate::{SqliteStore, StoreError};

const COLUMNS: &str = "id, scope_kind, scope_id, kinds, created_at_ms, expires_at_ms, max_uses, \
                       uses_used, token_budget, proposal_floor, revoked_at_ms, \
                       confirm_unattended_merge, branch_prefix";
const EVENT_COLUMNS: &str = "sequence, grant_id, event, proposal_id, digest, task_id, code, at_ms";

/// What one audit row records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantEventKind {
    Created,
    /// A use was consumed for a proposal.
    Used,
    /// The proposal ran; `task_id` names the task it started or acted on, if any.
    Executed,
    /// The proposal did not run to the end; `code` says why.
    Stopped,
    Revoked,
}

impl GrantEventKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Used => "used",
            Self::Executed => "executed",
            Self::Stopped => "stopped",
            Self::Revoked => "revoked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "created" => Some(Self::Created),
            "used" => Some(Self::Used),
            "executed" => Some(Self::Executed),
            "stopped" => Some(Self::Stopped),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

/// One audit row. Holds ids and digests, never parameters or text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantEvent {
    pub sequence: i64,
    pub grant_id: String,
    pub event: GrantEventKind,
    pub proposal_id: Option<String>,
    pub digest: Option<String>,
    pub task_id: Option<String>,
    pub code: Option<String>,
    pub at_ms: u64,
}

/// How an execution under a grant ended, for [`SqliteStore::record_grant_outcome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantOutcome<'a> {
    Executed { task_id: Option<&'a str> },
    Stopped { code: &'a str },
}

/// Why the store refused a grant call. Carried inside a `StoreError::Io` of kind
/// `InvalidInput`; [`GrantError::of`] reads it back. A refused call writes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantError {
    /// The terms failed core validation.
    Invalid(GrantRefusal),
    /// An id, proposal id or digest is empty, too long or not lowercase hex.
    BadInput,
    NotFound,
    /// A grant with this id exists with other terms.
    IdInUse,
    Revoked,
    Expired,
    UsesExhausted,
    BudgetExhausted,
    /// This proposal already consumed a use.
    AlreadyUsed,
    /// The outcome names a use that was never recorded.
    NotUsed,
}

impl fmt::Display for GrantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => {
                write!(formatter, "the grant terms are refused: {}", reason.code())
            }
            Self::BadInput => {
                formatter.write_str("the grant id, proposal id or digest is malformed")
            }
            Self::NotFound => formatter.write_str("no standing grant has this id"),
            Self::IdInUse => formatter.write_str("a standing grant with this id has other terms"),
            Self::Revoked => formatter.write_str("the standing grant is revoked"),
            Self::Expired => formatter.write_str("the standing grant has expired"),
            Self::UsesExhausted => formatter.write_str("the standing grant has no uses left"),
            Self::BudgetExhausted => {
                formatter.write_str("the standing grant's token budget is spent")
            }
            Self::AlreadyUsed => formatter.write_str("this proposal already consumed a grant use"),
            Self::NotUsed => formatter.write_str("this proposal has no recorded grant use"),
        }
    }
}

impl std::error::Error for GrantError {}

impl From<GrantError> for StoreError {
    fn from(error: GrantError) -> Self {
        StoreError::Io(io::Error::new(io::ErrorKind::InvalidInput, error))
    }
}

/// Why [`SqliteStore::approve_action_proposal_by_grant`] refused. A refused call writes nothing.
#[derive(Debug)]
pub enum GrantClaimError {
    /// The grant refused the use: revoked, expired, out of uses or out of token budget.
    Grant(GrantError),
    /// The proposal is gone, no longer pending, or waits on a parent.
    Proposal(ResolveError),
    /// The proposal's digest is not the one the matcher read.
    DigestChanged,
}

impl From<StoreError> for GrantClaimError {
    fn from(error: StoreError) -> Self {
        match GrantError::of(&error) {
            Some(refusal) => Self::Grant(refusal),
            None => Self::Proposal(ResolveError::Store(error)),
        }
    }
}

impl From<sqlx::Error> for GrantClaimError {
    fn from(error: sqlx::Error) -> Self {
        Self::from(StoreError::Database(error))
    }
}

impl From<ResolveError> for GrantClaimError {
    fn from(error: ResolveError) -> Self {
        Self::Proposal(error)
    }
}

impl GrantError {
    /// The refusal inside `error`, if it is one.
    pub fn of(error: &StoreError) -> Option<Self> {
        match error {
            StoreError::Io(cause) if cause.kind() == io::ErrorKind::InvalidInput => cause
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<Self>())
                .copied(),
            _ => None,
        }
    }
}

impl SqliteStore {
    /// Validates `terms` and writes the grant and its `created` event in one transaction.
    /// `proposal_floor` is the highest proposal event sequence at this moment, so the grant
    /// never matches a proposal that already exists. A repeat with the same id and the same
    /// terms returns the stored grant and writes nothing; the same id with other terms is
    /// [`GrantError::IdInUse`]. Refused terms are [`GrantError::Invalid`].
    pub fn create_standing_grant(
        &self,
        id: &str,
        terms: &GrantTerms,
        now_ms: u64,
    ) -> Result<StandingGrant, StoreError> {
        terms.validate().map_err(GrantError::Invalid)?;
        if !(1..=120).contains(&id.chars().count()) {
            return Err(StoreError::from(GrantError::BadInput));
        }
        let created = to_i64(now_ms)?;
        let expires = to_i64(terms.expires_at_ms(now_ms))?;
        let kinds = serde_json::to_string(
            &terms
                .kinds
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| corrupt(format!("grant kinds do not encode: {error}")))?;
        let id = id.to_owned();
        let terms = terms.clone();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            if let Some(stored) = read_grant(&mut transaction, &id).await? {
                if same_terms(&stored, &terms) {
                    return Ok(stored);
                }
                return Err(StoreError::from(GrantError::IdInUse));
            }
            let floor: i64 =
                sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM action_proposal_events")
                    .fetch_one(&mut *transaction)
                    .await?;
            sqlx::query(
                "INSERT INTO standing_grants (id, scope_kind, scope_id, kinds, created_at_ms, \
                 expires_at_ms, max_uses, token_budget, proposal_floor, \
                 confirm_unattended_merge, branch_prefix) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(terms.scope.kind_str())
            .bind(terms.scope.id())
            .bind(&kinds)
            .bind(created)
            .bind(expires)
            .bind(i64::from(terms.max_uses))
            .bind(to_i64(terms.token_budget)?)
            .bind(floor)
            .bind(terms.confirm_unattended_merge)
            .bind(terms.branch_prefix.as_deref())
            .execute(&mut *transaction)
            .await?;
            insert_event(
                &mut transaction,
                &id,
                GrantEventKind::Created,
                None,
                None,
                None,
                created,
            )
            .await?;
            let saved = read_grant(&mut transaction, &id)
                .await?
                .ok_or_else(|| corrupt("the standing grant just written is missing".to_owned()))?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// One grant by id.
    pub fn standing_grant(&self, id: &str) -> Result<Option<StandingGrant>, StoreError> {
        let id = id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            read_grant(&mut connection, &id).await
        })
    }

    /// Newest first, at most `limit`. Without `include_ended` only grants a matcher may still
    /// use: not revoked, not expired at `now_ms`, uses left. The token budget is not read here;
    /// [`SqliteStore::consume_grant_use`] checks it.
    pub fn standing_grants(
        &self,
        include_ended: bool,
        now_ms: u64,
        limit: usize,
    ) -> Result<Vec<StandingGrant>, StoreError> {
        let now = to_i64(now_ms)?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let filter = if include_ended {
                ""
            } else {
                "WHERE revoked_at_ms IS NULL AND expires_at_ms > ? AND uses_used < max_uses "
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `COLUMNS` and `filter` are fixed text; `now` and `limit` are bound.
                "SELECT {COLUMNS} FROM standing_grants {filter}\
                 ORDER BY created_at_ms DESC, rowid DESC LIMIT ?"
            )));
            if !include_ended {
                query = query.bind(now);
            }
            let rows = query.bind(limit).fetch_all(&pool).await?;
            rows.iter().map(grant_from_row).collect()
        })
    }

    /// Revokes a grant at `now_ms` and writes the `revoked` event, in one transaction. A grant
    /// already revoked is returned unchanged and writes nothing. An unknown id is
    /// [`GrantError::NotFound`]. The use count is untouched: a revoke refunds nothing.
    pub fn revoke_standing_grant(
        &self,
        id: &str,
        now_ms: u64,
    ) -> Result<StandingGrant, StoreError> {
        let at = to_i64(now_ms)?;
        let id = id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let saved = revoke_in(&mut transaction, &id, at).await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// [`SqliteStore::revoke_standing_grant`], and in the same transaction every proposal this
    /// grant approved that has not started is moved to `failed` with `grant_revoked` and a
    /// `stopped` event, so nothing under the grant starts after the revoke commits. Returns the
    /// grant and how many proposals were stopped. A grant already revoked still stops any
    /// proposal it left approved.
    pub fn revoke_standing_grant_and_stop(
        &self,
        id: &str,
        now_ms: u64,
    ) -> Result<(StandingGrant, usize), StoreError> {
        let at = to_i64(now_ms)?;
        let id = id.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let saved = revoke_in(&mut transaction, &id, at).await?;
            let actor = format!("grant:{id}");
            let waiting: Vec<String> = sqlx::query_scalar(
                "SELECT e.proposal_id FROM action_proposal_events e \
                 WHERE e.to_state = 'approved' AND e.actor = ? \
                   AND (SELECT p.state FROM action_proposals p WHERE p.id = e.proposal_id) = 'approved' \
                 ORDER BY e.sequence",
            )
            .bind(&actor)
            .fetch_all(&mut *transaction)
            .await?;
            let mut stopped = 0;
            for proposal_id in waiting {
                let Some(proposal) = load_proposal(&mut transaction, &proposal_id).await? else {
                    continue;
                };
                fail_approved(&mut transaction, &proposal, REVOKED_CODE, &actor)
                    .await
                    .map_err(resolve_to_store)?;
                // A proposal with no recorded use has no audit row to close, and still stops.
                if let Err(error) = close_use(
                    &mut transaction,
                    &id,
                    &proposal.id,
                    &proposal.digest,
                    (GrantEventKind::Stopped, None, Some(REVOKED_CODE)),
                    at.max(to_i64(saved.created_at_ms)?),
                )
                .await
                {
                    if GrantError::of(&error) != Some(GrantError::NotUsed) {
                        return Err(error);
                    }
                }
                stopped += 1;
            }
            transaction.commit().await?;
            Ok((saved, stopped))
        })
    }

    /// The claim that makes a grant approval: in ONE `BEGIN IMMEDIATE` transaction, checks the
    /// proposal is still pending with `digest`, consumes one use of the grant (the same checks
    /// as [`SqliteStore::consume_grant_use`]), moves the proposal to `approved` with event
    /// actor `grant:<grant_id>`, and writes the `used` event. All of it commits or none of it
    /// does, so a use is never spent without an approval and an approval never lacks a use.
    pub fn approve_action_proposal_by_grant(
        &self,
        id: &str,
        digest: &str,
        grant_id: &str,
        now_ms: u64,
    ) -> Result<ActionProposal, GrantClaimError> {
        if to_i64(now_ms).is_err() {
            return Err(GrantClaimError::Grant(GrantError::BadInput));
        }
        let (id, digest, grant_id) = (id.to_owned(), digest.to_owned(), grant_id.to_owned());
        self.call(move |pool| async move {
            Ok::<_, StoreError>(claim_in(pool, id, digest, grant_id, now_ms).await)
        })
        .map_err(|error| GrantClaimError::Proposal(ResolveError::Store(error)))?
    }

    /// The atomic claim. In one `BEGIN IMMEDIATE` transaction: refuses a grant that is revoked
    /// ([`GrantError::Revoked`]), expired at `now_ms` (`Expired`), out of uses
    /// (`UsesExhausted`) or whose reported tokens reached its budget (`BudgetExhausted`), and a
    /// proposal that already consumed a use (`AlreadyUsed`); otherwise adds one use and writes
    /// the `used` event with the proposal id and digest. Returns the grant as updated. Two
    /// callers racing for the last use: exactly one succeeds. The use is never refunded.
    pub fn consume_grant_use(
        &self,
        grant_id: &str,
        proposal_id: &str,
        digest: &str,
        now_ms: u64,
    ) -> Result<StandingGrant, StoreError> {
        if !valid_proposal_and_digest(proposal_id, digest) {
            return Err(StoreError::from(GrantError::BadInput));
        }
        to_i64(now_ms)?;
        let grant_id = grant_id.to_owned();
        let proposal_id = proposal_id.to_owned();
        let digest = digest.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            let saved =
                consume_in(&mut transaction, &grant_id, &proposal_id, &digest, now_ms).await?;
            transaction.commit().await?;
            Ok(saved)
        })
    }

    /// Closes a use with its outcome: an `executed` event (with the task it started, so the
    /// token budget can find its receipts) or a `stopped` event with a short code. The use
    /// must have been consumed for this proposal and digest ([`GrantError::NotUsed`]
    /// otherwise). A proposal that already has an outcome event is left alone, so a retry after
    /// a crash writes nothing twice.
    pub fn record_grant_outcome(
        &self,
        grant_id: &str,
        proposal_id: &str,
        digest: &str,
        outcome: GrantOutcome<'_>,
        now_ms: u64,
    ) -> Result<(), StoreError> {
        if !valid_proposal_and_digest(proposal_id, digest) {
            return Err(StoreError::from(GrantError::BadInput));
        }
        let (event, task_id, code) = match outcome {
            GrantOutcome::Executed { task_id } => {
                (GrantEventKind::Executed, task_id.map(str::to_owned), None)
            }
            GrantOutcome::Stopped { code } => {
                if !(1..=64).contains(&code.len()) {
                    return Err(StoreError::from(GrantError::BadInput));
                }
                (GrantEventKind::Stopped, None, Some(code.to_owned()))
            }
        };
        let at = to_i64(now_ms)?;
        let grant_id = grant_id.to_owned();
        let proposal_id = proposal_id.to_owned();
        let digest = digest.to_owned();
        self.call(move |pool| async move {
            let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
            close_use(
                &mut transaction,
                &grant_id,
                &proposal_id,
                &digest,
                (event, task_id.as_deref(), code.as_deref()),
                at,
            )
            .await?;
            transaction.commit().await?;
            Ok(())
        })
    }

    /// Tokens reported so far (input, output and reasoning, each only when reported) by the
    /// usage receipts of every task this grant started, for receipts that end at or after the
    /// grant's creation. The estimated receipt of the same session is never counted. An
    /// unknown grant reads as zero.
    pub fn grant_tokens_spent(&self, grant_id: &str) -> Result<u64, StoreError> {
        let grant_id = grant_id.to_owned();
        self.call(move |pool| async move {
            let mut connection = pool.acquire().await?;
            let Some(grant) = read_grant(&mut connection, &grant_id).await? else {
                return Ok(0);
            };
            tokens_spent(&mut connection, &grant).await
        })
    }

    /// Audit rows of one grant after sequence `after`, oldest first, at most `limit`.
    pub fn grant_events(
        &self,
        grant_id: &str,
        after: i64,
        limit: usize,
    ) -> Result<Vec<GrantEvent>, StoreError> {
        let grant_id = grant_id.to_owned();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.call(move |pool| async move {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                // Safe assert: `EVENT_COLUMNS` is a fixed column list; the values are bound.
                "SELECT {EVENT_COLUMNS} FROM standing_grant_events \
                 WHERE grant_id = ? AND sequence > ? ORDER BY sequence LIMIT ?"
            )))
            .bind(&grant_id)
            .bind(after)
            .bind(limit)
            .fetch_all(&pool)
            .await?;
            rows.iter().map(event_from_row).collect()
        })
    }
}

/// Code of the `stopped` event and the `failed` proposal that a revoke leaves behind.
const REVOKED_CODE: &str = "grant_revoked";

fn resolve_to_store(error: ResolveError) -> StoreError {
    match error {
        ResolveError::Store(error) => error,
        other => corrupt(format!(
            "a proposal of a grant could not be stopped: {other:?}"
        )),
    }
}

/// Revokes inside the caller's transaction. See [`SqliteStore::revoke_standing_grant`].
async fn revoke_in(
    connection: &mut SqliteConnection,
    id: &str,
    at: i64,
) -> Result<StandingGrant, StoreError> {
    let stored = read_grant(connection, id)
        .await?
        .ok_or_else(|| StoreError::from(GrantError::NotFound))?;
    if stored.revoked_at_ms.is_some() {
        return Ok(stored);
    }
    // A caller clock behind the grant's creation still revokes: the column check
    // needs the time to be at least `created_at_ms`.
    let revoked_at = at.max(to_i64(stored.created_at_ms)?);
    sqlx::query(
        "UPDATE standing_grants SET revoked_at_ms = ? WHERE id = ? AND revoked_at_ms IS NULL",
    )
    .bind(revoked_at)
    .bind(id)
    .execute(&mut *connection)
    .await?;
    insert_event(
        connection,
        id,
        GrantEventKind::Revoked,
        None,
        None,
        None,
        revoked_at,
    )
    .await?;
    read_grant(connection, id)
        .await?
        .ok_or_else(|| corrupt("the standing grant just revoked is missing".to_owned()))
}

/// Consumes one use inside the caller's transaction. See [`SqliteStore::consume_grant_use`].
async fn consume_in(
    connection: &mut SqliteConnection,
    grant_id: &str,
    proposal_id: &str,
    digest: &str,
    now_ms: u64,
) -> Result<StandingGrant, StoreError> {
    let now = to_i64(now_ms)?;
    let grant = read_grant(connection, grant_id)
        .await?
        .ok_or_else(|| StoreError::from(GrantError::NotFound))?;
    if grant.revoked_at_ms.is_some() {
        return Err(StoreError::from(GrantError::Revoked));
    }
    if now_ms >= grant.expires_at_ms {
        return Err(StoreError::from(GrantError::Expired));
    }
    if grant.uses_used >= grant.max_uses {
        return Err(StoreError::from(GrantError::UsesExhausted));
    }
    let already: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM standing_grant_events WHERE event = 'used' AND proposal_id = ?",
    )
    .bind(proposal_id)
    .fetch_one(&mut *connection)
    .await?;
    if already > 0 {
        return Err(StoreError::from(GrantError::AlreadyUsed));
    }
    if tokens_spent(connection, &grant).await? >= grant.token_budget {
        return Err(StoreError::from(GrantError::BudgetExhausted));
    }
    let updated = sqlx::query(
        "UPDATE standing_grants SET uses_used = uses_used + 1 \
         WHERE id = ? AND revoked_at_ms IS NULL AND uses_used < max_uses AND expires_at_ms > ?",
    )
    .bind(grant_id)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(corrupt(
            "a checked standing grant refused its use".to_owned(),
        ));
    }
    insert_event(
        connection,
        grant_id,
        GrantEventKind::Used,
        Some((proposal_id, digest)),
        None,
        None,
        now,
    )
    .await?;
    read_grant(connection, grant_id)
        .await?
        .ok_or_else(|| corrupt("the standing grant just used is missing".to_owned()))
}

/// Writes the closing event of a use inside the caller's transaction, unless the use already
/// has one. See [`SqliteStore::record_grant_outcome`].
async fn close_use(
    connection: &mut SqliteConnection,
    grant_id: &str,
    proposal_id: &str,
    digest: &str,
    (event, task_id, code): (GrantEventKind, Option<&str>, Option<&str>),
    at: i64,
) -> Result<(), StoreError> {
    let used: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM standing_grant_events \
         WHERE grant_id = ? AND event = 'used' AND proposal_id = ? AND digest = ?",
    )
    .bind(grant_id)
    .bind(proposal_id)
    .bind(digest)
    .fetch_one(&mut *connection)
    .await?;
    if used == 0 {
        return Err(StoreError::from(GrantError::NotUsed));
    }
    let closed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM standing_grant_events \
         WHERE grant_id = ? AND event IN ('executed', 'stopped') AND proposal_id = ?",
    )
    .bind(grant_id)
    .bind(proposal_id)
    .fetch_one(&mut *connection)
    .await?;
    if closed > 0 {
        return Ok(());
    }
    insert_event(
        connection,
        grant_id,
        event,
        Some((proposal_id, digest)),
        task_id,
        code,
        at,
    )
    .await
}

/// The claim of [`SqliteStore::approve_action_proposal_by_grant`] in one transaction.
async fn claim_in(
    pool: SqlitePool,
    id: String,
    digest: String,
    grant_id: String,
    now_ms: u64,
) -> Result<ActionProposal, GrantClaimError> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let proposal = load_proposal(&mut transaction, &id)
        .await?
        .ok_or(GrantClaimError::Proposal(ResolveError::NotFound))?;
    if proposal.state != ProposalState::Pending {
        return Err(GrantClaimError::Proposal(ResolveError::NotPending(
            proposal.state,
        )));
    }
    if proposal.digest != digest {
        return Err(GrantClaimError::DigestChanged);
    }
    consume_in(
        &mut transaction,
        &grant_id,
        &proposal.id,
        &proposal.digest,
        now_ms,
    )
    .await?;
    let approved =
        approve_pending(&mut transaction, &proposal, &format!("grant:{grant_id}")).await?;
    transaction.commit().await?;
    Ok(approved)
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| GrantError::BadInput.into())
}

fn to_u64(row: &SqliteRow, column: &str) -> Result<u64, StoreError> {
    let value: i64 = row.try_get(column)?;
    u64::try_from(value).map_err(|_| corrupt(format!("standing grant column {column} is negative")))
}

fn valid_proposal_and_digest(proposal_id: &str, digest: &str) -> bool {
    (1..=160).contains(&proposal_id.chars().count())
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether a stored grant is the grant `terms` would create (kind order does not matter).
fn same_terms(stored: &StandingGrant, terms: &GrantTerms) -> bool {
    stored.scope == terms.scope
        && stored.kinds.len() == terms.kinds.len()
        && terms.kinds.iter().all(|kind| stored.kinds.contains(kind))
        && stored.max_uses == terms.max_uses
        && stored.token_budget == terms.token_budget
        && stored.expires_at_ms.saturating_sub(stored.created_at_ms)
            == terms.expires_in_secs.saturating_mul(1000)
        && stored.confirm_unattended_merge == terms.confirm_unattended_merge
        && stored.branch_prefix == terms.branch_prefix
}

async fn read_grant(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<StandingGrant>, StoreError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        // Safe assert: `COLUMNS` is a fixed column list; the id is bound.
        "SELECT {COLUMNS} FROM standing_grants WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(grant_from_row).transpose()
}

async fn tokens_spent(
    connection: &mut SqliteConnection,
    grant: &StandingGrant,
) -> Result<u64, StoreError> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(COALESCE(r.input_tokens, 0) + COALESCE(r.output_tokens, 0) \
                            + COALESCE(r.reasoning_tokens, 0)), 0) \
         FROM usage_receipts r \
         WHERE r.source = 'provider_reported' AND r.window_end_ms >= ? \
           AND r.task_id IN (SELECT task_id FROM standing_grant_events \
                             WHERE grant_id = ? AND event = 'executed' AND task_id IS NOT NULL)",
    )
    .bind(to_i64(grant.created_at_ms)?)
    .bind(&grant.id)
    .fetch_one(connection)
    .await?;
    u64::try_from(total).map_err(|_| corrupt("a token total is negative".to_owned()))
}

async fn insert_event(
    connection: &mut SqliteConnection,
    grant_id: &str,
    event: GrantEventKind,
    proposal: Option<(&str, &str)>,
    task_id: Option<&str>,
    code: Option<&str>,
    at_ms: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO standing_grant_events (grant_id, event, proposal_id, digest, task_id, code, at_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(grant_id)
    .bind(event.as_str())
    .bind(proposal.map(|(proposal_id, _)| proposal_id))
    .bind(proposal.map(|(_, digest)| digest))
    .bind(task_id)
    .bind(code)
    .bind(at_ms)
    .execute(connection)
    .await?;
    Ok(())
}

fn grant_from_row(row: &SqliteRow) -> Result<StandingGrant, StoreError> {
    let scope_kind: String = row.try_get("scope_kind")?;
    let scope_id: String = row.try_get("scope_id")?;
    let scope = GrantScope::from_parts(&scope_kind, &scope_id)
        .ok_or_else(|| corrupt(format!("unknown grant scope kind {scope_kind:?}")))?;
    let kinds_json: String = row.try_get("kinds")?;
    let names: Vec<String> = serde_json::from_str(&kinds_json)
        .map_err(|error| corrupt(format!("grant kinds are not a string list: {error}")))?;
    let kinds = names
        .iter()
        .map(|name| {
            ActionKind::parse(name).ok_or_else(|| corrupt(format!("unknown grant kind {name:?}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let revoked: Option<i64> = row.try_get("revoked_at_ms")?;
    Ok(StandingGrant {
        id: row.try_get("id")?,
        scope,
        kinds,
        created_at_ms: to_u64(row, "created_at_ms")?,
        expires_at_ms: to_u64(row, "expires_at_ms")?,
        max_uses: u32::try_from(to_u64(row, "max_uses")?)
            .map_err(|_| corrupt("grant max_uses is too large".to_owned()))?,
        uses_used: u32::try_from(to_u64(row, "uses_used")?)
            .map_err(|_| corrupt("grant uses_used is too large".to_owned()))?,
        token_budget: to_u64(row, "token_budget")?,
        revoked_at_ms: revoked
            .map(|value| {
                u64::try_from(value)
                    .map_err(|_| corrupt("grant revoke time is negative".to_owned()))
            })
            .transpose()?,
        proposal_floor: row.try_get("proposal_floor")?,
        confirm_unattended_merge: row.try_get("confirm_unattended_merge")?,
        branch_prefix: row.try_get("branch_prefix")?,
    })
}

fn event_from_row(row: &SqliteRow) -> Result<GrantEvent, StoreError> {
    let event: String = row.try_get("event")?;
    Ok(GrantEvent {
        sequence: row.try_get("sequence")?,
        grant_id: row.try_get("grant_id")?,
        event: GrantEventKind::parse(&event)
            .ok_or_else(|| corrupt(format!("unknown grant event {event:?}")))?,
        proposal_id: row.try_get("proposal_id")?,
        digest: row.try_get("digest")?,
        task_id: row.try_get("task_id")?,
        code: row.try_get("code")?,
        at_ms: to_u64(row, "at_ms")?,
    })
}
