//! A scriptable in-memory [`GitHub`] for tests (DPES-1105, phase 4c).
//!
//! `pub` and not behind `cfg(test)`, because the daemon's tests need it. It touches no
//! network and runs no process. It records every call, applies the rule the host applies on
//! a merge (the bound head must still be the head, the pull request must be open and
//! mergeable, every required check must have passed), and lets a test script failures, a
//! head move between a check and a merge, checks that turn red, and a merge that lands while
//! its reply is lost.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};

use orchestrator_core::landing::{CheckState, MergeMethod, PrState};

use crate::github::{CheckRun, GitHub, GitHubError, PullRequestView, RepoRef};
use crate::Repository;

/// The four trait methods, for scripting a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    RepoOf,
    FindByBranch,
    View,
    Merge,
}

/// One recorded call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    RepoOf,
    FindByBranch {
        repo: RepoRef,
        branch: String,
    },
    View {
        repo: RepoRef,
        number: u64,
    },
    Merge {
        repo: RepoRef,
        number: u64,
        head_sha: String,
        method: MergeMethod,
    },
}

impl Call {
    pub fn method(&self) -> Method {
        match self {
            Self::RepoOf => Method::RepoOf,
            Self::FindByBranch { .. } => Method::FindByBranch,
            Self::View { .. } => Method::View,
            Self::Merge { .. } => Method::Merge,
        }
    }
}

/// Something that happens right after the next `view` returns.
enum AfterView {
    MoveHead {
        repo: RepoRef,
        number: u64,
        head_sha: String,
    },
    TurnChecksRed {
        repo: RepoRef,
        number: u64,
    },
}

#[derive(Default)]
struct FakeState {
    repo: Option<RepoRef>,
    pulls: BTreeMap<(RepoRef, u64), PullRequestView>,
    calls: Vec<Call>,
    failures: Vec<(Method, VecDeque<GitHubError>)>,
    after_next_view: Vec<AfterView>,
    lose_next_merge_reply: bool,
}

impl FakeState {
    fn take_failure(&mut self, method: Method) -> Option<GitHubError> {
        self.failures
            .iter_mut()
            .find(|(scripted, _)| *scripted == method)
            .and_then(|(_, queue)| queue.pop_front())
    }
}

/// See the module documentation. Every method takes `&self`; state sits behind one mutex.
#[derive(Default)]
pub struct FakeGitHub {
    state: Mutex<FakeState>,
}

impl FakeGitHub {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What `repo_of` answers. Unset, it fails with `NotFound`.
    pub fn set_repo(&self, repo: RepoRef) {
        self.lock().repo = Some(repo);
    }

    /// Adds or replaces a pull request.
    pub fn set_pull_request(&self, repo: &RepoRef, view: PullRequestView) {
        self.lock().pulls.insert((repo.clone(), view.number), view);
    }

    /// Moves the head of a pull request now.
    pub fn set_head(&self, repo: &RepoRef, number: u64, head_sha: &str) {
        if let Some(view) = self.lock().pulls.get_mut(&(repo.clone(), number)) {
            view.head_sha = head_sha.to_owned();
        }
    }

    pub fn set_checks(&self, repo: &RepoRef, number: u64, checks: Vec<CheckRun>) {
        if let Some(view) = self.lock().pulls.get_mut(&(repo.clone(), number)) {
            view.checks = checks;
        }
    }

    /// The next call of `method` fails with `error`; later calls are not affected. Failures
    /// of one method run in the order they were scripted.
    pub fn fail_next(&self, method: Method, error: GitHubError) {
        let mut state = self.lock();
        match state.failures.iter_mut().find(|(m, _)| *m == method) {
            Some((_, queue)) => queue.push_back(error),
            None => state.failures.push((method, VecDeque::from([error]))),
        }
    }

    /// After the next `view` returns, the head becomes `head_sha`: the head moves between
    /// the check and the merge.
    pub fn move_head_after_next_view(&self, repo: &RepoRef, number: u64, head_sha: &str) {
        self.lock().after_next_view.push(AfterView::MoveHead {
            repo: repo.clone(),
            number,
            head_sha: head_sha.to_owned(),
        });
    }

    /// After the next `view` returns, every check of the pull request is failed.
    pub fn turn_checks_red_after_next_view(&self, repo: &RepoRef, number: u64) {
        self.lock().after_next_view.push(AfterView::TurnChecksRed {
            repo: repo.clone(),
            number,
        });
    }

    /// The next `merge` that passes the host's rules lands, and then returns `Timeout`:
    /// the double-merge scenario. The pull request reads as merged afterwards.
    pub fn lose_next_merge_reply(&self) {
        self.lock().lose_next_merge_reply = true;
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        self.lock().calls.clone()
    }

    /// The `merge` calls so far, in order.
    pub fn merge_calls(&self) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.method() == Method::Merge)
            .collect()
    }

    /// How many `merge` calls were made, whether or not they succeeded.
    pub fn merge_count(&self) -> usize {
        self.merge_calls().len()
    }

    /// The stored pull request, without recording a call.
    pub fn pull_request(&self, repo: &RepoRef, number: u64) -> Option<PullRequestView> {
        self.lock().pulls.get(&(repo.clone(), number)).cloned()
    }
}

impl GitHub for FakeGitHub {
    fn repo_of(&self, _repository: &Repository) -> Result<RepoRef, GitHubError> {
        let mut state = self.lock();
        state.calls.push(Call::RepoOf);
        if let Some(error) = state.take_failure(Method::RepoOf) {
            return Err(error);
        }
        state.repo.clone().ok_or(GitHubError::NotFound)
    }

    fn find_by_branch(&self, repo: &RepoRef, branch: &str) -> Result<Option<u64>, GitHubError> {
        let mut state = self.lock();
        state.calls.push(Call::FindByBranch {
            repo: repo.clone(),
            branch: branch.to_owned(),
        });
        if let Some(error) = state.take_failure(Method::FindByBranch) {
            return Err(error);
        }
        // The open pull request with the highest number, else the highest of any state.
        let found = state
            .pulls
            .iter()
            .filter(|((r, _), view)| r == repo && view.head_branch == branch)
            .map(|((_, number), view)| (view.state == PrState::Open, *number))
            .max();
        Ok(found.map(|(_, number)| number))
    }

    fn view(&self, repo: &RepoRef, number: u64) -> Result<PullRequestView, GitHubError> {
        let mut state = self.lock();
        state.calls.push(Call::View {
            repo: repo.clone(),
            number,
        });
        if let Some(error) = state.take_failure(Method::View) {
            return Err(error);
        }
        let seen = state
            .pulls
            .get(&(repo.clone(), number))
            .cloned()
            .ok_or(GitHubError::NotFound)?;
        for step in std::mem::take(&mut state.after_next_view) {
            match step {
                AfterView::MoveHead {
                    repo,
                    number,
                    head_sha,
                } => {
                    if let Some(view) = state.pulls.get_mut(&(repo, number)) {
                        view.head_sha = head_sha;
                    }
                }
                AfterView::TurnChecksRed { repo, number } => {
                    if let Some(view) = state.pulls.get_mut(&(repo, number)) {
                        for check in &mut view.checks {
                            check.state = CheckState::Failed;
                        }
                    }
                }
            }
        }
        Ok(seen)
    }

    fn merge(
        &self,
        repo: &RepoRef,
        number: u64,
        head_sha: &str,
        method: MergeMethod,
    ) -> Result<(), GitHubError> {
        let mut state = self.lock();
        state.calls.push(Call::Merge {
            repo: repo.clone(),
            number,
            head_sha: head_sha.to_owned(),
            method,
        });
        if let Some(error) = state.take_failure(Method::Merge) {
            return Err(error);
        }
        let lose_reply = state.lose_next_merge_reply;
        let view = state
            .pulls
            .get_mut(&(repo.clone(), number))
            .ok_or(GitHubError::NotFound)?;
        // The rules the host applies.
        if view.head_sha != head_sha {
            return Err(GitHubError::HeadMoved);
        }
        let all_green = !view.checks.is_empty()
            && view
                .checks
                .iter()
                .all(|check| check.state == CheckState::Passed);
        if view.state != PrState::Open || view.mergeable != Some(true) || !all_green {
            return Err(GitHubError::NotMergeable);
        }
        view.state = PrState::Merged;
        view.merge_sha = Some(format!("{:040x}", 0xfeed_0000_u64 + number));
        if lose_reply {
            state.lose_next_merge_reply = false;
            return Err(GitHubError::Timeout);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
    const MOVED: &str = "fedcba9876543210fedcba9876543210fedcba98";

    fn repo() -> RepoRef {
        RepoRef::parse("octo", "demo").unwrap()
    }

    fn view(number: u64) -> PullRequestView {
        PullRequestView {
            number,
            state: PrState::Open,
            head_branch: "moa/fix".to_owned(),
            base_branch: "main".to_owned(),
            head_sha: HEAD.to_owned(),
            mergeable: Some(true),
            title: "Fix it".to_owned(),
            checks: vec![CheckRun {
                name: "build".to_owned(),
                state: CheckState::Passed,
            }],
            merge_sha: None,
        }
    }

    fn fake() -> FakeGitHub {
        let fake = FakeGitHub::new();
        fake.set_pull_request(&repo(), view(7));
        fake
    }

    #[test]
    fn fake_github_records_every_call_in_order() {
        let fake = fake();
        fake.set_repo(repo());
        let _ = fake.find_by_branch(&repo(), "moa/fix");
        let _ = fake.view(&repo(), 7);
        let _ = fake.merge(&repo(), 7, HEAD, MergeMethod::Squash);
        assert_eq!(
            fake.calls(),
            [
                Call::FindByBranch {
                    repo: repo(),
                    branch: "moa/fix".to_owned()
                },
                Call::View {
                    repo: repo(),
                    number: 7
                },
                Call::Merge {
                    repo: repo(),
                    number: 7,
                    head_sha: HEAD.to_owned(),
                    method: MergeMethod::Squash
                },
            ]
        );
        assert_eq!(fake.merge_count(), 1);
    }

    #[test]
    fn fake_github_finds_the_open_pull_request_of_a_branch() {
        let fake = fake();
        let mut closed = view(9);
        closed.state = PrState::Closed;
        fake.set_pull_request(&repo(), closed);
        assert_eq!(fake.find_by_branch(&repo(), "moa/fix"), Ok(Some(7)));
        assert_eq!(fake.find_by_branch(&repo(), "moa/other"), Ok(None));
    }

    #[test]
    fn fake_github_merges_a_green_pull_request_on_the_bound_head() {
        let fake = fake();
        assert_eq!(fake.merge(&repo(), 7, HEAD, MergeMethod::Squash), Ok(()));
        let merged = fake.pull_request(&repo(), 7).unwrap();
        assert_eq!(merged.state, PrState::Merged);
        assert!(merged.merge_sha.is_some_and(|sha| sha.len() == 40));
        // A second merge of a merged pull request is refused by the host rule.
        assert_eq!(
            fake.merge(&repo(), 7, HEAD, MergeMethod::Squash),
            Err(GitHubError::NotMergeable)
        );
    }

    #[test]
    fn fake_github_refuses_a_merge_on_a_stale_head_or_red_checks() {
        let fake = fake();
        assert_eq!(
            fake.merge(&repo(), 7, MOVED, MergeMethod::Squash),
            Err(GitHubError::HeadMoved)
        );
        fake.set_checks(
            &repo(),
            7,
            vec![CheckRun {
                name: "build".to_owned(),
                state: CheckState::Pending,
            }],
        );
        assert_eq!(
            fake.merge(&repo(), 7, HEAD, MergeMethod::Squash),
            Err(GitHubError::NotMergeable)
        );
        fake.set_checks(&repo(), 7, Vec::new());
        assert_eq!(
            fake.merge(&repo(), 7, HEAD, MergeMethod::Squash),
            Err(GitHubError::NotMergeable)
        );
    }

    #[test]
    fn fake_github_head_moves_between_the_view_and_the_merge() {
        let fake = fake();
        fake.move_head_after_next_view(&repo(), 7, MOVED);
        let seen = fake.view(&repo(), 7).unwrap();
        assert_eq!(seen.head_sha, HEAD);
        assert_eq!(
            fake.merge(&repo(), 7, &seen.head_sha, MergeMethod::Squash),
            Err(GitHubError::HeadMoved)
        );
        assert_eq!(fake.view(&repo(), 7).unwrap().head_sha, MOVED);
    }

    #[test]
    fn fake_github_checks_turn_red_after_the_view() {
        let fake = fake();
        fake.turn_checks_red_after_next_view(&repo(), 7);
        let seen = fake.view(&repo(), 7).unwrap();
        assert_eq!(seen.checks[0].state, CheckState::Passed);
        assert_eq!(
            fake.merge(&repo(), 7, HEAD, MergeMethod::Squash),
            Err(GitHubError::NotMergeable)
        );
    }

    #[test]
    fn fake_github_a_lost_merge_reply_still_merges_once() {
        let fake = fake();
        fake.lose_next_merge_reply();
        assert_eq!(
            fake.merge(&repo(), 7, HEAD, MergeMethod::Squash),
            Err(GitHubError::Timeout)
        );
        assert_eq!(fake.view(&repo(), 7).unwrap().state, PrState::Merged);
        assert_eq!(fake.merge_count(), 1);
    }

    #[test]
    fn fake_github_scripted_failures_apply_once_in_order_per_method() {
        let fake = fake();
        fake.fail_next(Method::View, GitHubError::RateLimited);
        fake.fail_next(Method::View, GitHubError::Timeout);
        assert_eq!(fake.view(&repo(), 7), Err(GitHubError::RateLimited));
        assert_eq!(fake.view(&repo(), 7), Err(GitHubError::Timeout));
        assert!(fake.view(&repo(), 7).is_ok());
        fake.fail_next(Method::Merge, GitHubError::Failed("gh_failed"));
        assert_eq!(
            fake.merge(&repo(), 7, HEAD, MergeMethod::Squash),
            Err(GitHubError::Failed("gh_failed"))
        );
        // A failed merge changed nothing and was still counted.
        assert_eq!(fake.pull_request(&repo(), 7).unwrap().state, PrState::Open);
        assert_eq!(fake.merge_count(), 1);
    }

    #[test]
    fn fake_github_unknown_pull_request_is_not_found() {
        let fake = FakeGitHub::new();
        assert_eq!(fake.view(&repo(), 1), Err(GitHubError::NotFound));
        assert_eq!(
            fake.merge(&repo(), 1, HEAD, MergeMethod::Squash),
            Err(GitHubError::NotFound)
        );
    }
}
