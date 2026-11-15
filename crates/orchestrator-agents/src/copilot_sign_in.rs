//! GitHub Copilot sign-in state for the daemon session (DPES-1105).
//!
//! The Copilot CLI has no non-interactive sign-in status command. Sign-in is
//! therefore learned from real runs: a completed run means `Ready`, and a
//! failure whose provider text names a missing seat or sign-in sets that
//! state. An explicit check spends one call on the cheapest model. It runs on
//! a detached thread, so the request that started it returns at once, and a
//! client that disconnects or times out does not cancel it. The state lives in
//! memory only, so a restarted daemon starts `NotChecked` and never reports a
//! check that died with the old process. Nothing here reads or stores a
//! credential.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::contract::AgentExit;

/// Failure text that means the account has no Copilot seat. Unverified: no
/// captured Copilot failure for an account without a seat is on record.
const NO_SEAT_PHRASES: [&str; 4] = ["seat", "subscription", "entitle", "copilot access"];
/// Failure text that means nobody is signed in. Unverified in the same way.
const SIGN_IN_PHRASES: [&str; 6] = [
    "sign in",
    "sign-in",
    "log in",
    "login",
    "logged in",
    "unauthorized",
];

pub(crate) const NOT_CHECKED_DETAIL: &str = "Not confirmed yet: sign-in is checked on your first Copilot use. If you have not signed in, run `copilot login`.";
pub(crate) const NOT_SIGNED_IN_DETAIL: &str =
    "Not signed in to GitHub Copilot. Run `copilot login`, then retry.";
pub(crate) const NO_SEAT_DETAIL: &str = "This GitHub account has no GitHub Copilot subscription.";
const RUN_NOT_COMPLETED: &str = "the Copilot run did not complete";
const RUN_STOPPED: &str = "the Copilot check stopped unexpectedly";

/// What is known about sign-in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInState {
    /// Nothing has established sign-in yet.
    #[default]
    NotChecked,
    /// An explicit check is running in the background. Nothing else is started.
    Checking,
    /// A run completed, so the CLI is signed in and the account has a seat.
    Ready,
    NotSignedIn,
    NoSeat,
    /// A failure that is not a sign-in fact; `reason` says what it was.
    Error,
}

/// The sign-in state with the facts it was established from.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SignInRecord {
    pub state: SignInState,
    /// Human-readable text. `None` when there is nothing to say.
    pub reason: Option<String>,
    /// When this state was established, in Unix milliseconds. `None` when
    /// it was never established.
    pub checked_at_ms: Option<u64>,
    /// Premium requests from the check's usage file. `None` when not reported.
    pub premium_requests: Option<f64>,
    /// AI credits from the check's usage file. `None` when not reported.
    pub ai_credits: Option<f64>,
    /// A warning from a completed run, such as an error a user hook reported.
    /// Sanitized and capped. `None` when there is none.
    pub warning: Option<String>,
}

impl SignInRecord {
    /// A check that did not run, for example because the sandbox host cannot
    /// run Copilot. Leaves the state unknown and does not count as a check.
    pub fn refused(reason: String) -> Self {
        Self {
            reason: Some(reason),
            ..Self::default()
        }
    }

    /// The record while a background check is running.
    pub(crate) fn checking() -> Self {
        Self {
            state: SignInState::Checking,
            ..Self::default()
        }
    }

    fn established(state: SignInState, reason: Option<String>, now_ms: u64) -> Self {
        Self {
            state,
            reason,
            checked_at_ms: Some(now_ms),
            premium_requests: None,
            ai_credits: None,
            warning: None,
        }
    }

    /// A failure the daemon itself observed, such as a timeout. The reason is
    /// set directly and is never run through [`classify_failure`], so its
    /// wording cannot be mistaken for a provider's sign-in message.
    pub fn failed(reason: impl Into<String>) -> Self {
        Self::established(SignInState::Error, Some(reason.into()), now_ms())
    }
}

/// Classifies failure text from a run or a check. Model-unavailable text is
/// checked first, then missing seat, then missing sign-in. Anything else is
/// `Error` with the text as the reason.
pub fn classify_failure(text: &str, now_ms: u64) -> SignInRecord {
    let lower = text.to_lowercase();
    let (state, reason) = if lower.contains("not available") {
        (SignInState::Error, format!("model unavailable: {text}"))
    } else if NO_SEAT_PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        (SignInState::NoSeat, NO_SEAT_DETAIL.to_owned())
    } else if SIGN_IN_PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        (SignInState::NotSignedIn, NOT_SIGNED_IN_DETAIL.to_owned())
    } else if text.is_empty() {
        (SignInState::Error, RUN_NOT_COMPLETED.to_owned())
    } else {
        (SignInState::Error, text.to_owned())
    };
    SignInRecord::established(state, Some(reason), now_ms)
}

/// The sign-in fact one finished run establishes, if it establishes any.
pub(crate) fn record_for_exit(
    exit: &AgentExit,
    warning: Option<String>,
    now_ms: u64,
) -> SignInRecord {
    match exit {
        AgentExit::Completed { .. } => SignInRecord {
            warning,
            ..SignInRecord::established(SignInState::Ready, None, now_ms)
        },
        AgentExit::Failed { reason, .. } => {
            classify_failure(reason.as_deref().unwrap_or(""), now_ms)
        }
        AgentExit::Cancelled { .. } => SignInRecord::established(
            SignInState::Error,
            Some("the Copilot run was cancelled".to_owned()),
            now_ms,
        ),
    }
}

/// Sign-in state shared by the catalog, the explicit check and every Copilot
/// run in one daemon session. Memory only: a restart forgets it.
#[derive(Debug, Default)]
pub struct CopilotSignIn {
    record: Mutex<SignInRecord>,
}

impl CopilotSignIn {
    pub fn snapshot(&self) -> SignInRecord {
        lock(&self.record).clone()
    }

    /// Applies what one real run established. Only `Ready`, `NotSignedIn`
    /// and `NoSeat` change the state. Other failures do not, because they say
    /// nothing about sign-in.
    pub fn record_run(&self, exit: &AgentExit) {
        let observed = record_for_exit(exit, None, now_ms());
        if matches!(
            observed.state,
            SignInState::Ready | SignInState::NotSignedIn | SignInState::NoSeat
        ) {
            *lock(&self.record) = observed;
        }
    }

    /// Starts the explicit check on a detached thread and returns at once.
    ///
    /// - While a check is running, the running record is returned and nothing
    ///   new starts, even with `force`. One paid run at a time.
    /// - A settled answer is returned without a new run unless `force` is set.
    /// - Otherwise the state becomes `Checking`, `run` runs on its own thread,
    ///   and its outcome becomes the stored record when it ends. A panic in
    ///   `run` settles as an error, so `Checking` can never be left behind.
    ///
    /// Nothing waits on the thread, so a client that disconnects or times out
    /// cancels nothing.
    pub fn start(
        self: &Arc<Self>,
        force: bool,
        run: impl FnOnce() -> SignInRecord + Send + 'static,
    ) -> SignInRecord {
        {
            let mut record = lock(&self.record);
            if record.state == SignInState::Checking
                || (!force && record.state != SignInState::NotChecked)
            {
                return record.clone();
            }
            *record = SignInRecord::checking();
        }
        let shared = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("copilot-sign-in".to_owned())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(run))
                    .unwrap_or_else(|_| SignInRecord::failed(RUN_STOPPED));
                shared.finish(outcome);
            });
        if let Err(error) = spawned {
            self.finish(SignInRecord::failed(format!(
                "could not start the Copilot check: {error}"
            )));
        }
        self.snapshot()
    }

    /// Stores the outcome of a background check. An outcome that never
    /// established a time is stored as an error, so the state is never left
    /// `Checking` and never looks like a verified answer.
    fn finish(&self, outcome: SignInRecord) {
        let stored = if outcome.checked_at_ms.is_some() {
            outcome
        } else {
            SignInRecord::failed(
                outcome
                    .reason
                    .unwrap_or_else(|| RUN_NOT_COMPLETED.to_owned()),
            )
        };
        *lock(&self.record) = stored;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// Waits until a background check settles, or fails the test after a bound.
    fn settled(cache: &CopilotSignIn) -> SignInRecord {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let record = cache.snapshot();
            if record.state != SignInState::Checking {
                return record;
            }
            assert!(Instant::now() < deadline, "the check never settled");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn ready_record() -> SignInRecord {
        record_for_exit(
            &AgentExit::Completed {
                code: Some(0),
                last_message: Some("OK".into()),
            },
            None,
            2,
        )
    }

    #[test]
    fn failure_text_is_classified_model_then_seat_then_sign_in() {
        let state = |text| classify_failure(text, 1).state;
        assert_eq!(
            state("Execution failed: Model \"x\" is not available."),
            SignInState::Error
        );
        assert_eq!(
            state("Model is not available for your Copilot plan"),
            SignInState::Error,
            "model text wins over the seat text"
        );
        assert_eq!(
            state("No active Copilot access for this account"),
            SignInState::NoSeat
        );
        assert_eq!(state("Please run copilot login"), SignInState::NotSignedIn);
        assert_eq!(state("401 unauthorized"), SignInState::NotSignedIn);
        assert_eq!(state("socket hang up"), SignInState::Error);
        assert_eq!(state(""), SignInState::Error);
    }

    #[test]
    fn a_run_that_is_not_a_sign_in_fact_leaves_the_state_alone() {
        let cache = CopilotSignIn::default();
        cache.record_run(&AgentExit::Failed {
            code: Some(1),
            reason: Some("socket hang up".into()),
        });
        assert_eq!(cache.snapshot().state, SignInState::NotChecked);
        cache.record_run(&AgentExit::Completed {
            code: Some(0),
            last_message: Some("PONG".into()),
        });
        assert_eq!(cache.snapshot().state, SignInState::Ready);
        cache.record_run(&AgentExit::Failed {
            code: Some(1),
            reason: Some("socket hang up".into()),
        });
        assert_eq!(cache.snapshot().state, SignInState::Ready);
    }

    #[test]
    fn a_check_reports_checking_at_once_and_settles_later() {
        let cache = Arc::new(CopilotSignIn::default());
        let (release, gate) = mpsc::channel::<()>();
        let started = cache.start(false, move || {
            let _ = gate.recv();
            ready_record()
        });
        assert_eq!(
            started.state,
            SignInState::Checking,
            "the start call does not wait"
        );
        assert_eq!(cache.snapshot().state, SignInState::Checking);
        release.send(()).expect("the check is waiting");
        assert_eq!(settled(&cache).state, SignInState::Ready);
    }

    #[test]
    fn a_second_start_while_checking_runs_nothing_more_even_when_forced() {
        let cache = Arc::new(CopilotSignIn::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let (release, gate) = mpsc::channel::<()>();
        let counted = Arc::clone(&calls);
        let first = cache.start(false, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            let _ = gate.recv();
            ready_record()
        });
        let second = cache.start(false, || panic!("a second run must not start"));
        let forced = cache.start(true, || {
            panic!("a forced run must not start while checking")
        });
        assert_eq!(first.state, SignInState::Checking);
        assert_eq!(second.state, SignInState::Checking);
        assert_eq!(forced.state, SignInState::Checking);
        release.send(()).expect("the check is waiting");
        assert_eq!(settled(&cache).state, SignInState::Ready);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly one paid run");
    }

    #[test]
    fn the_settled_answer_is_cached_until_a_check_is_forced() {
        let cache = Arc::new(CopilotSignIn::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        cache.start(false, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            classify_failure("Please run copilot login", 7)
        });
        assert_eq!(settled(&cache).state, SignInState::NotSignedIn);
        let cached = cache.start(false, || {
            panic!("a settled answer is served from the cache")
        });
        assert_eq!(cached.state, SignInState::NotSignedIn);
        let counted = Arc::clone(&calls);
        cache.start(true, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            ready_record()
        });
        assert_eq!(settled(&cache).state, SignInState::Ready);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "one run, then one forced run"
        );
    }

    #[test]
    fn a_panicking_run_settles_as_an_error_and_never_stays_checking() {
        let cache = Arc::new(CopilotSignIn::default());
        cache.start(false, || panic!("simulated failure inside the check"));
        let record = settled(&cache);
        assert_eq!(record.state, SignInState::Error);
        assert_eq!(record.reason.as_deref(), Some(RUN_STOPPED));
    }

    #[test]
    fn a_run_that_never_establishes_a_time_is_stored_as_an_error() {
        let cache = Arc::new(CopilotSignIn::default());
        cache.start(false, || {
            SignInRecord::refused("Copilot sandboxing is unavailable".into())
        });
        let record = settled(&cache);
        assert_eq!(record.state, SignInState::Error);
        assert_eq!(
            record.reason.as_deref(),
            Some("Copilot sandboxing is unavailable")
        );
        assert!(
            record.checked_at_ms.is_some(),
            "a stored error carries its time"
        );
    }

    #[test]
    fn a_fresh_cache_starts_not_checked_so_a_restart_never_shows_checking() {
        // The state is in memory only, so a daemon that restarts mid-check has
        // no record of it: its first snapshot is NotChecked, never Checking.
        let restarted = CopilotSignIn::default();
        assert_eq!(restarted.snapshot().state, SignInState::NotChecked);
        assert_eq!(restarted.snapshot().checked_at_ms, None);
    }
}
