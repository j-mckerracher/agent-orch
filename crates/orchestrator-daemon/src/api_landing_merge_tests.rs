//! Merging a pull request through an approved `delivery.merge` card (DPES-1105 phase 4c).
//! Every test drives an in-process `DaemonApi` over a real store with `FakeGitHub`: no network,
//! no `gh`, no agent. The merge runs when a test calls `merge_pending`, as the landing thread
//! does, so the call log of the double shows exactly what reached the host. A restart is a new
//! `DaemonApi` over the same data directory and the same double.

use orchestrator_core::landing::MergeMethod;
use orchestrator_core::orchestrator_action::{parse_block, Action, BlockScope};
use orchestrator_git::github::{GitHub, GitHubError};
use orchestrator_git::github_fake::{Call, Method};
use orchestrator_store::{
    ConversationTarget, MergeClaim, MergeState, ProposalState, TurnBlock, TurnPermission,
};

use super::actions_tests::{call, proposal, propose, propose_in, resolve};
use super::landing::{lock, SETTLE_INTERVAL_MS};
use super::landing_api::merge_denial;
use super::landing_tests::{
    landing, merge_action, now, set_mode, Landing, BRANCH, HEAD, NUMBER, OTHER,
};
use super::*;

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
    let block = parse_block(&held, BlockScope::Global)
        .unwrap()
        .checked_with(|action| match action {
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
    let api = DaemonApi::with_github(data.path(), github.clone() as Arc<dyn GitHub>).unwrap();
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
fn landing_checks_that_turn_red_after_the_read_are_refused_by_the_host() {
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
    l.api.merge_inner(&approved, now()).unwrap();
    l.api.merge_inner(&approved, now()).unwrap();
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
fn landing_a_grant_cannot_approve_a_merge() {
    let l = landing();
    let id = card(&l);
    l.api
        .store
        .approve_action_proposal(&id, "grant:g-1")
        .unwrap();

    l.api.merge_pending(now());

    assert_failed_with(&l, &id, "forbidden");
    assert_eq!(l.github.merge_count(), 0);
    assert!(l.api.store.pull_request_merge(&id).unwrap().is_none());
    assert!(!orchestrator_core::orchestrator_action::ActionKind::DeliveryMerge.is_grantable());
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
