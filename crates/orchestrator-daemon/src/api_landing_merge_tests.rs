//! Merging a pull request through an approved `delivery.merge` card (DPES-1105 phase 4c).
//! Every test drives an in-process `DaemonApi` over a real store with `FakeGitHub`: no network,
//! no `gh`, no agent. The merge runs when a test calls `merge_pending`, as the landing thread
//! does, so the call log of the double shows exactly what reached the host. A restart is a new
//! `DaemonApi` over the same data directory and the same double.

use orchestrator_core::landing::CheckState;
use orchestrator_core::landing::MergeMethod;
use orchestrator_core::orchestrator_action::{parse_block, Action, BlockScope};
use orchestrator_git::github::{CheckRun, GitHub, GitHubError, PullRequestView, RepoRef};
use orchestrator_git::github_fake::{Call, Method};
use orchestrator_store::{
    ConversationTarget, GrantClaimError, GrantEventKind, MergeClaim, MergeState, ProposalState,
    ResolveError, TurnBlock, TurnPermission,
};

use super::actions_task_tests::{register_project, ALPHA};
use super::actions_tests::{call, proposal, propose, propose_in, resolve};
use super::landing::{lock, SETTLE_INTERVAL_MS};
use super::landing_api::{merge_denial, with_project_merge_methods, MergePolicy};
use super::landing_tests::{
    failed_pr_task, landing, merge_action, now, set_mode, store_pull_request,
    submit_pr_task_not_finished, succeeded_task, view, Landing, BRANCH, HEAD, NUMBER, OTHER,
};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const SHORT: &str = "pr:7@0123456789ab";

/// A pull request the poller has read, and a pending merge card bound to its head.
fn card(l: &Landing) -> String {
    l.tick(now());
    propose(&l.api, "t1", &merge_action(&l.task, NUMBER, HEAD))[0].clone()
}

/// Reads the pull request again at the real time, whatever the poller schedule says. The
/// approve-time check reads the real clock, so a stored read must not lie in its future.
fn retick(l: &Landing) {
    {
        let mut schedule = lock(&l.api.landing.schedule);
        schedule.next_poll_ms = 0;
        schedule.next_due.clear();
    }
    l.tick(now());
}

fn approve(l: &Landing, id: &str, key: &str) -> Value {
    resolve(&l.api, id, "approve", key).unwrap()
}

/// Stores `action` the way the reply sink does: checked against the stored pull request, so a
/// merge that would be refused is stored already `failed`.
fn propose_checked(api: &DaemonApi, turn: &str, action: &str) -> Vec<String> {
    api.store
        .begin_conversation_turn(
            &ConversationTarget::Global,
            turn,
            "hi",
            "test-runtime",
            "1.0.0",
        )
        .unwrap();
    api.store
        .record_conversation_turn_permission(
            turn,
            TurnPermission {
                level: PermissionLevel::Restricted,
                profile: "test-profile",
            },
        )
        .unwrap();
    let held = format!("```mcao-actions\n{{\"mcao_actions\": 1, \"actions\": [{action}]}}\n```");
    let at = i64::try_from(now()).unwrap();
    // As the reply sink does: the project's method is written in before the block is stored.
    let parsed = parse_block(&held, BlockScope::Global).unwrap();
    let block =
        with_project_merge_methods(&api.store, parsed).checked_with(|action| match action {
            Action::DeliveryMerge(merge) => merge_denial(&api.store, merge, at),
            _ => None,
        });
    api.store
        .complete_conversation_turn(turn, TurnBlock::Accepted(block))
        .unwrap()
        .unwrap()
}

/// Closes the daemon and opens another over the same data directory and `GitHub` double.
fn restart(l: Landing) -> Landing {
    let host = l.github.clone() as Arc<dyn GitHub>;
    restart_over(l, host)
}

/// `restart` over another `GitHub`, which may wrap the same double.
fn restart_over(l: Landing, host: Arc<dyn GitHub>) -> Landing {
    let Landing {
        data,
        repo_dir,
        api,
        github,
        repo,
        project,
        task,
    } = l;
    drop(api);
    let api = DaemonApi::with_github(data.path(), host).unwrap();
    Landing {
        data,
        repo_dir,
        api,
        github,
        repo,
        project,
        task,
    }
}

/// Sets the merge method the project allows, keeping its mode.
fn set_method(l: &Landing, method: &str) {
    l.api
        .store
        .set_project_defaults(&orchestrator_store::ProjectDefaultsRow {
            project_id: l.project.clone(),
            mode: "no-mistakes".to_owned(),
            base_branch: None,
            branch_prefix: None,
            checks: vec![vec!["cargo".to_owned(), "test".to_owned()]],
            merge_method: method.to_owned(),
            updated_at: String::new(),
        })
        .unwrap();
}

/// A host that accepts a merge call and only queues it: the call succeeds, nothing merges and
/// the pull request stays open, as a merge queue or auto-merge does.
struct QueueingHost {
    inner: Arc<orchestrator_git::github_fake::FakeGitHub>,
    merges: AtomicUsize,
}

impl GitHub for QueueingHost {
    fn repo_of(&self, repository: &orchestrator_git::Repository) -> Result<RepoRef, GitHubError> {
        self.inner.repo_of(repository)
    }

    fn find_by_branch(&self, repo: &RepoRef, branch: &str) -> Result<Option<u64>, GitHubError> {
        self.inner.find_by_branch(repo, branch)
    }

    fn view(&self, repo: &RepoRef, number: u64) -> Result<PullRequestView, GitHubError> {
        self.inner.view(repo, number)
    }

    fn merge(
        &self,
        _repo: &RepoRef,
        _number: u64,
        _head_sha: &str,
        _method: MergeMethod,
    ) -> Result<(), GitHubError> {
        self.merges.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// A host that runs `hook` once, right after the first pull request read: the moment between the
/// grant's check and the host's merge call.
struct RevokingHost {
    inner: Arc<orchestrator_git::github_fake::FakeGitHub>,
    hook: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl GitHub for RevokingHost {
    fn repo_of(&self, repository: &orchestrator_git::Repository) -> Result<RepoRef, GitHubError> {
        self.inner.repo_of(repository)
    }

    fn find_by_branch(&self, repo: &RepoRef, branch: &str) -> Result<Option<u64>, GitHubError> {
        self.inner.find_by_branch(repo, branch)
    }

    fn view(&self, repo: &RepoRef, number: u64) -> Result<PullRequestView, GitHubError> {
        let seen = self.inner.view(repo, number);
        let hook = self.hook.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        seen
    }

    fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        head_sha: &str,
        method: MergeMethod,
    ) -> Result<(), GitHubError> {
        self.inner.merge(repo, number, head_sha, method)
    }
}

fn claim(l: &Landing, id: &str) {
    let claimed = l
        .api
        .store
        .claim_pull_request_merge(id, &l.task, NUMBER, HEAD, MergeMethod::Squash)
        .unwrap();
    assert!(matches!(claimed, MergeClaim::Claimed(_)));
}

fn attempt(l: &Landing, id: &str) -> orchestrator_store::MergeAttempt {
    l.api.store.pull_request_merge(id).unwrap().unwrap()
}

fn assert_failed_with(l: &Landing, id: &str, code: &str) {
    let stored = proposal(&l.api, id);
    assert_eq!(stored.state, ProposalState::Failed, "{stored:?}");
    assert_eq!(stored.error_code.as_deref(), Some(code), "{stored:?}");
}

#[test]
fn landing_a_green_pull_request_on_the_bound_head_merges_once_with_an_approved_card() {
    let l = landing();
    let id = card(&l);

    // Approval runs no network call: the card waits for the worker.
    let approved = approve(&l, &id, "k-1");
    assert_eq!(approved["proposal"]["state"], "approved", "{approved}");
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.api.store.pull_request_merge(&id).unwrap().is_none());

    assert_eq!(l.api.merge_pending(now()), 1);

    // One merge, with the bound commit and the project's method: never admin, never auto.
    assert_eq!(
        l.github.merge_calls(),
        vec![Call::Merge {
            repo: l.repo.clone(),
            number: NUMBER,
            head_sha: HEAD.to_owned(),
            method: MergeMethod::Squash,
        }]
    );
    let done = proposal(&l.api, &id);
    assert_eq!(done.state, ProposalState::Executed, "{done:?}");
    assert_eq!(done.result_ref.as_deref(), Some(SHORT));
    let ledger = attempt(&l, &id);
    assert_eq!(ledger.state, MergeState::Merged);
    assert_eq!(ledger.head_sha, HEAD);
    assert_eq!(ledger.merge_sha.as_ref().map(String::len), Some(40));
    assert_eq!(
        l.github.pull_request(&l.repo, NUMBER).unwrap().state,
        orchestrator_core::landing::PrState::Merged
    );

    // A later pass finds nothing to do and the ledger still shows one merge.
    l.api.merge_pending(now() + 1_000);
    assert_eq!(l.github.merge_count(), 1);
    assert!(l.api.store.open_pull_request_merges().unwrap().is_empty());
    let events = l.api.store.action_proposal_events(&id).unwrap();
    let transitions: Vec<(ProposalState, &str)> = events
        .iter()
        .skip(1)
        .map(|event| (event.to_state, event.actor.as_str()))
        .collect();
    assert_eq!(
        transitions,
        [
            (ProposalState::Approved, "user"),
            (ProposalState::Executed, "user")
        ]
    );
    // The list says merged.
    let listed = call(&l.api, "pulls.list", json!({})).unwrap();
    assert_eq!(listed["pulls"][0]["merge"]["state"], "merged", "{listed}");
}

#[test]
fn landing_an_approved_merge_stays_approved_until_the_worker_runs() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");

    let waiting = proposal(&l.api, &id);

    assert_eq!(waiting.state, ProposalState::Approved);
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.api.store.pull_request_merge(&id).unwrap().is_none());
}

#[test]
fn landing_github_is_called_only_with_the_planned_calls_in_the_planned_order() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");

    l.api.merge_pending(now());

    let repo = l.repo.clone();
    assert_eq!(
        l.calls(),
        vec![
            // The poll: the repository, the search by branch, the read.
            Call::RepoOf,
            Call::FindByBranch {
                repo: repo.clone(),
                branch: BRANCH.to_owned()
            },
            Call::View {
                repo: repo.clone(),
                number: NUMBER
            },
            // The merge: the repository, a fresh read, the one merge, the confirming read.
            Call::RepoOf,
            Call::View {
                repo: repo.clone(),
                number: NUMBER
            },
            Call::Merge {
                repo: repo.clone(),
                number: NUMBER,
                head_sha: HEAD.to_owned(),
                method: MergeMethod::Squash
            },
            Call::View {
                repo,
                number: NUMBER
            },
        ]
    );
}

#[test]
fn landing_a_merge_card_is_refused_for_a_local_only_project_at_every_step() {
    let l = landing();
    let pending = card(&l);
    set_mode(&l.api, &l.project, "local-only");

    // Approval: the card made before the mode changed is denied, nothing is approved.
    let denied = approve(&l, &pending, "k-1");
    assert_eq!(denied["proposal"]["state"], "failed", "{denied}");
    assert_eq!(denied["proposal"]["denial"], "project_mode", "{denied}");
    assert_failed_with(&l, &pending, "action_denied");

    // The card: a new merge for the local-only project is stored already failed.
    let ids = propose_checked(&l.api, "t2", &merge_action(&l.task, NUMBER, HEAD));
    let stored = proposal(&l.api, &ids[0]);
    assert_eq!(stored.state, ProposalState::Failed);
    assert_eq!(stored.denial.as_deref(), Some("project_mode"));

    // The worker, even for a card approved behind the checks: the host is not asked at all.
    let forced = propose(&l.api, "t3", &merge_action(&l.task, NUMBER, HEAD))[0].clone();
    l.api
        .store
        .approve_action_proposal(&forced, "user")
        .unwrap();
    let before = l.calls().len();
    l.api.merge_pending(now());
    assert_failed_with(&l, &forced, "project_mode");
    assert_eq!(l.calls().len(), before);
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.api.store.pull_request_merge(&forced).unwrap().is_none());
}

#[test]
fn landing_a_moved_head_fails_the_card_and_never_merges() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    l.github.set_head(&l.repo, NUMBER, OTHER);

    l.api.merge_pending(now());

    assert_failed_with(&l, &id, "head_moved");
    assert_eq!(l.github.merge_count(), 0);
    let ledger = attempt(&l, &id);
    assert_eq!(ledger.state, MergeState::Refused);
    assert_eq!(ledger.refusal.as_deref(), Some("head_moved"));
    // Never retried by the daemon.
    l.api.merge_pending(now() + 1_000);
    assert_eq!(l.github.merge_count(), 0);
    assert_eq!(
        l.github.pull_request(&l.repo, NUMBER).unwrap().state,
        orchestrator_core::landing::PrState::Open
    );
}

#[test]
fn landing_a_head_that_moves_between_the_read_and_the_merge_is_refused_by_the_host() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    l.github.move_head_after_next_view(&l.repo, NUMBER, OTHER);

    l.api.merge_pending(now());

    // The host was asked once, with the bound commit, and refused.
    assert_eq!(l.github.merge_count(), 1);
    assert_failed_with(&l, &id, "head_moved");
    assert_eq!(attempt(&l, &id).state, MergeState::Refused);
    assert_eq!(
        l.github.pull_request(&l.repo, NUMBER).unwrap().state,
        orchestrator_core::landing::PrState::Open
    );
}

#[test]
fn landing_the_fake_host_refuses_checks_that_turn_red_after_the_read() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    l.github.turn_checks_red_after_next_view(&l.repo, NUMBER);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_failed_with(&l, &id, "not_mergeable");
    assert_eq!(attempt(&l, &id).state, MergeState::Refused);
}

#[test]
fn landing_red_pending_missing_and_unknown_checks_fail_the_card_with_no_merge_call() {
    for (case, code) in [
        ("red", "checks_red"),
        ("pending", "checks_pending"),
        ("missing", "checks_missing"),
        ("unknown", "checks_unknown"),
    ] {
        let l = landing();
        let id = card(&l);
        approve(&l, &id, "k-1");
        let mut changed = l.github.pull_request(&l.repo, NUMBER).unwrap();
        match case {
            "red" => changed.checks[0].state = orchestrator_core::landing::CheckState::Failed,
            "pending" => changed.checks[1].state = orchestrator_core::landing::CheckState::Pending,
            "missing" => changed.checks.clear(),
            _ => changed.mergeable = None,
        }
        l.github.set_pull_request(&l.repo, changed);

        l.api.merge_pending(now());

        assert_failed_with(&l, &id, code);
        assert_eq!(l.github.merge_count(), 0, "{case}");
        assert_eq!(attempt(&l, &id).refusal.as_deref(), Some(code), "{case}");
    }
}

/// Sequential replays only: two callers at once are `landing_two_callers_on_one_claim_call_the_host_once`.
#[test]
fn landing_the_same_proposal_approved_twice_merges_once() {
    let l = landing();
    let id = card(&l);
    let first = approve(&l, &id, "k-1");
    // The same key replays the answer; another key finds nothing pending.
    assert_eq!(approve(&l, &id, "k-1"), first);
    assert_eq!(
        resolve(&l.api, &id, "approve", "k-2").unwrap_err(),
        RpcError::ProposalNotPending
    );
    let approved = proposal(&l.api, &id);
    assert_eq!(approved.state, ProposalState::Approved);

    // Two signals for one approval: the ledger holds one merge.
    l.api
        .merge_inner(&approved, &MergePolicy::PerAction, now())
        .unwrap();
    l.api
        .merge_inner(&approved, &MergePolicy::PerAction, now())
        .unwrap();
    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
    // And a later claim of the proposal finds the first.
    let again = l
        .api
        .store
        .claim_pull_request_merge(&id, &l.task, NUMBER, HEAD, MergeMethod::Squash)
        .unwrap();
    assert!(matches!(again, MergeClaim::Existing(_)));
}

#[test]
fn landing_a_restart_between_the_claim_and_the_host_call_resolves_to_one_merge() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    // The daemon stopped after the claim: nothing was sent.
    claim(&l, &id);
    let l = restart(l);
    assert_eq!(attempt(&l, &id).state, MergeState::Claimed);
    assert_eq!(l.github.merge_count(), 0);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
    assert_eq!(attempt(&l, &id).state, MergeState::Merged);
    assert!(l.api.store.open_pull_request_merges().unwrap().is_empty());
}

#[test]
fn landing_a_restart_with_an_issued_attempt_reads_the_pull_request_and_never_merges() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    // The daemon stopped after `issued` was committed and before the reply: the call may have
    // been sent.
    claim(&l, &id);
    l.api
        .store
        .advance_pull_request_merge(&id, MergeState::Issued, None, None)
        .unwrap();
    let l = restart(l);
    let reads = l.count(Method::View);

    l.api.merge_pending(now());

    // A read, no merge. The pull request is open at the bound head, which is not assumed
    // merged: the card fails as unconfirmed and the user asks again.
    assert!(l.count(Method::View) > reads);
    assert_eq!(l.github.merge_count(), 0);
    assert_failed_with(&l, &id, "merge_unconfirmed");
    let ledger = attempt(&l, &id);
    assert_eq!(ledger.state, MergeState::Refused);
    assert_eq!(ledger.refusal.as_deref(), Some("unconfirmed"));
}

#[test]
fn landing_a_restart_with_an_issued_attempt_that_landed_ends_merged_without_a_second_merge() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    claim(&l, &id);
    l.api
        .store
        .advance_pull_request_merge(&id, MergeState::Issued, None, None)
        .unwrap();
    // The host merged it before the daemon stopped.
    l.github
        .merge(&l.repo, NUMBER, HEAD, MergeMethod::Squash)
        .unwrap();
    let before = l.github.merge_count();
    let l = restart(l);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), before);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
    assert_eq!(attempt(&l, &id).state, MergeState::Merged);
}

#[test]
fn landing_a_lost_merge_reply_resolves_to_exactly_one_merge() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    l.github.lose_next_merge_reply();
    let t = now();

    l.api.merge_pending(t);

    // The merge landed and the reply was lost: unknown, still waiting, one call.
    assert_eq!(attempt(&l, &id).state, MergeState::Unknown);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    assert_eq!(l.github.merge_count(), 1);

    // One read finds it merged. The host is never asked to merge again.
    l.api.merge_pending(t + 1_000);

    assert_eq!(attempt(&l, &id).state, MergeState::Merged);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
    assert_eq!(l.github.merge_count(), 1);
}

#[test]
fn landing_a_lost_merge_reply_that_never_landed_is_given_up_after_three_reads_without_a_retry() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    l.github.fail_next(Method::Merge, GitHubError::Timeout);
    let t = now();
    l.api.merge_pending(t);
    assert_eq!(attempt(&l, &id).state, MergeState::Unknown);

    // Reads are two minutes apart: the first at once, the others when due.
    l.api.merge_pending(t);
    let reads = l.count(Method::View);
    l.api.merge_pending(t + 60_000);
    assert_eq!(l.count(Method::View), reads);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    l.api.merge_pending(t + SETTLE_INTERVAL_MS);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    l.api.merge_pending(t + 2 * SETTLE_INTERVAL_MS);

    assert_failed_with(&l, &id, "merge_unconfirmed");
    let ledger = attempt(&l, &id);
    assert_eq!(ledger.state, MergeState::Refused);
    assert_eq!(ledger.refusal.as_deref(), Some("unconfirmed"));
    // One call in all, and never another for this proposal.
    assert_eq!(l.github.merge_count(), 1);
    l.api.merge_pending(t + 10 * SETTLE_INTERVAL_MS);
    assert_eq!(l.github.merge_count(), 1);
}

#[test]
fn landing_a_failure_before_any_request_leaves_refuses_the_card_and_asks_nothing() {
    for (error, code) in [
        (GitHubError::NotSignedIn, "not_signed_in"),
        (GitHubError::NotAvailable, "not_available"),
    ] {
        let l = landing();
        let id = card(&l);
        approve(&l, &id, "k-1");
        l.github.fail_next(Method::Merge, error);

        l.api.merge_pending(now());

        assert_failed_with(&l, &id, code);
        // The attempt was refused, not left unknown: nothing was sent.
        assert_eq!(attempt(&l, &id).state, MergeState::Refused);
    }
}

#[test]
fn landing_delivery_merge_from_an_auto_turn_is_never_executable() {
    let l = landing();
    l.tick(now());
    let ids = propose_in(
        &l.api,
        &ConversationTarget::Global,
        "t1",
        PermissionLevel::Auto,
        &merge_action(&l.task, NUMBER, HEAD),
    );
    assert_eq!(
        resolve(&l.api, &ids[0], "approve", "k-1").unwrap_err(),
        RpcError::ProposalInvalid
    );

    // Even approved behind the gate, the worker refuses a card from a turn that was not
    // Restricted.
    l.api
        .store
        .approve_action_proposal(&ids[0], "user")
        .unwrap();
    let before = l.calls().len();
    l.api.merge_pending(now());

    assert_failed_with(&l, &ids[0], "invalid");
    assert_eq!(l.calls().len(), before);
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.api.store.pull_request_merge(&ids[0]).unwrap().is_none());
}

#[test]
fn landing_a_merge_approved_by_a_grant_that_does_not_exist_never_reaches_the_host() {
    let l = landing();
    let id = card(&l);
    l.api
        .store
        .approve_action_proposal(&id, "grant:g-1")
        .unwrap();

    l.api.merge_pending(now());

    // No such grant fails closed, as a revoked one does, and the host is never asked.
    assert_failed_with(&l, &id, "grant_revoked");
    assert_eq!(l.github.merge_count(), 0);
    assert!(matches!(attempt(&l, &id).state, MergeState::Refused));
}

#[test]
fn landing_a_merge_for_a_task_with_no_stored_pull_request_fails_not_found_and_asks_nothing() {
    let l = landing();
    // No poll: the app never read the pull request.
    let id = propose(&l.api, "t1", &merge_action(&l.task, NUMBER, HEAD))[0].clone();
    l.api.store.approve_action_proposal(&id, "user").unwrap();

    l.api.merge_pending(now());

    assert_failed_with(&l, &id, "not_found");
    assert!(l.calls().is_empty());
    assert!(l.api.store.pull_request_merge(&id).unwrap().is_none());
}

#[test]
fn landing_a_card_for_a_stale_head_or_an_unread_pull_request_is_stored_failed() {
    let l = landing();
    // Never read: refused as stale.
    let unread = propose_checked(&l.api, "t0", &merge_action(&l.task, NUMBER, HEAD));
    assert_eq!(
        proposal(&l.api, &unread[0]).denial.as_deref(),
        Some("stale")
    );

    l.tick(now());
    let green = propose_checked(&l.api, "t1", &merge_action(&l.task, NUMBER, HEAD));
    let moved = propose_checked(&l.api, "t2", &merge_action(&l.task, NUMBER, OTHER));
    let other = propose_checked(&l.api, "t3", &merge_action(&l.task, NUMBER + 1, HEAD));
    let method = propose_checked(
        &l.api,
        "t4",
        &json!({
            "kind": "delivery.merge", "task_id": l.task, "pr_number": NUMBER,
            "head_sha": HEAD, "method": "rebase",
        })
        .to_string(),
    );

    let green = proposal(&l.api, &green[0]);
    assert_eq!(green.state, ProposalState::Pending);
    assert_eq!(green.denial, None);
    for (ids, denial) in [
        (&moved, "head_moved"),
        (&other, "head_moved"),
        (&method, "method_not_allowed"),
    ] {
        let stored = proposal(&l.api, &ids[0]);
        assert_eq!(stored.state, ProposalState::Failed, "{denial}");
        assert_eq!(
            stored.error_code.as_deref(),
            Some("action_denied"),
            "{denial}"
        );
        assert_eq!(stored.denial.as_deref(), Some(denial));
    }
    // A denied card replaced nothing: the pending card is still the user's to decide.
    assert_eq!(proposal(&l.api, &green.id).state, ProposalState::Pending);
}

#[test]
fn landing_approving_a_card_that_went_red_before_the_click_is_denied_with_the_reason() {
    let l = landing();
    let id = card(&l);
    let mut red = l.github.pull_request(&l.repo, NUMBER).unwrap();
    red.checks[0].state = orchestrator_core::landing::CheckState::Failed;
    l.github.set_pull_request(&l.repo, red);
    retick(&l);

    let result = approve(&l, &id, "k-1");

    assert_eq!(result["proposal"]["state"], "failed", "{result}");
    assert_eq!(
        result["proposal"]["error_code"], "action_denied",
        "{result}"
    );
    assert_eq!(result["proposal"]["denial"], "checks_red", "{result}");
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.api.store.pull_request_merge(&id).unwrap().is_none());
}

#[test]
fn landing_the_merge_card_wire_shows_the_stored_facts_and_the_refusal() {
    let l = landing();
    let id = card(&l);

    let pending = call(&l.api, "actions.pending", json!({})).unwrap();

    let wire = &pending["proposals"][0];
    assert_eq!(wire["id"], id.as_str());
    assert_eq!(wire["kind"], "delivery.merge");
    assert_eq!(wire["params"]["pr_number"], NUMBER);
    assert_eq!(wire["params"]["head_sha"], HEAD);
    assert_eq!(wire["params"]["task_id"], l.task.as_str());
    assert_eq!(wire["denial"], Value::Null);
    let target = &wire["target"];
    assert_eq!(target["task_title"], "Add tests (task-pr-1)");
    assert_eq!(target["task_state"], "Succeeded");
    let pull = &target["pull"];
    assert_eq!(pull["merge_method"], "squash");
    assert_eq!(pull["required_total"], 2);
    assert_eq!(pull["required_passed"], 2);
    assert_eq!(pull["required_failed"], 0);
    assert_eq!(pull["refusal"], Value::Null);
    assert_eq!(
        pull["checks"],
        json!([
            {"name": "ci / build", "result": "passed"},
            {"name": "ci / test", "result": "passed"},
        ])
    );
    assert!(pull["observed_age_secs"].is_u64(), "{pull}");

    // A check that goes red shows as the refusal a merge would meet now.
    let mut red = l.github.pull_request(&l.repo, NUMBER).unwrap();
    red.checks[1].state = orchestrator_core::landing::CheckState::Failed;
    l.github.set_pull_request(&l.repo, red);
    retick(&l);
    let again = call(&l.api, "actions.pending", json!({})).unwrap();
    assert_eq!(
        again["proposals"][0]["target"]["pull"]["refusal"], "checks_red",
        "{again}"
    );
    // No path anywhere in it.
    assert!(!again.to_string().contains(&l.project), "{again}");
}

#[test]
fn landing_a_restart_resumes_an_approved_merge_that_never_started() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    // Approved, never claimed: the daemon stopped between the click and the worker.
    let l = restart(l);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    assert_eq!(l.github.merge_count(), 0);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
}

#[test]
fn landing_a_host_merge_that_the_view_does_not_confirm_is_not_recorded_as_merged() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    let host = Arc::new(QueueingHost {
        inner: l.github.clone(),
        merges: AtomicUsize::new(0),
    });
    let l = restart_over(l, host.clone() as Arc<dyn GitHub>);

    l.api.merge_pending(now());

    // The call went through and the pull request is still open: not a merge.
    assert_eq!(host.merges.load(Ordering::SeqCst), 1);
    let ledger = attempt(&l, &id);
    assert_eq!(ledger.state, MergeState::Unknown);
    assert_eq!(ledger.merge_sha, None);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    // A later read settles it and the host is never asked again.
    l.api.merge_pending(now() + SETTLE_INTERVAL_MS);
    assert_eq!(host.merges.load(Ordering::SeqCst), 1);
    assert_ne!(attempt(&l, &id).state, MergeState::Merged);
    assert_ne!(proposal(&l.api, &id).state, ProposalState::Executed);
}

#[test]
fn landing_two_callers_on_one_claim_call_the_host_once() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    let approved = proposal(&l.api, &id);
    let barrier = std::sync::Barrier::new(2);

    // The final state is not asserted: in production one landing thread runs merges, so a
    // second caller settling an `issued` attempt while the first is mid-call is not modeled.
    // What holds for any interleaving is the number of host calls.
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                barrier.wait();
                let _ = l.api.merge_inner(&approved, &MergePolicy::PerAction, now());
            });
        }
    });

    // Whichever caller moved the claim to issued called the host; the other stood down.
    assert_eq!(l.github.merge_count(), 1);
    // A caller that finds the claim already issued never calls the host either.
    assert!(!l.api.store.issue_pull_request_merge(&id).unwrap());
}

#[test]
fn landing_a_pull_request_merged_at_another_head_is_not_recorded_as_our_merge() {
    let l = landing();
    let id = card(&l);
    approve(&l, &id, "k-1");
    claim(&l, &id);
    l.api
        .store
        .advance_pull_request_merge(&id, MergeState::Issued, None, None)
        .unwrap();
    // Someone else merged the pull request, at a head this card never bound.
    let mut theirs = l.github.pull_request(&l.repo, NUMBER).unwrap();
    theirs.state = orchestrator_core::landing::PrState::Merged;
    theirs.head_sha = OTHER.to_owned();
    theirs.merge_sha = Some(OTHER.to_owned());
    l.github.set_pull_request(&l.repo, theirs);
    let l = restart(l);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 0);
    assert_failed_with(&l, &id, "merge_unconfirmed");
    let ledger = attempt(&l, &id);
    assert_eq!(ledger.state, MergeState::Refused);
    assert_eq!(ledger.refusal.as_deref(), Some("unconfirmed"));
}

#[test]
fn landing_a_merge_runs_with_the_method_the_card_showed() {
    // The card is stored with the project's method, so the digest binds it.
    let l = landing();
    set_method(&l, "merge");
    l.tick(now());
    let id = propose_checked(&l.api, "t1", &merge_action(&l.task, NUMBER, HEAD))[0].clone();
    let stored = proposal(&l.api, &id);
    let Ok(Action::DeliveryMerge(shown)) = serde_json::from_str::<Action>(&stored.params_json)
    else {
        panic!("a merge card must store a merge: {stored:?}");
    };
    assert_eq!(shown.method.as_deref(), Some("merge"));
    approve(&l, &id, "k-1");

    l.api.merge_pending(now());

    assert_eq!(
        l.github.merge_calls(),
        vec![Call::Merge {
            repo: l.repo.clone(),
            number: NUMBER,
            head_sha: HEAD.to_owned(),
            method: MergeMethod::Merge,
        }]
    );

    // A later change of the project's default never changes what a stored card runs: the
    // card is refused, and the host is not asked with another method.
    let l = landing();
    l.tick(now());
    let id = propose_checked(&l.api, "t1", &merge_action(&l.task, NUMBER, HEAD))[0].clone();
    set_method(&l, "rebase");
    let result = approve(&l, &id, "k-1");
    assert_eq!(result["proposal"]["state"], "failed", "{result}");
    assert_eq!(
        result["proposal"]["denial"], "method_not_allowed",
        "{result}"
    );
    l.api.merge_pending(now());
    assert_eq!(l.github.merge_count(), 0);
}

#[test]
fn landing_a_merge_for_a_failed_or_local_review_task_is_denied() {
    let l = landing();
    succeeded_task(
        &l.api,
        "task-local",
        &l.project,
        "moa/task-local",
        DeliveryPolicy::LocalReview,
    );
    failed_pr_task(&l.api, "task-failed", &l.project);
    let queued = submit_pr_task_not_finished(&l.api, &l.project);
    let tasks = ["task-local", "task-failed", queued.as_str()];
    for (index, task) in tasks.into_iter().enumerate() {
        store_pull_request(&l.api, task, &l.project, 20 + index as u64);
    }

    for (index, task) in tasks.into_iter().enumerate() {
        let number = 20 + index as u64;
        // At propose time: the card is stored failed.
        let checked = propose_checked(
            &l.api,
            &format!("turn-checked-{index}"),
            &merge_action(task, number, HEAD),
        );
        let stored = proposal(&l.api, &checked[0]);
        assert_eq!(stored.state, ProposalState::Failed, "{task}");
        assert_eq!(
            stored.denial.as_deref(),
            Some("delivery_not_authorized"),
            "{task}"
        );

        // At approve time, for a card that was stored pending anyway.
        let pending = propose(
            &l.api,
            &format!("turn-approve-{index}"),
            &merge_action(task, number, HEAD),
        )[0]
        .clone();
        let result = approve(&l, &pending, &format!("k-{index}"));
        assert_eq!(result["proposal"]["state"], "failed", "{task}: {result}");
        assert_eq!(
            result["proposal"]["denial"], "delivery_not_authorized",
            "{task}: {result}"
        );

        // At execution, for a proposal approved behind the check.
        let behind = propose(
            &l.api,
            &format!("turn-run-{index}"),
            &merge_action(task, number, OTHER),
        )[0]
        .clone();
        l.api
            .store
            .approve_action_proposal(&behind, "user")
            .unwrap();
        l.api.merge_pending(now());
        assert_failed_with(&l, &behind, "conflict");
        assert!(l.api.store.pull_request_merge(&behind).unwrap().is_none());
    }
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.calls().is_empty());
}

#[test]
fn landing_a_grant_without_the_merge_kind_cannot_approve_a_merge() {
    let l = landing();
    // A grant of another kind on the same project: the matcher never approves a merge under
    // it, and a claim made behind the matcher's back is refused before the host.
    let other = call(
        &l.api,
        "grants.create",
        json!({
            "scope": {"kind": "project", "id": project_code(&l.project)},
            "kinds": ["task.retry"],
            "token_budget": 5_000,
            "idempotency_key": "g-retry",
        }),
    )
    .unwrap()["grant"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = card(&l);

    assert_eq!(match_once(&l), 0);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Pending);

    let digest = proposal(&l.api, &id).digest;
    l.api
        .store
        .approve_action_proposal_by_grant(&id, &digest, &other, now())
        .unwrap();
    l.api.merge_pending(now());
    assert_failed_with(&l, &id, "project_not_covered");
    assert_eq!(l.github.merge_count(), 0);
}

// ----- the unattended merge grant (N8) ------------------------------------------------------

/// A merge grant on the project of `l`. `extra` adds or replaces request fields.
fn merge_grant(l: &Landing, key: &str, extra: Value) -> String {
    let mut params = json!({
        "scope": {"kind": "project", "id": project_code(&l.project)},
        "kinds": ["delivery.merge"],
        "token_budget": 1_000,
        "confirm_unattended_merge": true,
        "branch_prefix": "moa/",
        "idempotency_key": key,
    });
    for (name, value) in extra.as_object().unwrap() {
        params[name] = value.clone();
    }
    let created = call(&l.api, "grants.create", params).unwrap();
    created["grant"]["id"].as_str().unwrap().to_owned()
}

/// Proposals the matcher approved in one pass at the real time.
fn match_once(l: &Landing) -> usize {
    l.api.grant_matcher_tick(now()).approved
}

fn grant_events(l: &Landing, grant: &str) -> Vec<(GrantEventKind, Option<String>)> {
    l.api
        .store
        .grant_events(grant, 0, 100)
        .unwrap()
        .into_iter()
        .map(|event| (event.event, event.code))
        .collect()
}

fn uses(l: &Landing, grant: &str) -> u32 {
    l.api
        .store
        .standing_grant(grant)
        .unwrap()
        .unwrap()
        .uses_used
}

fn check_of(name: &str, state: CheckState) -> CheckRun {
    CheckRun {
        name: name.to_owned(),
        state,
    }
}

#[test]
fn merge_grant_merges_once_on_a_green_bound_head_with_a_grant_actor() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);

    assert_eq!(match_once(&l), 1);
    // Approved by the grant, and not merged yet: the landing thread runs it.
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    assert_eq!(l.github.merge_count(), 0);
    assert_eq!(l.api.merge_pending(now()), 1);

    assert_eq!(
        l.github.merge_calls(),
        vec![Call::Merge {
            repo: l.repo.clone(),
            number: NUMBER,
            head_sha: HEAD.to_owned(),
            method: MergeMethod::Squash,
        }]
    );
    let done = proposal(&l.api, &id);
    assert_eq!(done.state, ProposalState::Executed, "{done:?}");
    assert_eq!(done.result_ref.as_deref(), Some(SHORT));
    let actor = format!("grant:{grant}");
    let transitions: Vec<(ProposalState, String)> = l
        .api
        .store
        .action_proposal_events(&id)
        .unwrap()
        .into_iter()
        .skip(1)
        .map(|event| (event.to_state, event.actor))
        .collect();
    assert_eq!(
        transitions,
        [
            (ProposalState::Approved, actor.clone()),
            (ProposalState::Executed, actor)
        ]
    );
    assert_eq!(
        grant_events(&l, &grant),
        [
            (GrantEventKind::Created, None),
            (GrantEventKind::Used, None),
            (GrantEventKind::Executed, None)
        ]
    );
    assert_eq!(uses(&l, &grant), 1);
    assert_eq!(attempt(&l, &id).state, MergeState::Merged);
    // Another pass finds nothing to do.
    l.api.merge_pending(now() + 1_000);
    assert_eq!(l.github.merge_count(), 1);
}

#[test]
fn merge_grant_each_refusal_ends_the_card_failed_and_stops_the_grant() {
    type Setup = fn(&Landing);
    let cases: [(&str, Value, Setup); 6] = [
        ("stale_head", json!({}), |l| {
            l.github.set_head(&l.repo, NUMBER, OTHER);
        }),
        ("checks_not_green", json!({}), |l| {
            l.github.set_checks(
                &l.repo,
                NUMBER,
                vec![
                    check_of("ci / build", CheckState::Passed),
                    check_of("ci / test", CheckState::Pending),
                ],
            );
        }),
        ("checks_not_green", json!({}), |l| {
            l.github.set_checks(
                &l.repo,
                NUMBER,
                vec![check_of("ci / build", CheckState::Failed)],
            );
        }),
        // No required check configured counts as missing, never as green.
        ("checks_not_green", json!({}), |l| {
            l.github.set_checks(&l.repo, NUMBER, Vec::new());
        }),
        (
            "branch_not_covered",
            json!({"branch_prefix": "release/"}),
            |_| {},
        ),
        ("own_upstream", json!({}), |_| {}),
    ];
    for (index, (code, extra, setup)) in cases.into_iter().enumerate() {
        let l = landing();
        if code == "own_upstream" {
            let own = RepoRef::parse("acme", "motif-mcao").unwrap();
            l.github.set_repo(own.clone());
            l.github.set_pull_request(&own, view(NUMBER, BRANCH));
        }
        let grant = merge_grant(&l, "g-1", extra);
        let id = card(&l);
        assert_eq!(match_once(&l), 1, "{code} {index}");
        setup(&l);

        l.api.merge_pending(now());

        assert_failed_with(&l, &id, code);
        assert_eq!(l.github.merge_count(), 0, "{code} {index}");
        assert_eq!(attempt(&l, &id).state, MergeState::Refused);
        assert_eq!(attempt(&l, &id).refusal.as_deref(), Some(code));
        assert_eq!(
            grant_events(&l, &grant),
            [
                (GrantEventKind::Created, None),
                (GrantEventKind::Used, None),
                (GrantEventKind::Stopped, Some(code.to_owned()))
            ],
            "{code} {index}"
        );
        let own_repo = RepoRef::parse("acme", "motif-mcao").unwrap();
        let open = |repo: &RepoRef| l.github.pull_request(repo, NUMBER).map(|view| view.state);
        assert_eq!(
            open(if code == "own_upstream" {
                &own_repo
            } else {
                &l.repo
            }),
            Some(orchestrator_core::landing::PrState::Open)
        );
    }
}

#[test]
fn merge_grant_refusals_run_in_the_documented_order() {
    // A moved head wins over red checks, red checks over a branch outside the prefix, and the
    // prefix over the repository.
    let l = landing();
    let own = RepoRef::parse("acme", "motif-mcao").unwrap();
    l.github.set_repo(own.clone());
    l.github.set_pull_request(&own, view(NUMBER, BRANCH));
    let grant = merge_grant(&l, "g-1", json!({"branch_prefix": "release/"}));
    let id = card(&l);
    match_once(&l);
    l.github.set_head(&own, NUMBER, OTHER);
    l.github.set_checks(&own, NUMBER, Vec::new());
    l.api.merge_pending(now());
    assert_failed_with(&l, &id, "stale_head");

    l.github.set_head(&own, NUMBER, HEAD);
    let second = propose(&l.api, "t2", &merge_action(&l.task, NUMBER, HEAD))[0].clone();
    match_once(&l);
    l.api.merge_pending(now());
    assert_failed_with(&l, &second, "checks_not_green");

    l.github.set_checks(
        &own,
        NUMBER,
        vec![check_of("ci / build", CheckState::Passed)],
    );
    let third = propose(&l.api, "t3", &merge_action(&l.task, NUMBER, HEAD))[0].clone();
    match_once(&l);
    l.api.merge_pending(now());
    assert_failed_with(&l, &third, "branch_not_covered");
    assert_eq!(l.github.merge_count(), 0);
    assert_eq!(uses(&l, &grant), 3);
}

#[test]
fn merge_grant_a_head_that_moves_between_the_check_and_the_merge_never_merges() {
    let l = landing();
    merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    match_once(&l);
    l.github.move_head_after_next_view(&l.repo, NUMBER, OTHER);

    l.api.merge_pending(now());

    // The host was asked with the bound commit and refused: nothing merged.
    assert_failed_with(&l, &id, "head_moved");
    assert_eq!(
        l.github.pull_request(&l.repo, NUMBER).unwrap().state,
        orchestrator_core::landing::PrState::Open
    );
    assert_eq!(attempt(&l, &id).state, MergeState::Refused);
    for call in l.github.merge_calls() {
        assert!(matches!(call, Call::Merge { head_sha, .. } if head_sha == HEAD));
    }
}

#[test]
fn merge_grant_never_runs_a_proposal_from_an_auto_turn() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    l.tick(now());
    let ids = propose_in(
        &l.api,
        &ConversationTarget::Global,
        "t1",
        PermissionLevel::Auto,
        &merge_action(&l.task, NUMBER, HEAD),
    );

    assert_eq!(match_once(&l), 0);
    assert_eq!(proposal(&l.api, &ids[0]).state, ProposalState::Pending);
    assert_eq!(uses(&l, &grant), 0);

    // Claimed behind the matcher's back, the worker still refuses a turn that was not Restricted.
    let digest = proposal(&l.api, &ids[0]).digest;
    l.api
        .store
        .approve_action_proposal_by_grant(&ids[0], &digest, &grant, now())
        .unwrap();
    let before = l.calls().len();
    l.api.merge_pending(now());

    assert_failed_with(&l, &ids[0], "invalid");
    assert_eq!(l.calls().len(), before);
    assert_eq!(l.github.merge_count(), 0);
}

#[test]
fn merge_grant_never_merges_in_a_local_only_project() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    set_mode(&l.api, &l.project, "local-only");
    let before = l.calls().len();

    l.api.merge_pending(now());

    assert_failed_with(&l, &id, "project_mode");
    assert_eq!(l.calls().len(), before, "the host is not asked");
    assert_eq!(l.github.merge_count(), 0);
    assert_eq!(
        grant_events(&l, &grant).last(),
        Some(&(GrantEventKind::Stopped, Some("project_mode".to_owned())))
    );
}

#[test]
fn merge_grant_a_revoked_grant_merges_nothing_before_the_check() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let waiting = card(&l);
    call(
        &l.api,
        "grants.revoke",
        json!({"grant_id": grant, "idempotency_key": "r-1"}),
    )
    .unwrap();

    // Revoked before the matcher: the card stays for the user.
    assert_eq!(match_once(&l), 0);
    assert_eq!(proposal(&l.api, &waiting).state, ProposalState::Pending);

    // Revoked after the approval and before the merge: the card fails and the host is not asked.
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    let revoked = call(
        &l.api,
        "grants.revoke",
        json!({"grant_id": grant, "idempotency_key": "r-1"}),
    )
    .unwrap();
    assert_eq!(revoked["stopped"], 1);
    l.api.merge_pending(now());
    assert_failed_with(&l, &id, "grant_revoked");
    assert_eq!(l.github.merge_count(), 0);
}

#[test]
fn merge_grant_an_expired_grant_merges_nothing_before_the_check() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({"expires_in_secs": 3_600}));
    let first = card(&l);
    let later = now() + 3_700_000;

    // Expired before the matcher: nothing is approved.
    assert_eq!(l.api.grant_matcher_tick(later).approved, 0);
    assert_eq!(proposal(&l.api, &first).state, ProposalState::Pending);

    // Expired between the approval and the merge: the worker stops it before the host.
    assert_eq!(match_once(&l), 1);
    l.api.merge_pending(later);
    assert_failed_with(&l, &first, "grant_expired");
    assert_eq!(l.github.merge_count(), 0);
    assert_eq!(
        grant_events(&l, &grant).last(),
        Some(&(GrantEventKind::Stopped, Some("grant_expired".to_owned())))
    );
}

#[test]
fn merge_grant_a_used_up_grant_leaves_the_next_card_for_the_user() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({"max_uses": 1}));
    let first = card(&l);
    assert_eq!(match_once(&l), 1);
    let second = propose(&l.api, "t2", &merge_action(&l.task, NUMBER, HEAD))[0].clone();

    assert_eq!(match_once(&l), 0);
    l.api.merge_pending(now());

    assert_eq!(proposal(&l.api, &first).state, ProposalState::Executed);
    assert_eq!(proposal(&l.api, &second).state, ProposalState::Pending);
    assert_eq!(uses(&l, &grant), 1);
    assert_eq!(l.github.merge_count(), 1);
}

#[test]
fn merge_grant_the_same_proposal_taken_twice_in_turn_merges_once() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    let digest = proposal(&l.api, &id).digest;

    // The second claim finds the proposal no longer pending and spends nothing.
    let second = l
        .api
        .store
        .approve_action_proposal_by_grant(&id, &digest, &grant, now());
    assert!(matches!(
        second,
        Err(GrantClaimError::Proposal(ResolveError::NotPending(
            ProposalState::Approved
        )))
    ));
    assert_eq!(match_once(&l), 0);
    let approved = proposal(&l.api, &id);
    let policy = l.api.merge_policy(&approved).unwrap();
    l.api.merge_inner(&approved, &policy, now()).unwrap();
    l.api.merge_inner(&approved, &policy, now()).unwrap();
    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_eq!(uses(&l, &grant), 1);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
}

#[test]
fn merge_grant_a_restart_between_the_claim_and_the_host_call_resolves_to_one_merge() {
    // The claim is in the ledger and the host was never asked: the restart runs it once.
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    claim(&l, &id);
    let l = restart(l);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Executed);
    assert_eq!(
        grant_events(&l, &grant).last(),
        Some(&(GrantEventKind::Executed, None))
    );

    // The attempt was `issued` and the daemon stopped inside the call: a read settles it and
    // the host is never asked again.
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    claim(&l, &id);
    l.api
        .store
        .advance_pull_request_merge(&id, MergeState::Issued, None, None)
        .unwrap();
    let l = restart(l);

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 0);
    assert_failed_with(&l, &id, "merge_unconfirmed");
    assert_eq!(
        grant_events(&l, &grant).last(),
        Some(&(
            GrantEventKind::Stopped,
            Some("merge_unconfirmed".to_owned())
        ))
    );
}

#[test]
fn merge_grant_a_grant_of_another_project_neither_approves_nor_runs_a_merge() {
    let l = landing();
    register_project(&l.api, ALPHA);
    let other = call(
        &l.api,
        "grants.create",
        json!({
            "scope": {"kind": "project", "id": project_code(ALPHA)},
            "kinds": ["delivery.merge"],
            "token_budget": 1_000,
            "confirm_unattended_merge": true,
            "branch_prefix": "moa/",
            "idempotency_key": "g-other",
        }),
    )
    .unwrap()["grant"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = card(&l);

    assert_eq!(match_once(&l), 0);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Pending);
    assert_eq!(uses(&l, &other), 0);

    // Claimed behind the matcher's back, the worker refuses it before the host.
    let digest = proposal(&l.api, &id).digest;
    l.api
        .store
        .approve_action_proposal_by_grant(&id, &digest, &other, now())
        .unwrap();
    l.api.merge_pending(now());
    assert_failed_with(&l, &id, "project_not_covered");
    assert_eq!(l.github.merge_count(), 0);
}

#[test]
fn merge_grant_ignores_a_proposal_older_than_the_grant() {
    let l = landing();
    let old = card(&l);
    let grant = merge_grant(&l, "g-1", json!({}));

    assert_eq!(match_once(&l), 0);

    assert_eq!(proposal(&l.api, &old).state, ProposalState::Pending);
    assert_eq!(uses(&l, &grant), 0);
}

#[test]
fn merge_grant_a_user_approval_still_runs_with_the_per_action_policy() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    // The user got there first.
    approve(&l, &id, "k-1");

    assert_eq!(match_once(&l), 0);
    let approved = proposal(&l.api, &id);
    assert!(matches!(
        l.api.merge_policy(&approved).unwrap(),
        MergePolicy::PerAction
    ));
    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 1);
    assert_eq!(uses(&l, &grant), 0);
    assert_eq!(
        grant_events(&l, &grant),
        [(GrantEventKind::Created, None)],
        "a user approval writes no grant event"
    );
}

#[test]
fn merge_grant_a_cross_repository_pull_request_never_merges() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    // The host now reports the head in a fork, as a read of the same number could.
    let mut forked = view(NUMBER, BRANCH);
    forked.same_repository = false;
    l.github.set_pull_request(&l.repo, forked);

    l.api.merge_pending(now());

    assert_failed_with(&l, &id, "cross_repo");
    assert_eq!(l.github.merge_count(), 0);
    assert_eq!(attempt(&l, &id).state, MergeState::Refused);
    assert_eq!(
        grant_events(&l, &grant).last(),
        Some(&(GrantEventKind::Stopped, Some("cross_repo".to_owned())))
    );
}

#[test]
fn merge_grant_a_revoke_between_the_check_and_the_host_call_stops_the_merge() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    let host = Arc::new(RevokingHost {
        inner: l.github.clone(),
        hook: std::sync::Mutex::new(None),
    });
    let l = restart_over(l, host.clone() as Arc<dyn GitHub>);
    let store = l.api.store.clone();
    let revoked = grant.clone();
    // Runs after the grant was checked and the pull request read, before the claim is issued.
    *host.hook.lock().unwrap() = Some(Box::new(move || {
        store
            .revoke_standing_grant_and_stop(&revoked, now())
            .unwrap();
    }));

    l.api.merge_pending(now());

    assert_eq!(l.github.merge_count(), 0);
    assert_failed_with(&l, &id, "grant_revoked");
    assert_eq!(attempt(&l, &id).state, MergeState::Refused);
    let stopped: Vec<_> = grant_events(&l, &grant)
        .into_iter()
        .filter(|(event, _)| *event == GrantEventKind::Stopped)
        .collect();
    assert_eq!(
        stopped,
        [(GrantEventKind::Stopped, Some("grant_revoked".to_owned()))]
    );
}

#[test]
fn merge_grant_the_own_upstream_pair_is_refused_under_any_name() {
    // The compiled-in pair, the same pair in other letter case, and the name under any owner.
    let pairs = [
        ("mc-innersource", "motif-mcao"),
        ("MC-Innersource", "Motif-MCAO"),
        ("someone-else", "motif-mcao"),
    ];
    for (owner, name) in pairs {
        let l = landing();
        let own = RepoRef::parse(owner, name).unwrap();
        l.github.set_repo(own.clone());
        l.github.set_pull_request(&own, view(NUMBER, BRANCH));
        merge_grant(&l, "g-1", json!({}));
        let id = card(&l);
        assert_eq!(match_once(&l), 1, "{owner}/{name}");

        l.api.merge_pending(now());

        assert_failed_with(&l, &id, "own_upstream");
        assert_eq!(l.github.merge_count(), 0, "{owner}/{name}");
    }
}

#[test]
fn merge_grant_a_repository_other_than_the_one_the_project_tracks_is_refused() {
    let l = landing();
    merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    // The poller bound the pull request to the repository it read it from.
    assert_eq!(
        l.api.store.pull_request_repo(&l.task).unwrap(),
        Some(("acme".to_owned(), "widgets".to_owned()))
    );
    // Then the remote now names a mirror under another name, which holds the same number.
    let mirror = RepoRef::parse("acme", "mirror").unwrap();
    l.github.set_repo(mirror.clone());
    l.github.set_pull_request(&mirror, view(NUMBER, BRANCH));

    l.api.merge_pending(now());

    assert_failed_with(&l, &id, "repo_not_tracked");
    assert_eq!(l.github.merge_count(), 0);
    // The first binding stays: a later read of the mirror never moves it.
    l.api
        .store
        .bind_pull_request_repo(&l.task, mirror.owner(), mirror.name())
        .unwrap();
    assert_eq!(
        l.api.store.pull_request_repo(&l.task).unwrap(),
        Some(("acme".to_owned(), "widgets".to_owned()))
    );
}

#[test]
fn merge_grant_a_failed_or_local_review_task_never_merges_unattended() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    succeeded_task(
        &l.api,
        "task-local",
        &l.project,
        "moa/task-local",
        DeliveryPolicy::LocalReview,
    );
    failed_pr_task(&l.api, "task-failed", &l.project);
    for (index, (task, number)) in [("task-local", 20), ("task-failed", 21)]
        .into_iter()
        .enumerate()
    {
        store_pull_request(&l.api, task, &l.project, number);
        // Claimed behind the matcher's back: the worker refuses it before the host.
        let behind = propose(
            &l.api,
            &format!("tb-{index}"),
            &merge_action(task, number, OTHER),
        )[0]
        .clone();
        let digest = proposal(&l.api, &behind).digest;
        l.api
            .store
            .approve_action_proposal_by_grant(&behind, &digest, &grant, now())
            .unwrap();
        l.api.merge_pending(now());
        assert_failed_with(&l, &behind, "conflict");

        // Through the matcher: approved or denied, never merged.
        let matched = propose(
            &l.api,
            &format!("tm-{index}"),
            &merge_action(task, number, HEAD),
        )[0]
        .clone();
        match_once(&l);
        l.api.merge_pending(now());
        assert_eq!(
            proposal(&l.api, &matched).state,
            ProposalState::Failed,
            "{task}"
        );
        assert!(l.api.store.pull_request_merge(&matched).unwrap().is_none());
    }
    assert_eq!(l.github.merge_count(), 0);
}

#[test]
fn merge_grant_the_audit_rows_name_the_head_commit() {
    // A merge that ran: the use and its close both name the bound head.
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    card(&l);
    assert_eq!(match_once(&l), 1);
    l.api.merge_pending(now());
    let heads = |l: &Landing, grant: &str| -> Vec<(GrantEventKind, Option<String>)> {
        l.api
            .store
            .grant_events(grant, 0, 100)
            .unwrap()
            .into_iter()
            .map(|event| (event.event, event.head_sha))
            .collect()
    };
    assert_eq!(
        heads(&l, &grant),
        [
            (GrantEventKind::Created, None),
            (GrantEventKind::Used, Some(HEAD.to_owned())),
            (GrantEventKind::Executed, Some(HEAD.to_owned()))
        ]
    );

    // A merge that stopped: the close names the head the card bound, not the moved one.
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    card(&l);
    assert_eq!(match_once(&l), 1);
    l.github.set_head(&l.repo, NUMBER, OTHER);
    l.api.merge_pending(now());
    assert_eq!(
        heads(&l, &grant),
        [
            (GrantEventKind::Created, None),
            (GrantEventKind::Used, Some(HEAD.to_owned())),
            (GrantEventKind::Stopped, Some(HEAD.to_owned()))
        ]
    );
}

#[test]
fn merge_grant_a_merge_the_host_does_not_confirm_is_not_recorded_as_merged() {
    let l = landing();
    let grant = merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    let host = Arc::new(QueueingHost {
        inner: l.github.clone(),
        merges: AtomicUsize::new(0),
    });
    let l = restart_over(l, host.clone() as Arc<dyn GitHub>);

    l.api.merge_pending(now());

    // The call went through and the pull request is still open: not a merge, not executed.
    assert_eq!(host.merges.load(Ordering::SeqCst), 1);
    assert_eq!(attempt(&l, &id).state, MergeState::Unknown);
    assert_eq!(proposal(&l.api, &id).state, ProposalState::Approved);
    assert_eq!(
        grant_events(&l, &grant),
        [
            (GrantEventKind::Created, None),
            (GrantEventKind::Used, None)
        ]
    );
}

#[test]
fn merge_grant_two_callers_on_one_proposal_merge_once() {
    let l = landing();
    merge_grant(&l, "g-1", json!({}));
    let id = card(&l);
    assert_eq!(match_once(&l), 1);
    let approved = proposal(&l.api, &id);
    let barrier = std::sync::Barrier::new(2);

    // The final state is not asserted: what holds for any interleaving is the number of
    // host calls, because only the caller that moved the claim to issued calls the host.
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let policy = l.api.merge_policy(&approved).unwrap();
                barrier.wait();
                let _ = l.api.merge_inner(&approved, &policy, now());
            });
        }
    });

    assert_eq!(l.github.merge_count(), 1);
    assert!(!l.api.store.issue_pull_request_merge(&id).unwrap());
}
