//! The pull request poller and the landing thread (DPES-1105 phase 4c).
//!
//! One thread, "landing", runs a pass every two seconds, and sooner when an approval pokes it.
//! A pass does two things. [`DaemonApi::merge_pending`] runs the merge of every approved
//! `delivery.merge` proposal, outside the idempotency gate (see `api_landing`). Then
//! [`DaemonApi::landing_tick`] polls the pull requests of tasks that deliver by pull request:
//! the user opens each pull request by hand and the daemon only reads it. The poller never
//! opens, closes, comments on, or merges a pull request.
//!
//! Bounds: at most 20 `GitHub` calls a tick; a tick every 120 seconds while a pull request is
//! watched and every 600 seconds when none is; a failing pull request backs off by doubling up
//! to 15 minutes; a `RateLimited` answer pauses the poller for 15 minutes. A read that fails
//! before the host answered stores nothing. Only the fixed text of a `GitHubError`, the task
//! id and a project code reach a log: never a title, a check name, a URL or host output.
//!
//! The clock is a parameter of both entry points: the thread passes wall time and a test
//! passes any reading and calls them directly.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Condvar, Weak};
use std::thread::{self, JoinHandle};

use orchestrator_core::contract::DeliveryPolicy;
use orchestrator_core::landing::PrState;
use orchestrator_core::orchestrator_action::{Action, ActionKind as ProposalKind};
use orchestrator_core::project_mode::ProjectMode;
use orchestrator_git::github::{GitHubError, PullRequestView, RepoRef};
use orchestrator_store::{PullRequestChange, PullRequestRow, PullRequestSeen, StoredCheck};

use super::*;

const LOG: &str = "landing";
/// How often the loop thread runs a pass.
const TICK: Duration = Duration::from_secs(2);
/// Milliseconds between polls while a pull request is watched.
pub(super) const BUSY_INTERVAL_MS: u64 = 120_000;
/// Milliseconds between polls when none is.
pub(super) const IDLE_INTERVAL_MS: u64 = 600_000;
/// Most `GitHub` calls one poll makes.
pub(super) const MAX_CALLS_PER_TICK: usize = 20;
/// Longest backoff of one failing pull request, and the pause after `RateLimited`.
pub(super) const BACKOFF_CAP_MS: u64 = 15 * 60 * 1000;
/// Fewest milliseconds between two `pulls.refresh` of one task.
pub(super) const REFRESH_MIN_MS: u64 = 10_000;
/// The window, in milliseconds, and the most `pulls.refresh` reads inside it across all tasks.
pub(super) const REFRESH_WINDOW_MS: u64 = 60_000;
pub(super) const REFRESH_GLOBAL_CAP: usize = 20;
/// Milliseconds between two reads that settle a merge whose reply was lost.
pub(super) const SETTLE_INTERVAL_MS: u64 = 120_000;
/// Reads of a still-open pull request after which a lost merge reply is given up.
pub(super) const SETTLE_READS: u8 = 3;
/// Entries a schedule map may hold before stale ones are dropped.
const SCHEDULE_CAP: usize = 1_000;
/// Pending merge proposals read when a pull request moves.
const SUPERSEDE_PAGE: usize = 200;

#[derive(Default)]
struct Signal {
    stop: bool,
    poke: bool,
}

/// In-memory schedule: a restart empties it, which only makes the next pass read sooner.
#[derive(Default)]
pub(super) struct Schedule {
    /// No poll before this time.
    pub(super) next_poll_ms: u64,
    /// No poll at all before this time (a `RateLimited` answer).
    pub(super) paused_until_ms: u64,
    /// No poll of this task before this time.
    pub(super) next_due: BTreeMap<String, u64>,
    /// Consecutive failed reads per task.
    pub(super) failures: BTreeMap<String, u32>,
    /// When `pulls.refresh` last read each task.
    pub(super) refreshed: BTreeMap<String, u64>,
    /// When each `pulls.refresh` read of the last minute was made, any task.
    pub(super) refresh_times: Vec<u64>,
    /// Projects already named in a log line this run.
    pub(super) announced: BTreeSet<String>,
    /// No settle read of this merge attempt before this time.
    pub(super) settle_due: BTreeMap<String, u64>,
    /// Reads that found a lost-reply merge still open, per proposal.
    pub(super) unknown_reads: BTreeMap<String, u8>,
}

/// The landing thread's handle, signal and schedule, kept by the API.
#[derive(Default)]
pub(super) struct LandingState {
    thread: Mutex<Option<JoinHandle<()>>>,
    signal: Arc<(Mutex<Signal>, Condvar)>,
    pub(super) schedule: Mutex<Schedule>,
}

/// What one poll did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PollReport {
    /// Pull requests read.
    pub polled: usize,
    /// Reads that created or changed a row.
    pub changed: usize,
    /// Tasks whose pull request was not found yet.
    pub not_found: usize,
    /// Reads that failed.
    pub failed: usize,
    /// `GitHub` calls made.
    pub calls: usize,
}

/// A task whose pull request the poller follows.
pub(super) struct Watched {
    pub(super) task_id: String,
    pub(super) project_id: String,
    pub(super) branch: String,
    /// The number of the stored pull request, if one was found.
    pub(super) number: Option<u64>,
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// The text the log may carry for a refused host call.
fn error_text(error: GitHubError) -> String {
    error.to_string()
}

impl DaemonApi {
    /// Spawns the thread "landing": a pass, then two seconds of sleep, until stopped or poked.
    /// It holds only a weak reference, so a dropped API ends it. Called once by `main` after
    /// the API is in an `Arc`; the API never starts it for itself, so a test that builds one
    /// never has a poller behind it.
    pub(crate) fn start_landing(self: &Arc<Self>) {
        let weak: Weak<Self> = Arc::downgrade(self);
        let signal = self.landing.signal.clone();
        let spawned = thread::Builder::new()
            .name("landing".into())
            .spawn(move || loop {
                let Some(api) = weak.upgrade() else {
                    return;
                };
                // No guard held here: only the phases that act take one (see `landing_tick`).
                if crate::executor::serving(&api.stop_gate).is_some() {
                    let now_ms = crate::executor::unix_now_ms();
                    let passed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        api.merge_pending(now_ms);
                        api.landing_tick(now_ms);
                    }));
                    if passed.is_err() {
                        api.config
                            .diagnostics
                            .error(LOG, "landing_pass_panicked", &[]);
                    }
                }
                drop(api);
                let (flag, wake) = &*signal;
                let (mut state, _) = wake
                    .wait_timeout_while(lock(flag), TICK, |state| !state.stop && !state.poke)
                    .unwrap_or_else(|error| error.into_inner());
                if state.stop {
                    return;
                }
                state.poke = false;
            });
        match spawned {
            Ok(thread) => *lock(&self.landing.thread) = Some(thread),
            Err(_) => self
                .config
                .diagnostics
                .error(LOG, "landing_thread_not_started", &[]),
        }
    }

    /// Stops the landing thread and waits for it. Nothing happens when none runs.
    pub(super) fn stop_landing(&self) {
        let Some(thread) = lock(&self.landing.thread).take() else {
            return;
        };
        let (flag, wake) = &*self.landing.signal;
        lock(flag).stop = true;
        wake.notify_all();
        let _ = thread.join();
    }

    /// Asks the landing thread to run a pass now: an approved merge is waiting.
    pub(super) fn poke_landing(&self) {
        let (flag, wake) = &*self.landing.signal;
        lock(flag).poke = true;
        wake.notify_all();
    }

    /// One poll at `now_ms`, when one is due. Reads the pull request of each watched task,
    /// stores what changed and supersedes the merge cards that no longer match. Never starts an
    /// agent, never writes to a remote. The reads hold no stop gate, so a poll in flight never
    /// makes an idle shutdown refuse; each store write holds a read guard and a closed gate
    /// ends the tick.
    pub(super) fn landing_tick(&self, now_ms: u64) -> PollReport {
        let mut report = PollReport::default();
        {
            let schedule = lock(&self.landing.schedule);
            if now_ms < schedule.next_poll_ms || now_ms < schedule.paused_until_ms {
                return report;
            }
        }
        let (watched, any_open) = match self.watched_tasks(now_ms) {
            Ok(found) => found,
            Err(()) => {
                self.config.diagnostics.warn(LOG, "poll_list_failed", &[]);
                return report;
            }
        };
        // A task waiting out a backoff still counts as watched: it is not an idle poller.
        lock(&self.landing.schedule).next_poll_ms = now_ms
            + if any_open {
                BUSY_INTERVAL_MS
            } else {
                IDLE_INTERVAL_MS
            };
        let mut repos: HashMap<String, Option<RepoRef>> = HashMap::new();
        for task in watched {
            // A task not yet found costs two calls (find, then view): never past the bound.
            if report.calls + 2 > MAX_CALLS_PER_TICK {
                break;
            }
            let repo = match repos.get(&task.project_id) {
                Some(repo) => repo.clone(),
                None => {
                    report.calls += 1;
                    let found = self.repo_for_project(&task.project_id);
                    let repo = match found {
                        Ok(repo) => Some(repo),
                        Err(GitHubError::RateLimited) => {
                            self.pause(now_ms);
                            return report;
                        }
                        Err(error) => {
                            self.announce_skipped(&task.project_id, error);
                            None
                        }
                    };
                    repos.insert(task.project_id.clone(), repo.clone());
                    repo
                }
            };
            let Some(repo) = repo else {
                continue;
            };
            let observed = match self.read_one(&task, &repo, &mut report.calls) {
                Ok(Some((number, view))) => {
                    let Some(_serving) = crate::executor::serving(&self.stop_gate) else {
                        return report;
                    };
                    self.store_one(&task, &repo, number, &view, now_ms)
                        .map(Some)
                }
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            match observed {
                Ok(Some(change)) => {
                    report.polled += 1;
                    if matches!(
                        change,
                        PullRequestChange::Created | PullRequestChange::Changed
                    ) {
                        report.changed += 1;
                    }
                    self.polled_ok(&task.task_id, now_ms);
                }
                Ok(None) => {
                    report.not_found += 1;
                    self.polled_ok(&task.task_id, now_ms);
                }
                Err(GitHubError::RateLimited) => {
                    report.failed += 1;
                    self.pause(now_ms);
                    return report;
                }
                Err(error) => {
                    report.failed += 1;
                    self.poll_failed(&task.task_id, error, now_ms);
                }
            }
        }
        report
    }

    /// The tasks to read this tick: delivery by pull request, succeeded, in a project that is
    /// not archived and not `local-only`, whose pull request is not yet merged or closed, and
    /// whose backoff has passed, the longest-unread first so no task starves behind others.
    /// At most 20 of them. The flag says whether any task is watched at all, due or not.
    fn watched_tasks(&self, now_ms: u64) -> Result<(Vec<Watched>, bool), ()> {
        let tasks = self.store.list().map_err(|_| ())?;
        let archived: BTreeSet<String> = self
            .store
            .archived_project_ids()
            .map_err(|_| ())?
            .into_iter()
            .collect();
        let rows: BTreeMap<String, PullRequestRow> = self
            .store
            .list_pull_requests(None, 1_000)
            .map_err(|_| ())?
            .into_iter()
            .map(|row| (row.task_id.clone(), row))
            .collect();
        let mut modes: HashMap<String, bool> = HashMap::new();
        let mut watched: Vec<(u64, Watched)> = Vec::new();
        let mut any_open = false;
        for task in tasks {
            let project_id = &task.contract.project_id;
            if task.contract.delivery != DeliveryPolicy::PullRequest
                || task.state != TaskState::Succeeded
                || archived.contains(project_id)
            {
                continue;
            }
            let row = rows.get(&task.contract.id);
            if row.is_some_and(|row| row.state != PrState::Open) {
                continue;
            }
            let aims_at_a_pull_request = match modes.get(project_id) {
                Some(answer) => *answer,
                None => {
                    let answer = self
                        .project_landing(project_id)
                        .is_ok_and(|(mode, _)| mode != ProjectMode::LocalOnly);
                    modes.insert(project_id.clone(), answer);
                    answer
                }
            };
            if !aims_at_a_pull_request {
                continue;
            }
            any_open = true;
            let due = lock(&self.landing.schedule)
                .next_due
                .get(&task.contract.id)
                .copied()
                .unwrap_or(0);
            if now_ms < due {
                continue;
            }
            let branch = match row {
                Some(row) => row.branch.clone(),
                None => match self.store.workspaces_for_task(&task.contract.id) {
                    Ok(spaces) => match spaces.last() {
                        Some(space) => space.branch.clone(),
                        None => continue,
                    },
                    Err(_) => continue,
                },
            };
            watched.push((
                due,
                Watched {
                    task_id: task.contract.id.clone(),
                    project_id: project_id.clone(),
                    branch,
                    number: row.map(|row| row.number),
                },
            ));
        }
        watched.sort_by_key(|(due, _)| *due);
        watched.truncate(MAX_CALLS_PER_TICK);
        Ok((
            watched.into_iter().map(|(_, task)| task).collect(),
            any_open,
        ))
    }

    /// The GitHub repository a project's `origin` names. A project whose root is not a Git work
    /// tree is not on GitHub for this purpose.
    pub(super) fn repo_for_project(&self, project_id: &str) -> Result<RepoRef, GitHubError> {
        let repository =
            Repository::new(Path::new(project_id)).map_err(|_| GitHubError::NotFound)?;
        self.github.repo_of(&repository)
    }

    /// Reads one task's pull request once and stores what changed, for a caller that already
    /// holds the stop gate. See [`Self::read_one`] and [`Self::store_one`].
    pub(super) fn observe_one(
        &self,
        task: &Watched,
        repo: &RepoRef,
        now_ms: u64,
        calls: &mut usize,
    ) -> Result<Option<PullRequestChange>, GitHubError> {
        match self.read_one(task, repo, calls)? {
            Some((number, view)) => self.store_one(task, repo, number, &view, now_ms).map(Some),
            None => Ok(None),
        }
    }

    /// Reads one task's pull request from the host and stores nothing. Finds the pull request
    /// by the task's branch when none is stored. `Ok(None)`: no pull request yet, which is no
    /// error, because the user may not have opened it. Counts each `GitHub` call in `calls`.
    fn read_one(
        &self,
        task: &Watched,
        repo: &RepoRef,
        calls: &mut usize,
    ) -> Result<Option<(u64, PullRequestView)>, GitHubError> {
        let number = match task.number {
            Some(number) => number,
            None => {
                *calls += 1;
                match self.github.find_by_branch(repo, &task.branch)? {
                    Some(number) => number,
                    None => return Ok(None),
                }
            }
        };
        *calls += 1;
        Ok(Some((number, self.github.view(repo, number)?)))
    }

    /// Stores what a read found and supersedes the merge cards it outdates.
    fn store_one(
        &self,
        task: &Watched,
        repo: &RepoRef,
        number: u64,
        view: &PullRequestView,
        now_ms: u64,
    ) -> Result<PullRequestChange, GitHubError> {
        let seen = seen_of(task, number, view);
        let now = i64::try_from(now_ms).unwrap_or(i64::MAX);
        let change = self
            .store
            .observe_pull_request(&seen, now)
            .map_err(|_| GitHubError::Failed("store"))?;
        // The first repository a pull request is read from stays its repository: a merge grant
        // merges nowhere else.
        self.store
            .bind_pull_request_repo(&task.task_id, repo.owner(), repo.name())
            .map_err(|_| GitHubError::Failed("store"))?;
        if matches!(
            change,
            PullRequestChange::Created | PullRequestChange::Changed
        ) {
            self.supersede_stale_merges(&task.task_id, number, view);
        }
        Ok(change)
    }

    /// Moves the pending `delivery.merge` cards of this pull request that are bound to another
    /// head, or to a pull request that is no longer open, to `superseded`. Their digests bound
    /// a commit the user is no longer being shown.
    fn supersede_stale_merges(&self, task_id: &str, number: u64, view: &PullRequestView) {
        let Ok(pending) = self.store.pending_action_proposals(SUPERSEDE_PAGE) else {
            self.config
                .diagnostics
                .warn(LOG, "supersede_read_failed", &[]);
            return;
        };
        let subject = format!("delivery.merge:{task_id}:{number}");
        for proposal in pending {
            if proposal.kind != ProposalKind::DeliveryMerge || proposal.subject != subject {
                continue;
            }
            let bound = match serde_json::from_str::<Action>(&proposal.params_json) {
                Ok(Action::DeliveryMerge(merge)) => merge.head_sha,
                _ => continue,
            };
            if view.state == PrState::Open && bound == view.head_sha {
                continue;
            }
            if self
                .store
                .supersede_pending_action_proposal(&proposal.id)
                .is_err()
            {
                self.config.diagnostics.warn(LOG, "supersede_failed", &[]);
            }
        }
    }

    fn polled_ok(&self, task_id: &str, now_ms: u64) {
        let mut schedule = lock(&self.landing.schedule);
        schedule.failures.remove(task_id);
        schedule
            .next_due
            .insert(task_id.to_owned(), now_ms + BUSY_INTERVAL_MS);
        trim(&mut schedule.next_due, now_ms);
    }

    /// Doubles the wait before this task is read again, up to 15 minutes. A read that failed
    /// before the host answered writes nothing; one that may have reached it is counted in the
    /// store.
    fn poll_failed(&self, task_id: &str, error: GitHubError, now_ms: u64) {
        {
            let mut schedule = lock(&self.landing.schedule);
            let failures = schedule.failures.entry(task_id.to_owned()).or_insert(0);
            *failures = failures.saturating_add(1);
            let doubled = BUSY_INTERVAL_MS << (*failures).min(10);
            let wait = doubled.min(BACKOFF_CAP_MS);
            schedule.next_due.insert(task_id.to_owned(), now_ms + wait);
            trim(&mut schedule.next_due, now_ms);
        }
        if error.may_have_reached_host() {
            // Only a counter; a task with no stored row has nothing to count.
            let _ = self.store.record_pull_request_poll_failure(task_id);
        }
        self.config.diagnostics.warn(
            LOG,
            "pr_poll_failed",
            &[("task", task_id), ("error", &error_text(error))],
        );
    }

    pub(super) fn pause(&self, now_ms: u64) {
        lock(&self.landing.schedule).paused_until_ms = now_ms + BACKOFF_CAP_MS;
        self.config.diagnostics.warn(LOG, "poller_paused", &[]);
    }

    /// Names a project the poller cannot read, once per run. The project code only.
    fn announce_skipped(&self, project_id: &str, error: GitHubError) {
        let first = lock(&self.landing.schedule)
            .announced
            .insert(project_id.to_owned());
        if first {
            self.config.diagnostics.warn(
                LOG,
                "pr_project_skipped",
                &[
                    ("project", &project_code(project_id)),
                    ("error", &error_text(error)),
                ],
            );
        }
    }
}

/// Drops the entries of a schedule map that are already due, once it is large.
fn trim(map: &mut BTreeMap<String, u64>, now_ms: u64) {
    if map.len() > SCHEDULE_CAP {
        map.retain(|_, due| *due > now_ms);
    }
}

/// The row the store keeps for a read. Remote text is cleaned and capped by the store.
fn seen_of(task: &Watched, number: u64, view: &PullRequestView) -> PullRequestSeen {
    PullRequestSeen {
        task_id: task.task_id.clone(),
        project_id: task.project_id.clone(),
        number,
        branch: task.branch.clone(),
        base_branch: view.base_branch.clone(),
        state: view.state,
        head_sha: view.head_sha.clone(),
        mergeable: view.mergeable,
        checks: view
            .checks
            .iter()
            .map(|check| StoredCheck {
                name: check.name.clone(),
                state: check.state,
            })
            .collect(),
        title: view.title.clone(),
    }
}
