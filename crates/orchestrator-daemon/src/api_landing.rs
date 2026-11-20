//! Pull requests and merges (DPES-1105 phase 4c): `pulls.list`, `pulls.refresh` and the
//! execution of an approved `delivery.merge`.
//!
//! No method merges. A merge is reachable only through a `delivery.merge` proposal that the
//! approver resolved (`actions.resolve` is user-only), and it runs on the landing thread,
//! never inside the idempotency gate: a host call can take a minute and must not hold it.
//! The single-run guarantee is the primary key of `pull_request_merges`, not the idempotency
//! table.
//!
//! For one approved proposal, in order:
//!
//! 1. The project must be in a pull request mode. A local-only project never merges and the
//!    host is not asked.
//! 2. The merge is CLAIMED in the ledger. A second claim of the proposal finds the first.
//! 3. The pull request is read fresh. [`merge_verdict`] runs on that read, never on a stored
//!    row, with the head commit the card bound.
//! 4. The attempt moves to `issued` and the state is committed BEFORE the host is called.
//! 5. The host is called once, with the bound head (`--match-head-commit`): the host refuses
//!    when the head moved. Only the caller that moved the attempt from `claimed` to `issued`
//!    calls it. The merge is recorded when a read shows the pull request merged at the bound
//!    head; a call that went through and shows nothing yet (a merge queue) stays `unknown`.
//!
//! A restart or a lost reply never repeats step 5. An attempt left `issued` or `unknown` is
//! settled by reading the pull request alone ([`DaemonApi::settle_attempt`]). A red or
//! pending check, or a moved head, ends the card `failed` with a stable code and is never
//! retried by the daemon; the user proposes again.
//!
//! A merge grant (N8) is the one other authority. The matcher approves a `delivery.merge`
//! proposal under it with no click, and the same steps run under [`MergePolicy::GrantGreenOnly`],
//! which adds, on the fresh read and in this order: `stale_head`, `cross_repo`,
//! `checks_not_green`, `branch_not_covered`, `own_upstream`, `repo_not_tracked` (the repository
//! is not the one the poller first bound the pull request to), then the rule of
//! [`merge_verdict`]. Each refusal ends the card `failed` with that code and the grant records
//! `stopped`. The grant is read again, with the proposal's state, in the one store call that moves the
//! claim to `issued`: a revoke before it stops the merge, one after it cannot stop a call
//! already made. The merge still goes out once, bound to the head, and the ledger decides
//! that, not the grant.
//!
//! Remote text (a title, a check name) is plain, redacted and cut before it leaves the daemon.

use std::collections::BTreeMap;

use orchestrator_core::contract::DeliveryPolicy;
use orchestrator_core::landing::{
    merge_verdict, CheckState, MergeMethod, MergeRefusal, PrObservation, PrState, FRESH_MAX_AGE_MS,
    STORED_MAX_AGE_MS,
};
use orchestrator_core::orchestrator_action::{
    Action, ActionBlock, ActionKind as ProposalKind, MergePullRequest,
};
use orchestrator_core::project_mode::ProjectMode;
use orchestrator_core::standing_grant::{GrantScope, StandingGrant};
use orchestrator_core::task_envelope::Denial;
use orchestrator_git::github::{GitHubError, PullRequestView, RepoRef};
use orchestrator_store::{
    ActionOutcome, ActionProposal, GrantError, GrantIssue, GrantOutcome, MergeAttempt, MergeClaim,
    MergeState, PullRequestRow, ResolveError, TaskRecord,
};

use super::actions::{cut_at_char, map_resolve_error};
use super::landing::{
    lock, Watched, REFRESH_GLOBAL_CAP, REFRESH_MIN_MS, REFRESH_WINDOW_MS, SETTLE_INTERVAL_MS,
    SETTLE_READS,
};
use super::*;

const LOG: &str = "landing";
/// Most pull requests `pulls.list` returns.
const LIST_LIMIT: usize = 100;
/// Longest pull request title and check name on the wire, in bytes.
const TITLE_BYTES: usize = 120;
const CHECK_NAME_BYTES: usize = 80;
const BRANCH_BYTES: usize = 120;
/// The ledger's word for a merge that was asked and never confirmed.
const UNCONFIRMED: &str = "unconfirmed";
/// The proposal's failure code for it.
const MERGE_UNCONFIRMED: &str = "merge_unconfirmed";
const INVALID: &str = "invalid";
/// The name of MCAO's own repository, whatever its owner, and the owner and name of its
/// upstream as `git remote` reports them for this repository (`origin` is
/// `git@github.com:mc-innersource/motif-mcao.git`). A merge grant never merges here.
const OWN_UPSTREAM_REPO_NAME: &str = "motif-mcao";
const OWN_UPSTREAM_OWNER: &str = "mc-innersource";

/// Who stands behind a merge, read from the proposal's approver.
pub(super) enum MergePolicy {
    /// The user approved this card. The merge rule alone decides.
    PerAction,
    /// A merge grant approved it. Green required checks on the bound head only, a covered
    /// branch, never MCAO's own repository.
    GrantGreenOnly { grant_id: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    project_id: Option<String>,
}

/// Whether `repo` is MCAO's own upstream: the compiled-in owner and name pair, or the NAME
/// under any owner (case insensitive), so a fork or a moved repository is covered too. A
/// project that happens to hold another repository with that name is refused as well:
/// refusing too much is the safe side here. A repository renamed on the host is caught by
/// [`repo_is_tracked`], not here.
fn is_own_upstream(repo: &RepoRef) -> bool {
    repo.name().eq_ignore_ascii_case(OWN_UPSTREAM_REPO_NAME)
        || (repo.owner().eq_ignore_ascii_case(OWN_UPSTREAM_OWNER)
            && repo.name().eq_ignore_ascii_case(OWN_UPSTREAM_REPO_NAME))
}

/// Whether `repo` is the repository the poller first bound the task's pull request to. No
/// binding (a pull request never read since the binding existed) is not tracked.
fn repo_is_tracked(store: &SqliteStore, task_id: &str, repo: &RepoRef) -> Result<bool, StoreError> {
    Ok(store
        .pull_request_repo(task_id)?
        .is_some_and(|(owner, name)| {
            owner.eq_ignore_ascii_case(repo.owner()) && name.eq_ignore_ascii_case(repo.name())
        }))
}

/// The refusal a merge grant adds to a fresh read, or `None`. In this order: the head is not
/// the bound one, the pull request comes from a fork, a required check is missing or not
/// green, the head branch is outside the grant's prefix, the repository is MCAO's own. No
/// clock, no I/O.
fn grant_refusal(
    grant: &StandingGrant,
    merge: &MergePullRequest,
    view: &PullRequestView,
    repo: &RepoRef,
) -> Option<&'static str> {
    if view.head_sha != merge.head_sha {
        return Some("stale_head");
    }
    if !view.same_repository {
        return Some("cross_repo");
    }
    if view.checks.is_empty()
        || view
            .checks
            .iter()
            .any(|check| check.state != CheckState::Passed)
    {
        return Some("checks_not_green");
    }
    if !grant.covers_branch(&view.head_branch) {
        return Some("branch_not_covered");
    }
    if is_own_upstream(repo) {
        return Some("own_upstream");
    }
    None
}

/// What a merge needs to know of its task's project.
struct MergeJob {
    project_id: String,
    mode: ProjectMode,
    /// The method the merge asks for.
    method: MergeMethod,
    /// The method the project allows.
    project_method: MergeMethod,
}

/// Remote text as plain, redacted, one-line text of at most `max_bytes` bytes. A path or a
/// token in it becomes a marker, so a hostile title never reaches a prompt, a snapshot or a
/// card as written.
pub(super) fn remote_text(text: &str, max_bytes: usize) -> String {
    let flat = super::conversation_agent::sanitize(&text.replace('\r', " "));
    let redacted = super::wake_events::redact(&flat);
    let one_line = redacted.split_whitespace().collect::<Vec<_>>().join(" ");
    cut_at_char(&one_line, max_bytes).to_owned()
}

/// Remote text that is not redacted: a branch name. Plain and cut.
fn plain_text(text: &str, max_bytes: usize) -> String {
    let flat = super::conversation_agent::sanitize(&text.replace('\r', " "));
    let one_line = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    cut_at_char(&one_line, max_bytes).to_owned()
}

/// What the merge rule reads of a fresh view.
fn observation_of(view: &PullRequestView, now_ms: i64) -> PrObservation {
    let states: Vec<CheckState> = view.checks.iter().map(|check| check.state).collect();
    PrObservation::from_required_checks(view.state, &view.head_sha, view.mergeable, &states, now_ms)
}

/// The stable code a failed host call gives a refused attempt.
fn host_code(error: GitHubError) -> &'static str {
    match error {
        GitHubError::NotAvailable => "not_available",
        GitHubError::NotSignedIn => "not_signed_in",
        GitHubError::NotFound => "not_found",
        GitHubError::HeadMoved => "head_moved",
        GitHubError::NotMergeable => "not_mergeable",
        GitHubError::RateLimited => "rate_limited",
        GitHubError::Timeout => "timed_out",
        GitHubError::Failed("cross_repo") => "cross_repo",
        GitHubError::Failed(_) => "host_error",
    }
}

/// The failure code of a proposal whose attempt was refused with `refusal`.
fn failure_code(refusal: Option<&str>) -> &str {
    match refusal {
        Some(UNCONFIRMED) => MERGE_UNCONFIRMED,
        Some(code) => code,
        None => INVALID,
    }
}

/// The result a merged proposal records: the pull request and the commit it was bound to.
fn merged_ref(merge: &MergePullRequest) -> String {
    let short = merge.head_sha.get(..12).unwrap_or(&merge.head_sha);
    format!("pr:{}@{short}", merge.pr_number)
}

fn to_i64(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

/// Whether a task can have its pull request merged: it delivers by pull request and it
/// finished, as the poller reads only such tasks.
fn task_may_merge(task: &TaskRecord) -> bool {
    task.contract.delivery == DeliveryPolicy::PullRequest && task.state == TaskState::Succeeded
}

/// The block with the project's merge method written into each `delivery.merge` that names
/// none, for the reply sink to call before it stores the block. The card then shows, and the
/// digest binds, the method that will run: a later change of the project's default changes
/// nothing for a card already stored.
pub(super) fn with_project_merge_methods(store: &SqliteStore, block: ActionBlock) -> ActionBlock {
    block.with_merge_methods(|task_id| {
        let task = store.get(task_id).ok()?;
        let row = store
            .project_defaults_or_local(&task.contract.project_id)
            .ok()?;
        Some(super::projects_defaults::typed(&row).merge_method)
    })
}

/// The refusal a `delivery.merge` meets against the STORED pull request, for the card when it
/// is made and again when the user approves. Local reads only. A task or pull request that is
/// not stored, or a store that cannot be read, refuses (`stale`): no reading is not green. A
/// stored number that differs from the card's is another pull request (`head_moved`).
pub(super) fn merge_denial(
    store: &SqliteStore,
    merge: &MergePullRequest,
    now_ms: i64,
) -> Option<Denial> {
    let refuse = |refusal| Some(Denial::Merge(refusal));
    let Ok(task) = store.get(&merge.task_id) else {
        return refuse(MergeRefusal::Stale);
    };
    if !task_may_merge(&task) {
        return Some(Denial::Delivery);
    }
    let row = match store.pull_request(&merge.task_id) {
        Ok(Some(row)) => row,
        _ => return refuse(MergeRefusal::Stale),
    };
    if row.number != merge.pr_number {
        return refuse(MergeRefusal::HeadMoved);
    }
    let Ok(defaults) = store.project_defaults_or_local(&task.contract.project_id) else {
        return refuse(MergeRefusal::ProjectMode);
    };
    let defaults = super::projects_defaults::typed(&defaults);
    let method = merge
        .method
        .as_deref()
        .and_then(MergeMethod::parse)
        .unwrap_or(defaults.merge_method);
    merge_verdict(
        &merge.head_sha,
        &row.observation(),
        defaults.mode,
        method,
        defaults.merge_method,
        now_ms,
        STORED_MAX_AGE_MS,
    )
    .err()
    .map(Denial::Merge)
}

impl DaemonApi {
    /// The mode of a project and the merge method it allows. A project that cannot be read is
    /// an error, so nothing is assumed.
    pub(super) fn project_landing(
        &self,
        project_id: &str,
    ) -> Result<(ProjectMode, MergeMethod), StoreError> {
        let row = self.store.project_defaults_or_local(project_id)?;
        let defaults = super::projects_defaults::typed(&row);
        Ok((defaults.mode, defaults.merge_method))
    }

    /// The refusal a pending `delivery.merge` meets against the stored pull request as it is
    /// now, for `resolve_as` to apply on approve. `None` for every other kind. Local reads
    /// only.
    pub(super) fn recheck_merge(
        &self,
        proposal: &ActionProposal,
    ) -> Result<Option<Denial>, RpcError> {
        Ok(
            match serde_json::from_str::<Action>(&proposal.params_json) {
                Ok(Action::DeliveryMerge(merge)) => {
                    merge_denial(&self.store, &merge, to_i64(crate::executor::unix_now_ms()))
                }
                _ => None,
            },
        )
    }

    /// What a `delivery.merge` card shows of its task and pull request, read now: the task's
    /// title and state and, when the stored pull request is the card's, its facts. No path,
    /// no URL, no id beyond what the parameters hold.
    pub(super) fn merge_target(&self, merge: &MergePullRequest) -> Value {
        let Ok(task) = self.store.get(&merge.task_id) else {
            return Value::Null;
        };
        let mut target = json!({
            "task_title": cut_at_char(&task.contract.objective, 80),
            "task_state": serde_json::to_value(task.state).unwrap_or(Value::Null),
        });
        let row = self.store.pull_request(&merge.task_id).ok().flatten();
        if let (Some(row), Ok((mode, project_method))) =
            (row, self.project_landing(&task.contract.project_id))
        {
            if row.number == merge.pr_number {
                let method = merge
                    .method
                    .as_deref()
                    .and_then(MergeMethod::parse)
                    .unwrap_or(project_method);
                let now = to_i64(crate::executor::unix_now_ms());
                target["pull"] = pull_facts(&row, mode, method, project_method, now);
            }
        }
        target
    }

    /// `pulls.list`: the watched pull requests, most recently changed first. Read only, and it
    /// reads the store, not the host.
    pub(super) fn pulls_list(&self, params: Value) -> Result<Value, RpcError> {
        self.pulls_list_at(params, crate::executor::unix_now_ms())
    }

    pub(super) fn pulls_list_at(&self, params: Value, now_ms: u64) -> Result<Value, RpcError> {
        let request: ListParams = if params.is_null() {
            ListParams { project_id: None }
        } else {
            parse_params(params)?
        };
        let project = match &request.project_id {
            Some(token) => Some(self.listed_project(token)?),
            None => None,
        };
        let rows = self
            .store
            .list_pull_requests(project.as_deref(), LIST_LIMIT)
            .map_err(map_store_error)?;
        let mut projects: BTreeMap<String, (ProjectMode, MergeMethod)> = BTreeMap::new();
        let mut wire = Vec::with_capacity(rows.len());
        for row in &rows {
            let (mode, method) = match projects.get(&row.project_id) {
                Some(found) => *found,
                None => {
                    let found = self
                        .project_landing(&row.project_id)
                        .unwrap_or((ProjectMode::LocalOnly, MergeMethod::Squash));
                    projects.insert(row.project_id.clone(), found);
                    found
                }
            };
            // A project that went back to local-only lists nothing: the feature is off there.
            if mode == ProjectMode::LocalOnly {
                continue;
            }
            wire.push(self.pull_wire(row, mode, method, now_ms)?);
        }
        Ok(json!({"pulls": wire}))
    }

    /// The project a `pulls.list` names: its id, or the short code the snapshot shows.
    fn listed_project(&self, token: &str) -> Result<String, RpcError> {
        let projects = self.store.list_projects().map_err(map_store_error)?;
        projects
            .iter()
            .find(|project| project.id == token || project_code(&project.id) == token)
            .map(|project| project.id.clone())
            .ok_or(RpcError::NotFound)
    }

    /// `pulls.refresh`: one fresh read of one task's pull request, at most once every ten
    /// seconds per task. A project in `local-only` mode has none. The only write is the store's
    /// record of what the host said; nothing is sent to the host but a read.
    pub(super) fn pulls_refresh(&self, params: Value) -> Result<Value, RpcError> {
        self.pulls_refresh_at(params, crate::executor::unix_now_ms())
    }

    pub(super) fn pulls_refresh_at(&self, params: Value, now_ms: u64) -> Result<Value, RpcError> {
        let request: TaskIdParams = parse_params(params)?;
        let task = self.store.get(&request.task_id).map_err(map_store_error)?;
        let project_id = task.contract.project_id.clone();
        let (mode, method) = self.project_landing(&project_id).map_err(map_store_error)?;
        // Only what the poller would store: a pull request delivery of a finished task.
        if mode == ProjectMode::LocalOnly
            || !task_may_merge(&task)
            || self
                .store
                .project_is_archived(&project_id)
                .map_err(map_store_error)?
        {
            return Err(RpcError::NotFound);
        }
        {
            let mut schedule = lock(&self.landing.schedule);
            // The poller's rate-limit pause holds for a refresh too.
            if now_ms < schedule.paused_until_ms {
                return Err(RpcError::Conflict);
            }
            let limited = schedule
                .refreshed
                .get(&request.task_id)
                .is_some_and(|last| now_ms < last.saturating_add(REFRESH_MIN_MS));
            schedule
                .refresh_times
                .retain(|at| now_ms < at.saturating_add(REFRESH_WINDOW_MS));
            if limited || schedule.refresh_times.len() >= REFRESH_GLOBAL_CAP {
                return Err(RpcError::Conflict);
            }
            schedule.refresh_times.push(now_ms);
            // Set before the read: a failing read counts, so the limit holds.
            schedule.refreshed.insert(request.task_id.clone(), now_ms);
            if schedule.refreshed.len() > 1_000 {
                schedule
                    .refreshed
                    .retain(|_, last| now_ms < last.saturating_add(REFRESH_MIN_MS));
            }
        }
        let stored = self
            .store
            .pull_request(&request.task_id)
            .map_err(map_store_error)?;
        let branch = match &stored {
            Some(row) => row.branch.clone(),
            None => self
                .store
                .workspaces_for_task(&request.task_id)
                .map_err(map_store_error)?
                .iter()
                .rfind(|space| !space.is_in_place())
                .map(|space| space.branch.clone())
                .ok_or(RpcError::NotFound)?,
        };
        let watched = Watched {
            task_id: request.task_id.clone(),
            project_id: project_id.clone(),
            branch,
            number: stored.map(|row| row.number),
        };
        let repo = self
            .repo_for_project(&project_id)
            .map_err(|error| self.refresh_error(error, now_ms))?;
        let mut calls = 0;
        self.observe_one(&watched, &repo, now_ms, &mut calls)
            .map_err(|error| self.refresh_error(error, now_ms))?
            .ok_or(RpcError::NotFound)?;
        let row = self
            .store
            .pull_request(&request.task_id)
            .map_err(map_store_error)?
            .ok_or(RpcError::NotFound)?;
        self.pull_wire(&row, mode, method, now_ms)
    }

    /// How a host failure of a refresh reads to the caller. Fixed codes only. A rate limit
    /// pauses the poller too.
    fn refresh_error(&self, error: GitHubError, now_ms: u64) -> RpcError {
        match error {
            GitHubError::NotFound => RpcError::NotFound,
            GitHubError::RateLimited => {
                self.pause(now_ms);
                RpcError::Conflict
            }
            _ => RpcError::Internal,
        }
    }

    /// One pull request as the wire shows it. Remote text is redacted and cut.
    fn pull_wire(
        &self,
        row: &PullRequestRow,
        mode: ProjectMode,
        method: MergeMethod,
        now_ms: u64,
    ) -> Result<Value, RpcError> {
        let merge = self
            .store
            .latest_pull_request_merge(&row.task_id)
            .map_err(map_store_error)?;
        let mut wire = pull_facts(row, mode, method, method, to_i64(now_ms));
        wire["task_id"] = json!(row.task_id);
        wire["number"] = json!(row.number);
        wire["state"] = json!(row.state.as_str());
        wire["head_sha"] = json!(row.head_sha);
        wire["base_branch"] = json!(plain_text(&row.base_branch, BRANCH_BYTES));
        wire["branch"] = json!(plain_text(&row.branch, BRANCH_BYTES));
        wire["mergeable"] = json!(row.mergeable);
        wire["title"] = json!(remote_text(&row.title, TITLE_BYTES));
        wire["merge"] = match merge {
            Some(attempt) => json!({
                "state": attempt.state.as_str(),
                "refusal": attempt.refusal,
            }),
            None => Value::Null,
        };
        Ok(wire)
    }

    // ----- the merge worker -------------------------------------------------------------

    /// One pass of the merge worker at `now_ms`: runs every approved `delivery.merge`
    /// proposal (see [`Self::merge_inner`]), then settles the open attempts whose proposal is
    /// no longer approved. Returns how many approved proposals it looked at. Never inside the
    /// idempotency gate.
    pub(super) fn merge_pending(&self, now_ms: u64) -> usize {
        // A merge acts on the remote: hold the stop gate open for the whole pass.
        let Some(_serving) = crate::executor::serving(&self.stop_gate) else {
            return 0;
        };
        let approved = match self.store.approved_action_proposals() {
            Ok(approved) => approved,
            Err(_) => {
                self.config.diagnostics.warn(LOG, "merge_list_failed", &[]);
                return 0;
            }
        };
        let merges: Vec<ActionProposal> = approved
            .into_iter()
            .filter(|proposal| proposal.kind == ProposalKind::DeliveryMerge)
            .collect();
        for proposal in &merges {
            let run = self
                .merge_policy(proposal)
                .and_then(|policy| self.merge_inner(proposal, &policy, now_ms));
            if run.is_err() {
                self.config.diagnostics.warn(LOG, "merge_failed", &[]);
            }
        }
        self.settle_orphans(&merges, now_ms);
        merges.len()
    }

    /// The policy a proposal's approver stands for: a grant actor is a merge grant, anyone
    /// else a user approval.
    pub(super) fn merge_policy(&self, proposal: &ActionProposal) -> Result<MergePolicy, RpcError> {
        let approver = self
            .store
            .proposal_approver(&proposal.id)
            .map_err(map_store_error)?;
        Ok(
            match approver
                .as_deref()
                .and_then(|actor| actor.strip_prefix("grant:"))
            {
                Some(grant_id) => MergePolicy::GrantGreenOnly {
                    grant_id: grant_id.to_owned(),
                },
                None => MergePolicy::PerAction,
            },
        )
    }

    /// Runs one approved `delivery.merge` proposal under `policy`. Safe to call again for the
    /// same proposal, after a restart or while a reply is lost: the ledger decides what is
    /// left to do, and the host's `merge` is called at most once per proposal, ever.
    pub(super) fn merge_inner(
        &self,
        proposal: &ActionProposal,
        policy: &MergePolicy,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        let Ok(Action::DeliveryMerge(merge)) =
            serde_json::from_str::<Action>(&proposal.params_json)
        else {
            return self.finish_merge(
                Some(proposal),
                ActionOutcome::Failed {
                    error_code: INVALID,
                },
            );
        };
        // Defense in depth, as `resolve_as` checks it: only a Restricted turn carries a merge.
        let turn = self
            .store
            .conversation_turn(&proposal.turn_id)
            .map_err(super::conversations::map_conversation_error)?;
        if turn.and_then(|turn| turn.permission_level)
            != Some(orchestrator_core::permission::PermissionLevel::Restricted)
        {
            return self.finish_merge(
                Some(proposal),
                ActionOutcome::Failed {
                    error_code: INVALID,
                },
            );
        }
        if let Some(attempt) = self
            .store
            .pull_request_merge(&proposal.id)
            .map_err(map_store_error)?
        {
            return self.continue_attempt(proposal, &merge, &attempt, policy, now_ms);
        }
        let job = match self.merge_job(&merge) {
            Ok(job) => job,
            Err(code) => {
                return self
                    .finish_merge(Some(proposal), ActionOutcome::Failed { error_code: code })
            }
        };
        let claim = match self.store.claim_pull_request_merge(
            &proposal.id,
            &merge.task_id,
            merge.pr_number,
            &merge.head_sha,
            job.method,
        ) {
            Ok(claim) => claim,
            Err(StoreError::RecordNotFound { .. }) => {
                return self.finish_merge(
                    Some(proposal),
                    ActionOutcome::Failed {
                        error_code: "not_found",
                    },
                )
            }
            Err(error) => return Err(map_store_error(error)),
        };
        match claim {
            MergeClaim::Claimed(_) => self.run_claimed(proposal, &merge, &job, policy, now_ms),
            // A second approval, or a replay: the first claim owns the merge.
            MergeClaim::Existing(attempt) => {
                self.continue_attempt(proposal, &merge, &attempt, policy, now_ms)
            }
            // Another proposal is already merging this pull request.
            MergeClaim::Busy(_) => self.finish_merge(
                Some(proposal),
                ActionOutcome::Failed {
                    error_code: "conflict",
                },
            ),
        }
    }

    /// The project facts of a merge, or the stable failure code. A project in `local-only` mode
    /// never merges, and the host is not asked.
    fn merge_job(&self, merge: &MergePullRequest) -> Result<MergeJob, &'static str> {
        let task = match self.store.get(&merge.task_id) {
            Ok(task) => task,
            Err(StoreError::NotFound(_)) => return Err("not_found"),
            Err(_) => return Err("internal"),
        };
        if !task_may_merge(&task) {
            return Err("conflict");
        }
        let project_id = task.contract.project_id;
        match self.store.project_is_archived(&project_id) {
            Ok(false) => {}
            Ok(true) => return Err("conflict"),
            Err(_) => return Err("internal"),
        }
        let Ok((mode, project_method)) = self.project_landing(&project_id) else {
            return Err("internal");
        };
        if mode == ProjectMode::LocalOnly {
            return Err(MergeRefusal::ProjectMode.code());
        }
        let method = merge
            .method
            .as_deref()
            .and_then(MergeMethod::parse)
            .unwrap_or(project_method);
        Ok(MergeJob {
            project_id,
            mode,
            method,
            project_method,
        })
    }

    /// The merge grant behind a proposal, if it may still start this merge, else the stable
    /// refusal. The grant must be unrevoked and unexpired, still name `delivery.merge` for the
    /// project of the task, and the task's stored pull request must be the card's: a grant
    /// merges only the pull request the project tracks.
    fn grant_for_merge(
        &self,
        grant_id: &str,
        merge: &MergePullRequest,
        job: &MergeJob,
        now_ms: u64,
    ) -> Result<Result<StandingGrant, &'static str>, RpcError> {
        if let Some(code) = self.grant_stop_code(grant_id, now_ms)? {
            return Ok(Err(code));
        }
        let Some(grant) = self
            .store
            .standing_grant(grant_id)
            .map_err(|error| internal("grants", &error))?
        else {
            return Ok(Err(super::actions::GRANT_REVOKED));
        };
        let in_project = matches!(
            &grant.scope,
            GrantScope::Project(code) if *code == project_code(&job.project_id)
        );
        if !grant.covers(ProposalKind::DeliveryMerge) || !in_project {
            return Ok(Err("project_not_covered"));
        }
        let tracked = self
            .store
            .pull_request(&merge.task_id)
            .map_err(map_store_error)?;
        if tracked.is_none_or(|row| row.number != merge.pr_number) {
            return Ok(Err("pr_not_tracked"));
        }
        Ok(Ok(grant))
    }

    /// Steps 3 to 5 for an attempt in state `claimed`. Nothing has reached the host yet, so a
    /// failure before `issued` refuses the attempt and a store failure leaves it `claimed`,
    /// to run again from the read. Under a merge grant the grant is checked first and the
    /// grant's own refusals run on the fresh read before the merge rule.
    fn run_claimed(
        &self,
        proposal: &ActionProposal,
        merge: &MergePullRequest,
        job: &MergeJob,
        policy: &MergePolicy,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        let id = proposal.id.as_str();
        let refuse = |code: &str| -> Result<(), RpcError> {
            self.advance_merge(id, MergeState::Refused, Some(code), None)?;
            self.finish_merge(Some(proposal), ActionOutcome::Failed { error_code: code })
        };
        let grant = match policy {
            MergePolicy::PerAction => None,
            MergePolicy::GrantGreenOnly { grant_id } => {
                match self.grant_for_merge(grant_id, merge, job, now_ms)? {
                    Ok(grant) => Some(grant),
                    Err(code) => return refuse(code),
                }
            }
        };
        let repo = match self.repo_for_project(&job.project_id) {
            Ok(repo) => repo,
            Err(error) => return refuse(host_code(error)),
        };
        let view = match self.github.view(&repo, merge.pr_number) {
            Ok(view) => view,
            Err(error) => return refuse(host_code(error)),
        };
        if let Some(grant) = &grant {
            if let Some(code) = grant_refusal(grant, merge, &view, &repo) {
                return refuse(code);
            }
            // The read is the pull request the project tracks, not another one by number.
            let same_branch = self
                .store
                .pull_request(&merge.task_id)
                .map_err(map_store_error)?
                .is_some_and(|row| row.branch == view.head_branch);
            if !same_branch {
                return refuse("pr_not_tracked");
            }
            // And in the repository the poller bound it to, not a mirror under another name.
            if !repo_is_tracked(&self.store, &merge.task_id, &repo).map_err(map_store_error)? {
                return refuse("repo_not_tracked");
            }
        }
        // The rule runs on THIS read, never on a stored row.
        let seen = observation_of(&view, to_i64(now_ms));
        if let Err(refusal) = merge_verdict(
            &merge.head_sha,
            &seen,
            job.mode,
            job.method,
            job.project_method,
            to_i64(now_ms),
            FRESH_MAX_AGE_MS,
        ) {
            return refuse(refusal.code());
        }
        // Committed before the call: a restart after this point never calls `merge` again.
        // Only the caller that moves the row calls the host; a second caller on the same claim
        // stands down, and the first one ends the card.
        // Under a grant the grant and the proposal are read again in the same call, so a
        // revoke that committed after the check above stops the merge here.
        let issued = match &grant {
            Some(grant) => match self
                .store
                .issue_pull_request_merge_under_grant(id, &grant.id, now_ms)
                .map_err(map_store_error)?
            {
                GrantIssue::Issued => true,
                GrantIssue::NotMoved => false,
                GrantIssue::GrantRevoked => return refuse(super::actions::GRANT_REVOKED),
                GrantIssue::GrantExpired => return refuse(super::actions::GRANT_EXPIRED),
                GrantIssue::ProposalNotApproved => return refuse("conflict"),
            },
            None => self
                .store
                .issue_pull_request_merge(id)
                .map_err(map_store_error)?,
        };
        if !issued {
            return Ok(());
        }
        match self
            .github
            .merge(&repo, merge.pr_number, &merge.head_sha, job.method)
        {
            Ok(()) => {
                // A call that went through is not a merge: a merge queue or auto-merge only
                // queues it. It is recorded merged only when a read shows the bound head merged.
                let confirmed = self
                    .github
                    .view(&repo, merge.pr_number)
                    .ok()
                    .filter(|view| {
                        view.state == PrState::Merged && view.head_sha == merge.head_sha
                    });
                match confirmed {
                    Some(view) => {
                        let merge_sha = view.merge_sha.as_deref();
                        self.advance_merge(id, MergeState::Merged, None, merge_sha)?;
                        self.release_task_slots(std::slice::from_ref(&merge.task_id));
                        self.finish_merge(
                            Some(proposal),
                            ActionOutcome::Executed {
                                result_ref: &merged_ref(merge),
                            },
                        )
                    }
                    None => {
                        // Left for `settle_attempt`, which reads the pull request again.
                        self.advance_merge(id, MergeState::Unknown, None, None)?;
                        Ok(())
                    }
                }
            }
            Err(error)
                if matches!(error, GitHubError::HeadMoved | GitHubError::NotMergeable)
                    || !error.may_have_reached_host() =>
            {
                refuse(host_code(error))
            }
            Err(_) => {
                // The reply is lost and the merge may have landed: only a read settles it.
                self.advance_merge(id, MergeState::Unknown, None, None)?;
                Ok(())
            }
        }
    }

    /// What an attempt that already exists leaves to do. Never calls `merge`.
    fn continue_attempt(
        &self,
        proposal: &ActionProposal,
        merge: &MergePullRequest,
        attempt: &MergeAttempt,
        policy: &MergePolicy,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        match attempt.state {
            MergeState::Merged => self.finish_merge(
                Some(proposal),
                ActionOutcome::Executed {
                    result_ref: &merged_ref(merge),
                },
            ),
            MergeState::Refused => self.finish_merge(
                Some(proposal),
                ActionOutcome::Failed {
                    error_code: failure_code(attempt.refusal.as_deref()),
                },
            ),
            MergeState::Claimed => match self.merge_job(merge) {
                Ok(job) => self.run_claimed(proposal, merge, &job, policy, now_ms),
                Err(code) => {
                    self.advance_merge(&proposal.id, MergeState::Refused, Some(code), None)?;
                    self.finish_merge(Some(proposal), ActionOutcome::Failed { error_code: code })
                }
            },
            MergeState::Issued | MergeState::Unknown => {
                self.settle_attempt(Some(proposal), merge, attempt, now_ms)
            }
        }
    }

    /// Settles an attempt left `issued` or `unknown` by READING the pull request. Never calls
    /// `merge`. `Merged` ends it merged. A closed pull request, or an open one at another
    /// head, was not merged by this attempt (`refused`). An open one at the bound head is
    /// not assumed merged: an `issued` attempt (a daemon stopped mid-call) is refused
    /// `unconfirmed` at once, an `unknown` one (a lost reply) after three reads. A read that
    /// fails leaves the attempt as it is. Reads are two minutes apart.
    fn settle_attempt(
        &self,
        proposal: Option<&ActionProposal>,
        merge: &MergePullRequest,
        attempt: &MergeAttempt,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        let id = attempt.proposal_id.as_str();
        {
            let mut schedule = lock(&self.landing.schedule);
            if schedule.settle_due.get(id).is_some_and(|due| now_ms < *due) {
                return Ok(());
            }
            schedule
                .settle_due
                .insert(id.to_owned(), now_ms + SETTLE_INTERVAL_MS);
        }
        let Ok(task) = self.store.get(&attempt.task_id) else {
            return Ok(());
        };
        let Ok(repo) = self.repo_for_project(&task.contract.project_id) else {
            return Ok(());
        };
        let Ok(view) = self.github.view(&repo, attempt.number) else {
            return Ok(());
        };
        let refuse = |code: &str| -> Result<(), RpcError> {
            self.advance_merge(id, MergeState::Refused, Some(code), None)?;
            self.finish_merge(
                proposal,
                ActionOutcome::Failed {
                    error_code: failure_code(Some(code)),
                },
            )
        };
        match view.state {
            // Merged at a head this attempt did not bind: someone else's merge, not ours.
            PrState::Merged if view.head_sha != attempt.head_sha => refuse(UNCONFIRMED),
            PrState::Merged => {
                self.advance_merge(id, MergeState::Merged, None, view.merge_sha.as_deref())?;
                self.release_task_slots(std::slice::from_ref(&attempt.task_id));
                self.finish_merge(
                    proposal,
                    ActionOutcome::Executed {
                        result_ref: &merged_ref(merge),
                    },
                )
            }
            PrState::Closed => refuse(MergeRefusal::NotOpen.code()),
            PrState::Open if view.head_sha != attempt.head_sha => {
                refuse(MergeRefusal::HeadMoved.code())
            }
            PrState::Open if attempt.state == MergeState::Issued => refuse(UNCONFIRMED),
            PrState::Open => {
                let reads = {
                    let mut schedule = lock(&self.landing.schedule);
                    let reads = schedule.unknown_reads.entry(id.to_owned()).or_insert(0);
                    *reads = reads.saturating_add(1);
                    *reads
                };
                if reads >= SETTLE_READS {
                    refuse(UNCONFIRMED)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Settles the open attempts whose proposal is no longer approved (it ended, or a history
    /// clear removed it). The ledger holds the truth, so it is read to its end; no proposal
    /// outcome is written. A `claimed` attempt never reached the host and is closed refused.
    fn settle_orphans(&self, approved: &[ActionProposal], now_ms: u64) {
        let Ok(open) = self.store.open_pull_request_merges() else {
            self.config.diagnostics.warn(LOG, "merge_open_failed", &[]);
            return;
        };
        for attempt in open {
            if approved
                .iter()
                .any(|proposal| proposal.id == attempt.proposal_id)
            {
                continue;
            }
            self.config
                .diagnostics
                .warn(LOG, "merge_outcome_orphaned", &[]);
            let result = match attempt.state {
                MergeState::Claimed => self
                    .advance_merge(
                        &attempt.proposal_id,
                        MergeState::Refused,
                        Some("abandoned"),
                        None,
                    )
                    .map(|_| ()),
                _ => {
                    let merge = MergePullRequest {
                        task_id: attempt.task_id.clone(),
                        pr_number: attempt.number,
                        head_sha: attempt.head_sha.clone(),
                        method: None,
                    };
                    self.settle_attempt(None, &merge, &attempt, now_ms)
                }
            };
            if result.is_err() {
                self.config
                    .diagnostics
                    .warn(LOG, "merge_settle_failed", &[]);
            }
        }
    }

    fn advance_merge(
        &self,
        proposal_id: &str,
        to: MergeState,
        refusal: Option<&str>,
        merge_sha: Option<&str>,
    ) -> Result<MergeAttempt, RpcError> {
        let attempt = self
            .store
            .advance_pull_request_merge(proposal_id, to, refusal, merge_sha)
            .map_err(map_store_error)?;
        if attempt.state == MergeState::Merged {
            self.record_merge_fact(&attempt);
        }
        Ok(attempt)
    }

    /// Tells a coordinator's parent channel that a task it owns merged (DPES-1108 phase 7). The
    /// id derives from the merge attempt, so a second read of the merged pull request writes
    /// nothing. The merge is already recorded, so a failed write is logged, never returned.
    fn record_merge_fact(&self, attempt: &MergeAttempt) {
        let owner = match self.store.task_coordinator(&attempt.task_id) {
            Ok(Some((project_id, _))) => project_id,
            Ok(None) => return,
            Err(error) => {
                internal("landing", &error);
                return;
            }
        };
        let fact = orchestrator_store::CoordinatorFact::Merged;
        let recorded =
            self.store
                .record_coordinator_facts(&[orchestrator_store::NewCoordinatorFact {
                    id: idempotency::stable_id(
                        "fact",
                        &format!("{}:{}", attempt.proposal_id, fact.as_str()),
                    ),
                    project_id: owner,
                    fact,
                    task_id: attempt.task_id.clone(),
                    key: None,
                }]);
        if let Err(error) = recorded {
            internal("landing", &error);
        }
    }

    /// Records how the proposal ended, under the gate: a local write, never across a host
    /// call. A proposal that is no longer approved (a history clear removed it) records
    /// nothing; the ledger still holds the truth. `None` for an attempt with no live proposal.
    fn finish_merge(
        &self,
        proposal: Option<&ActionProposal>,
        outcome: ActionOutcome<'_>,
    ) -> Result<(), RpcError> {
        let Some(proposal) = proposal else {
            return Ok(());
        };
        let approver = self
            .store
            .proposal_approver(&proposal.id)
            .map_err(map_store_error)?
            .unwrap_or_else(|| "user".to_owned());
        let _held = self
            .idempotency_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // The grant's audit row first, as for every kind: a stop between the two writes leaves
        // the proposal approved, and the rerun closes it. The merged task is not named, so the
        // grant's token budget never counts a merge.
        if let Some(grant_id) = approver.strip_prefix("grant:") {
            let closed = match outcome {
                ActionOutcome::Executed { .. } => GrantOutcome::Executed { task_id: None },
                ActionOutcome::Failed { error_code } => GrantOutcome::Stopped { code: error_code },
            };
            let at = crate::executor::unix_now_ms();
            // An approval by a grant that never recorded a use (no claim made it) has no audit
            // row to close; the proposal still ends.
            match self.store.record_grant_outcome(
                grant_id,
                &proposal.id,
                &proposal.digest,
                closed,
                at,
            ) {
                Ok(()) => {}
                Err(error) if GrantError::of(&error) == Some(GrantError::NotUsed) => {}
                Err(error) => return Err(internal("grants", &error)),
            }
        }
        match self
            .store
            .record_action_outcome(&proposal.id, outcome, &approver)
        {
            Ok(_) => Ok(()),
            Err(ResolveError::NotPending(_) | ResolveError::NotFound) => {
                self.config
                    .diagnostics
                    .warn(LOG, "merge_outcome_orphaned", &[]);
                Ok(())
            }
            Err(error) => Err(map_resolve_error(error)),
        }
    }
}

/// The facts a pull request row and a merge card share. `merge_method` is the method the
/// merge asks for; the refusal is what the merge rule says of the STORED row now.
fn pull_facts(
    row: &PullRequestRow,
    mode: ProjectMode,
    method: MergeMethod,
    project_method: MergeMethod,
    now_ms: i64,
) -> Value {
    let refusal = merge_verdict(
        &row.head_sha,
        &row.observation(),
        mode,
        method,
        project_method,
        now_ms,
        STORED_MAX_AGE_MS,
    )
    .err()
    .map(MergeRefusal::code);
    let age_ms = now_ms.saturating_sub(row.observed_ms).max(0);
    json!({
        "merge_method": method.as_str(),
        "required_total": row.required_total,
        "required_passed": row.required_passed,
        "required_failed": row.required_failed,
        "checks": row
            .checks
            .iter()
            .map(|check| json!({
                "name": remote_text(&check.name, CHECK_NAME_BYTES),
                "result": check.state.as_str(),
            }))
            .collect::<Vec<_>>(),
        "observed_age_secs": age_ms / 1_000,
        "refusal": refusal,
    })
}
