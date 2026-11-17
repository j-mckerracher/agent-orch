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
//!    when the head moved.
//!
//! A restart or a lost reply never repeats step 5. An attempt left `issued` or `unknown` is
//! settled by reading the pull request alone ([`DaemonApi::settle_attempt`]). A red or
//! pending check, or a moved head, ends the card `failed` with a stable code and is never
//! retried by the daemon; the user proposes again.
//!
//! Remote text (a title, a check name) is plain, redacted and cut before it leaves the daemon.

use std::collections::BTreeMap;

use orchestrator_core::landing::{
    merge_verdict, CheckState, MergeMethod, MergeRefusal, PrObservation, PrState, FRESH_MAX_AGE_MS,
    STORED_MAX_AGE_MS,
};
use orchestrator_core::orchestrator_action::{
    Action, ActionKind as ProposalKind, MergePullRequest,
};
use orchestrator_core::project_mode::ProjectMode;
use orchestrator_core::task_envelope::Denial;
use orchestrator_git::github::{GitHubError, PullRequestView};
use orchestrator_store::{
    ActionOutcome, ActionProposal, MergeAttempt, MergeClaim, MergeState, PullRequestRow,
    ResolveError,
};

use super::actions::{cut_at_char, map_resolve_error};
use super::landing::{lock, Watched, REFRESH_MIN_MS, SETTLE_INTERVAL_MS, SETTLE_READS};
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    #[serde(default)]
    project_id: Option<String>,
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
        if mode == ProjectMode::LocalOnly
            || self
                .store
                .project_is_archived(&project_id)
                .map_err(map_store_error)?
        {
            return Err(RpcError::NotFound);
        }
        {
            let mut schedule = lock(&self.landing.schedule);
            let limited = schedule
                .refreshed
                .get(&request.task_id)
                .is_some_and(|last| now_ms < last.saturating_add(REFRESH_MIN_MS));
            if limited {
                return Err(RpcError::Conflict);
            }
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
                .last()
                .map(|space| space.branch.clone())
                .ok_or(RpcError::NotFound)?,
        };
        let watched = Watched {
            task_id: request.task_id.clone(),
            project_id: project_id.clone(),
            branch,
            number: stored.map(|row| row.number),
        };
        let repo = self.repo_for_project(&project_id).map_err(refresh_error)?;
        let mut calls = 0;
        self.observe_one(&watched, &repo, now_ms, &mut calls)
            .map_err(refresh_error)?
            .ok_or(RpcError::NotFound)?;
        let row = self
            .store
            .pull_request(&request.task_id)
            .map_err(map_store_error)?
            .ok_or(RpcError::NotFound)?;
        self.pull_wire(&row, mode, method, now_ms)
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
            if self.merge_inner(proposal, now_ms).is_err() {
                self.config.diagnostics.warn(LOG, "merge_failed", &[]);
            }
        }
        self.settle_orphans(&merges, now_ms);
        merges.len()
    }

    /// Runs one approved `delivery.merge` proposal. Safe to call again for the same proposal,
    /// after a restart or while a reply is lost: the ledger decides what is left to do, and the
    /// host's `merge` is called at most once per proposal, ever.
    pub(super) fn merge_inner(
        &self,
        proposal: &ActionProposal,
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
        // No grant can approve a merge. A proposal that says it did not start.
        let approver = self
            .store
            .proposal_approver(&proposal.id)
            .map_err(map_store_error)?;
        if approver
            .as_deref()
            .is_some_and(|actor| actor.starts_with("grant:"))
        {
            return self.finish_merge(
                Some(proposal),
                ActionOutcome::Failed {
                    error_code: "forbidden",
                },
            );
        }
        if let Some(attempt) = self
            .store
            .pull_request_merge(&proposal.id)
            .map_err(map_store_error)?
        {
            return self.continue_attempt(proposal, &merge, &attempt, now_ms);
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
            MergeClaim::Claimed(_) => self.run_claimed(proposal, &merge, &job, now_ms),
            // A second approval, or a replay: the first claim owns the merge.
            MergeClaim::Existing(attempt) => {
                self.continue_attempt(proposal, &merge, &attempt, now_ms)
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

    /// Steps 3 to 5 for an attempt in state `claimed`. Nothing has reached the host yet, so a
    /// failure before `issued` refuses the attempt and a store failure leaves it `claimed`,
    /// to run again from the read.
    fn run_claimed(
        &self,
        proposal: &ActionProposal,
        merge: &MergePullRequest,
        job: &MergeJob,
        now_ms: u64,
    ) -> Result<(), RpcError> {
        let id = proposal.id.as_str();
        let refuse = |code: &str| -> Result<(), RpcError> {
            self.advance_merge(id, MergeState::Refused, Some(code), None)?;
            self.finish_merge(Some(proposal), ActionOutcome::Failed { error_code: code })
        };
        let repo = match self.repo_for_project(&job.project_id) {
            Ok(repo) => repo,
            Err(error) => return refuse(host_code(error)),
        };
        let view = match self.github.view(&repo, merge.pr_number) {
            Ok(view) => view,
            Err(error) => return refuse(host_code(error)),
        };
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
        self.advance_merge(id, MergeState::Issued, None, None)?;
        match self
            .github
            .merge(&repo, merge.pr_number, &merge.head_sha, job.method)
        {
            Ok(()) => {
                let merge_sha = self
                    .github
                    .view(&repo, merge.pr_number)
                    .ok()
                    .filter(|view| view.state == PrState::Merged)
                    .and_then(|view| view.merge_sha);
                self.advance_merge(id, MergeState::Merged, None, merge_sha.as_deref())?;
                self.finish_merge(
                    Some(proposal),
                    ActionOutcome::Executed {
                        result_ref: &merged_ref(merge),
                    },
                )
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
                Ok(job) => self.run_claimed(proposal, merge, &job, now_ms),
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
            PrState::Merged => {
                self.advance_merge(id, MergeState::Merged, None, view.merge_sha.as_deref())?;
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
        self.store
            .advance_pull_request_merge(proposal_id, to, refusal, merge_sha)
            .map_err(map_store_error)
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

/// How a host failure of a refresh reads to the caller. Fixed codes only.
fn refresh_error(error: GitHubError) -> RpcError {
    match error {
        GitHubError::NotFound => RpcError::NotFound,
        GitHubError::RateLimited => RpcError::Conflict,
        _ => RpcError::Internal,
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
