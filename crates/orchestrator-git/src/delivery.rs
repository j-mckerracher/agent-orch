//! Report-only and local-review delivery for a verified task candidate.
//!
//! The destination and candidate worktree must share one Git common directory.
//! Delivery writes only a review ref, never checks out or merges a branch. The
//! callbacks must recheck current approval and verification records and persist
//! the final step. SQLite makes its step and audit event atomic; Git ref changes
//! cannot join that transaction.
//!
//! Updated: 2026-11-08T09:52:29Z.

use std::error::Error as StdError;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::git::git;
use crate::{Error, Repository};

const DELIVERY_LOCK_NAME: &str = ".orchestrator-delivery.lock";
const REVIEW_REF_PREFIX: &str = "refs/heads/orchestrator/review/";
static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(0);

/// Error returned by an approval, verification, or persistence callback.
pub type CallbackError = Box<dyn StdError + Send + Sync + 'static>;

/// The only delivery actions supported by this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryAction {
    /// Produce and persist a report without changing repository state.
    ReportOnly,
    /// Create a branch ref under `refs/heads/orchestrator/review/`.
    LocalReview,
}

impl DeliveryAction {
    /// Stable value used by the SQLite delivery record.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReportOnly => "report_only",
            Self::LocalReview => "local_review",
        }
    }
}

/// Outcome recorded for one delivery step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Report was persisted without changing Git state.
    Reported,
    /// A new local-review ref was created.
    ReviewRefCreated,
    /// The requested ref already pointed to this candidate.
    ReviewRefAlreadyMatches,
    /// The request was refused before delivery.
    Refused,
}

impl DeliveryOutcome {
    /// Stable value used by the SQLite delivery record.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reported => "reported",
            Self::ReviewRefCreated => "review_ref_created",
            Self::ReviewRefAlreadyMatches => "review_ref_already_matches",
            Self::Refused => "refused",
        }
    }
}

/// Recorded task and verifier inputs for one delivery attempt.
#[derive(Debug, Clone, Copy)]
pub struct DeliveryRequest<'a> {
    /// Caller-assigned id for the append-only delivery step.
    pub step_id: &'a str,
    /// Task owning the candidate worktree.
    pub task_id: &'a str,
    /// Full candidate object id recorded by the attempt and approval.
    pub candidate_sha: &'a str,
    /// Base commit used for the report diff.
    pub base_sha: &'a str,
    /// Approval record checked for this action.
    pub approval_id: &'a str,
    /// Persisted verification run checked for this candidate.
    pub verification_run_id: &'a str,
    /// SHA-256 digest of canonical action parameters.
    pub parameter_digest: [u8; 32],
    /// Task worktree that currently resolves to `candidate_sha`.
    pub worktree_path: &'a Path,
}

/// Exact approval binding passed to the caller's M3-02 approval check.
pub struct ApprovalCheck<'a> {
    /// Action kind covered by the approval.
    pub action: DeliveryAction,
    /// Task id covered by the approval.
    pub task_id: &'a str,
    /// Approval record id.
    pub approval_id: &'a str,
    /// Candidate commit covered by the approval.
    pub candidate_sha: &'a str,
    /// Digest of canonical parameters, including destination and review ref.
    pub parameter_digest: &'a [u8; 32],
    /// Canonical destination repository path.
    pub destination: &'a Path,
    /// Recorded base commit used to build the candidate.
    pub base_sha: &'a str,
    /// Review ref for local-review delivery, absent for report-only.
    pub target_ref: Option<&'a str>,
}

/// Exact verifier record dimensions that delivery must revalidate.
pub struct VerificationCheck<'a> {
    /// Task id owning the verification run.
    pub task_id: &'a str,
    /// Persisted verification run id.
    pub run_id: &'a str,
    /// Candidate SHA that the run must have passed.
    pub candidate_sha: &'a str,
}

/// Store adapters used while the destination lock is held.
pub struct DeliveryCallbacks<'a> {
    /// Must call M3-02 `approval::check` with every binding field unchanged.
    pub check_approval: &'a dyn for<'check> Fn(&ApprovalCheck<'check>) -> Result<(), CallbackError>,
    /// Must verify the current persisted run matches task, id, candidate, and passed status.
    pub check_verification:
        &'a dyn for<'check> Fn(&VerificationCheck<'check>) -> Result<(), CallbackError>,
    /// Persists the final success or refusal together with its audit event.
    pub persist_step: &'a dyn Fn(&DeliveryStep) -> Result<(), CallbackError>,
}
/// Persistable record returned by either delivery mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryStep {
    /// Caller-assigned step id.
    pub id: String,
    /// Owning task id.
    pub task_id: String,
    /// Action covered by approval.
    pub action: DeliveryAction,
    /// Exact verified candidate commit.
    pub candidate_sha: String,
    /// Approval record checked for this action.
    pub approval_id: String,
    /// Verification run checked for this candidate.
    pub verification_run_id: String,
    /// Canonical action-parameter digest.
    pub parameter_digest: [u8; 32],
    /// Canonical destination repository path.
    pub destination: PathBuf,
    /// Review ref, absent for report-only delivery.
    pub target_ref: Option<String>,
    /// Final result, including refusals.
    pub outcome: DeliveryOutcome,
    /// Failure reason for a refused step.
    pub detail: Option<String>,
    /// Git diff summary for the verified candidate.
    pub diff_summary: Option<String>,
}

/// Refusals and operational failures from delivery.
#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("delivery step id and task id must not be empty")]
    InvalidRequest,
    #[error("candidate commit must be a full lowercase SHA-1 or SHA-256: {0:?}")]
    InvalidCandidateSha(String),
    #[error("base commit must be a full lowercase SHA-1 or SHA-256: {0:?}")]
    InvalidBaseSha(String),
    #[error("base ref contains invalid option or line-break syntax: {0:?}")]
    InvalidBaseRef(String),
    #[error("candidate worktree HEAD changed from {expected} to {found}")]
    CandidateChanged { expected: String, found: String },
    #[error("candidate worktree has uncommitted changes: {0}")]
    DirtyCandidate(String),
    #[error(
        "candidate worktree {worktree} does not share destination Git directory {destination}"
    )]
    DifferentRepository {
        worktree: PathBuf,
        destination: PathBuf,
    },
    #[error("candidate worktree resolves to the destination primary checkout")]
    PrimaryCheckoutCandidate,
    #[error("review ref must be a valid ref under refs/heads/orchestrator/review/: {0}")]
    InvalidReviewRef(String),
    #[error("review ref {target_ref} points to {existing}, not candidate {candidate}")]
    ReviewRefConflict {
        target_ref: String,
        existing: String,
        candidate: String,
    },
    #[error("approval check refused delivery: {0}")]
    Approval(#[source] CallbackError),
    #[error("verification revalidation refused delivery: {0}")]
    Verification(#[source] CallbackError),
    #[error("delivery step persistence failed: {0}")]
    Persistence(#[source] CallbackError),
    #[error(transparent)]
    DestinationLock(#[from] DestinationLockError),
    #[error(transparent)]
    Git(#[from] Error),
    #[error("delivery filesystem operation failed: {0}")]
    Io(#[from] io::Error),
}

/// Report-only or local-review result of a non-destructive worktree check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeDeletionSafety {
    /// Worktree is clean and its HEAD is reachable from the base ref.
    SafeToDelete,
    /// Worktree must remain available for inspection or recovery.
    Unsafe(UnsafeToDeleteReason),
}

/// Concrete reason a task worktree must be retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsafeToDeleteReason {
    /// The path resolves to the primary checkout or a path inside it.
    PrimaryCheckout,
    /// Tracked or untracked changes remain in the worktree.
    Dirty(String),
    /// HEAD is not reachable from the current base ref.
    NotReachable { head: String, base_ref: String },
}

/// Creates a report step without changing any Git ref or worktree.
pub fn report_only(
    repository: &Repository,
    request: DeliveryRequest<'_>,
    callbacks: &DeliveryCallbacks<'_>,
) -> Result<DeliveryStep, DeliveryError> {
    execute_delivery(
        repository,
        request,
        DeliveryAction::ReportOnly,
        None,
        callbacks,
    )
}

/// Creates or reuses a local-review ref without checking out or merging it.
pub fn local_review(
    repository: &Repository,
    request: DeliveryRequest<'_>,
    target_ref: &str,
    callbacks: &DeliveryCallbacks<'_>,
) -> Result<DeliveryStep, DeliveryError> {
    execute_delivery(
        repository,
        request,
        DeliveryAction::LocalReview,
        Some(target_ref),
        callbacks,
    )
}

/// Reports whether worktree can be removed; this function never deletes it.
pub fn check_worktree_safe_to_delete(
    repository: &Repository,
    worktree_path: &Path,
    base_ref: &str,
) -> Result<WorktreeDeletionSafety, DeliveryError> {
    if base_ref.starts_with('-') || base_ref.contains(['\n', '\r']) {
        return Err(DeliveryError::InvalidBaseRef(base_ref.to_owned()));
    }
    let canonical_worktree = fs::canonicalize(worktree_path)?;
    let canonical_primary = fs::canonicalize(repository.path())?;
    if canonical_worktree == canonical_primary || canonical_worktree.starts_with(&canonical_primary)
    {
        return Ok(WorktreeDeletionSafety::Unsafe(
            UnsafeToDeleteReason::PrimaryCheckout,
        ));
    }
    let status = git(
        "check worktree cleanliness",
        worktree_path,
        ["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.is_empty() {
        return Ok(WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::Dirty(
            status,
        )));
    }
    let head = git("read worktree HEAD", worktree_path, ["rev-parse", "HEAD"])?;
    let base_sha = repository.resolve_base_ref(base_ref)?;
    match git(
        "check worktree reachability",
        repository.path(),
        ["merge-base", "--is-ancestor", &head, &base_sha],
    ) {
        Ok(_) => Ok(WorktreeDeletionSafety::SafeToDelete),
        Err(Error::Git { status, .. }) if status.code() == Some(1) => Ok(
            WorktreeDeletionSafety::Unsafe(UnsafeToDeleteReason::NotReachable {
                head,
                base_ref: base_ref.to_owned(),
            }),
        ),
        Err(error) => Err(DeliveryError::Git(error)),
    }
}
fn execute_delivery(
    repository: &Repository,
    request: DeliveryRequest<'_>,
    action: DeliveryAction,
    target_ref: Option<&str>,
    callbacks: &DeliveryCallbacks<'_>,
) -> Result<DeliveryStep, DeliveryError> {
    if request.step_id.is_empty()
        || request.task_id.is_empty()
        || request.approval_id.is_empty()
        || request.verification_run_id.is_empty()
    {
        return Err(DeliveryError::InvalidRequest);
    }
    let mut step = DeliveryStep {
        id: request.step_id.to_owned(),
        task_id: request.task_id.to_owned(),
        action,
        candidate_sha: request.candidate_sha.to_owned(),
        approval_id: request.approval_id.to_owned(),
        verification_run_id: request.verification_run_id.to_owned(),
        parameter_digest: request.parameter_digest,
        destination: repository.path().to_path_buf(),
        target_ref: target_ref.map(str::to_owned),
        outcome: DeliveryOutcome::Refused,
        detail: None,
        diff_summary: None,
    };
    let lock = match DestinationLock::acquire(repository.git_common_dir()) {
        Ok(lock) => lock,
        Err(error) => {
            let refusal = DeliveryError::DestinationLock(error);
            step.detail = Some(refusal.to_string());
            (callbacks.persist_step)(&step).map_err(DeliveryError::Persistence)?;
            return Err(refusal);
        }
    };
    let result = deliver_locked(
        repository, request, action, target_ref, callbacks, &mut step,
    );
    match &result {
        Ok(outcome) => step.outcome = *outcome,
        Err(error) => step.detail = Some(error.to_string()),
    }
    (callbacks.persist_step)(&step).map_err(DeliveryError::Persistence)?;
    drop(lock);
    result.map(|_| step)
}

fn deliver_locked(
    repository: &Repository,
    request: DeliveryRequest<'_>,
    action: DeliveryAction,
    target_ref: Option<&str>,
    callbacks: &DeliveryCallbacks<'_>,
    step: &mut DeliveryStep,
) -> Result<DeliveryOutcome, DeliveryError> {
    if action == DeliveryAction::LocalReview {
        validate_review_ref(repository, target_ref.ok_or(DeliveryError::InvalidRequest)?)?;
    }
    validate_candidate(repository, &request)?;
    (callbacks.check_verification)(&VerificationCheck {
        task_id: request.task_id,
        run_id: request.verification_run_id,
        candidate_sha: request.candidate_sha,
    })
    .map_err(DeliveryError::Verification)?;
    step.diff_summary = Some(git(
        "summarize delivery diff",
        repository.path(),
        ["diff", "--stat", request.base_sha, request.candidate_sha],
    )?);
    (callbacks.check_approval)(&ApprovalCheck {
        action,
        task_id: request.task_id,
        approval_id: request.approval_id,
        candidate_sha: request.candidate_sha,
        parameter_digest: &request.parameter_digest,
        destination: repository.path(),
        base_sha: request.base_sha,
        target_ref,
    })
    .map_err(DeliveryError::Approval)?;
    match action {
        DeliveryAction::ReportOnly => {
            validate_candidate(repository, &request)?;
            Ok(DeliveryOutcome::Reported)
        }
        DeliveryAction::LocalReview => create_review_ref(
            repository,
            target_ref.ok_or(DeliveryError::InvalidRequest)?,
            &request,
        ),
    }
}

fn validate_candidate(
    repository: &Repository,
    request: &DeliveryRequest<'_>,
) -> Result<(), DeliveryError> {
    if !is_full_object_id(request.candidate_sha) {
        return Err(DeliveryError::InvalidCandidateSha(
            request.candidate_sha.to_owned(),
        ));
    }
    if !is_full_object_id(request.base_sha) {
        return Err(DeliveryError::InvalidBaseSha(request.base_sha.to_owned()));
    }
    let destination_git_dir = fs::canonicalize(repository.git_common_dir())?;
    let candidate_repository = Repository::new(request.worktree_path)?;
    let candidate_git_dir = fs::canonicalize(candidate_repository.git_common_dir())?;
    if destination_git_dir != candidate_git_dir {
        return Err(DeliveryError::DifferentRepository {
            worktree: request.worktree_path.to_path_buf(),
            destination: repository.path().to_path_buf(),
        });
    }
    if fs::canonicalize(candidate_repository.path())? == fs::canonicalize(repository.path())? {
        return Err(DeliveryError::PrimaryCheckoutCandidate);
    }
    let candidate = git(
        "resolve current delivery candidate",
        request.worktree_path,
        ["rev-parse", "--verify", "HEAD^{commit}"],
    )?;
    if candidate != request.candidate_sha {
        return Err(DeliveryError::CandidateChanged {
            expected: request.candidate_sha.to_owned(),
            found: candidate,
        });
    }
    if repository.resolve_base_ref(request.base_sha)? != request.base_sha {
        return Err(DeliveryError::InvalidBaseSha(request.base_sha.to_owned()));
    }
    let status = git(
        "check delivery candidate cleanliness",
        request.worktree_path,
        ["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !status.is_empty() {
        return Err(DeliveryError::DirtyCandidate(status));
    }
    Ok(())
}

fn is_full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_review_ref(repository: &Repository, target_ref: &str) -> Result<(), DeliveryError> {
    if !target_ref.starts_with(REVIEW_REF_PREFIX) || target_ref.len() == REVIEW_REF_PREFIX.len() {
        return Err(DeliveryError::InvalidReviewRef(target_ref.to_owned()));
    }
    match git(
        "validate local-review ref",
        repository.path(),
        ["check-ref-format", target_ref],
    ) {
        Ok(_) => Ok(()),
        Err(Error::Git { status, .. }) if status.code() == Some(1) => {
            Err(DeliveryError::InvalidReviewRef(target_ref.to_owned()))
        }
        Err(error) => Err(error.into()),
    }
}

fn create_review_ref(
    repository: &Repository,
    target_ref: &str,
    request: &DeliveryRequest<'_>,
) -> Result<DeliveryOutcome, DeliveryError> {
    let candidate_sha = request.candidate_sha;
    if let Some(existing) = resolve_ref(repository, target_ref)? {
        if existing == candidate_sha {
            validate_candidate(repository, request)?;
            return Ok(DeliveryOutcome::ReviewRefAlreadyMatches);
        }
        return Err(DeliveryError::ReviewRefConflict {
            target_ref: target_ref.to_owned(),
            existing,
            candidate: candidate_sha.to_owned(),
        });
    }
    validate_candidate(repository, request)?;
    let zero_id = "0".repeat(candidate_sha.len());
    match git(
        "create local-review ref",
        repository.path(),
        ["update-ref", target_ref, candidate_sha, &zero_id],
    ) {
        Ok(_) => Ok(DeliveryOutcome::ReviewRefCreated),
        Err(write_error) => match resolve_ref(repository, target_ref)? {
            Some(existing) if existing == candidate_sha => {
                validate_candidate(repository, request)?;
                Ok(DeliveryOutcome::ReviewRefAlreadyMatches)
            }
            Some(existing) => Err(DeliveryError::ReviewRefConflict {
                target_ref: target_ref.to_owned(),
                existing,
                candidate: candidate_sha.to_owned(),
            }),
            None => Err(write_error.into()),
        },
    }
}

fn resolve_ref(repository: &Repository, target_ref: &str) -> Result<Option<String>, DeliveryError> {
    match git(
        "read local-review ref",
        repository.path(),
        ["rev-parse", "--verify", "--quiet", target_ref],
    ) {
        Ok(sha) => Ok(Some(sha)),
        Err(Error::Git { status, .. }) if status.code() == Some(1) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

struct DestinationLock {
    path: PathBuf,
    file: Option<File>,
    owner_contents: String,
    identity: Option<FileIdentity>,
}

impl DestinationLock {
    fn acquire(git_common_dir: &Path) -> Result<Self, DestinationLockError> {
        Self::acquire_with(git_common_dir, FileIdentity::from_file)
    }

    fn acquire_with(
        git_common_dir: &Path,
        identity_of: FileIdentityFn,
    ) -> Result<Self, DestinationLockError> {
        let path = fs::canonicalize(git_common_dir)?.join(DELIVERY_LOCK_NAME);
        let pid = std::process::id();
        let owner_contents = format!("pid={pid}\nowner={}\n", next_owner_id(pid));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(existing_lock_error(&path));
            }
            Err(error) => return Err(error.into()),
        };
        // The just-created lock file is unverified if its identity cannot be
        // read: fail acquisition with the real OS error and leave the file in
        // place for inspection rather than returning a guard that cannot
        // clean up (and would strand the lock).
        let identity = match file
            .metadata()
            .and_then(|metadata| identity_of(&file, &metadata))
        {
            Ok(identity) => identity,
            Err(source) => return Err(DestinationLockError::IdentityUnverified { path, source }),
        };
        if let Err(error) = file
            .write_all(owner_contents.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            remove_if_same_file(&path, identity.as_ref());
            return Err(error.into());
        }
        Ok(Self {
            path,
            file: Some(file),
            owner_contents,
            identity,
        })
    }
}

impl Drop for DestinationLock {
    fn drop(&mut self) {
        drop(self.file.take());
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        let Ok(contents) = fs::read_to_string(&self.path) else {
            return;
        };
        // Platforms without file identity fall back to exact-contents ownership.
        let same_file = self.identity.as_ref().is_none_or(|identity| {
            FileIdentity::from_path(&self.path, &metadata) == Some(*identity)
        });
        if !metadata.file_type().is_file() || !same_file || contents != self.owner_contents {
            return;
        }
        let _ = fs::remove_file(&self.path);
    }
}

/// Safe acquisition failures. Stale locks are reported, never reclaimed.
#[derive(Debug, Error)]
pub enum DestinationLockError {
    #[error("destination is locked by live process {holder_pid}")]
    AlreadyHeld { holder_pid: u32 },
    #[error("destination has stale lock from dead process {holder_pid}")]
    StaleLock { holder_pid: u32 },
    #[error("destination lock exists but its owner PID is unknown")]
    HolderUnknown,
    #[error("cannot verify destination lock owner {holder_pid}: {source}")]
    LivenessUnknown {
        holder_pid: u32,
        #[source]
        source: io::Error,
    },
    #[error("destination lock I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error(
        "cannot verify identity of newly created destination lock {}; file left in place for inspection: {source}",
        path.display()
    )]
    IdentityUnverified {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Reads a held file's identity. `Err` is a real OS failure; `Ok(None)` means
/// the platform has no file identity.
type FileIdentityFn = fn(&File, &fs::Metadata) -> io::Result<Option<FileIdentity>>;

fn existing_lock_error(path: &Path) -> DestinationLockError {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => return DestinationLockError::Io(error),
    };
    let Some(pid) = contents
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pid| *pid > 0)
    else {
        return DestinationLockError::HolderUnknown;
    };
    match process_is_alive(pid) {
        Ok(true) => DestinationLockError::AlreadyHeld { holder_pid: pid },
        Ok(false) => DestinationLockError::StaleLock { holder_pid: pid },
        Err(source) => DestinationLockError::LivenessUnknown {
            holder_pid: pid,
            source,
        },
    }
}

fn next_owner_id(pid: u32) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed);
    format!("{pid}-{timestamp:x}-{sequence:x}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    index: u64,
}

impl FileIdentity {
    fn from_file(file: &File, metadata: &fs::Metadata) -> io::Result<Option<Self>> {
        #[cfg(unix)]
        {
            let _ = file;
            Ok(Self::from_metadata(metadata))
        }
        #[cfg(windows)]
        {
            let _ = metadata;
            Self::from_windows_file(file)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (file, metadata);
            Ok(None)
        }
    }

    /// Comparison-only lookup: an unreadable path is "not the same file".
    fn from_path(path: &Path, metadata: &fs::Metadata) -> Option<Self> {
        #[cfg(unix)]
        {
            let _ = path;
            Self::from_metadata(metadata)
        }
        #[cfg(windows)]
        {
            let _ = metadata;
            let file = File::open(path).ok()?;
            Self::from_windows_file(&file).ok().flatten()
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (path, metadata);
            None
        }
    }

    #[cfg(unix)]
    fn from_metadata(metadata: &fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    #[cfg(windows)]
    fn from_windows_file(file: &File) -> io::Result<Option<Self>> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };

        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `file.as_raw_handle()` comes from this live `File`, borrowed
        // through the call. `information` is zero-initialized by `Default`;
        // read its fields only after checking the returned `BOOL`.
        // Updated: 2026-11-08T15:11:03-07:00.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(Self {
            volume: information.dwVolumeSerialNumber,
            index: (u64::from(information.nFileIndexHigh) << 32)
                | u64::from(information.nFileIndexLow),
        }))
    }
}

fn remove_if_same_file(path: &Path, identity: Option<&FileIdentity>) {
    let (Some(identity), Ok(metadata)) = (identity, fs::symlink_metadata(path)) else {
        return;
    };
    if metadata.file_type().is_file() && FileIdentity::from_path(path, &metadata) == Some(*identity)
    {
        let _ = fs::remove_file(path);
    }
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> io::Result<bool> {
    const EPERM: i32 = 1;
    const ESRCH: i32 = 3;
    const EACCES: i32 = 13;

    #[link(name = "c")]
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    let pid = i32::try_from(pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PID exceeds platform range"))?;
    if unsafe { kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(EPERM | EACCES) => Ok(true),
        Some(ESRCH) => Ok(false),
        _ => Err(error),
    }
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> io::Result<bool> {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_INVALID_PARAMETER: i32 = 87;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "OpenProcess"]
        fn open_process(access: u32, inherit: i32, process_id: u32) -> Handle;
        #[link_name = "GetExitCodeProcess"]
        fn get_exit_code_process(process: Handle, exit_code: *mut u32) -> i32;
        #[link_name = "CloseHandle"]
        fn close_handle(handle: Handle) -> i32;
    }

    let handle = unsafe { open_process(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(ERROR_ACCESS_DENIED) => Ok(true),
            Some(ERROR_INVALID_PARAMETER) => Ok(false),
            _ => Err(error),
        };
    }
    let mut exit_code = 0;
    let result = unsafe { get_exit_code_process(handle, &mut exit_code) };
    let error = (result == 0).then(io::Error::last_os_error);
    unsafe { close_handle(handle) };
    match error {
        Some(error) => Err(error),
        None => Ok(exit_code == STILL_ACTIVE),
    }
}

#[cfg(not(any(unix, windows)))]
fn process_is_alive(_pid: u32) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process liveness is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::{fs, DestinationLock, DestinationLockError, FileIdentity, DELIVERY_LOCK_NAME};
    use std::path::Path;

    #[test]
    fn identity_failure_fails_acquisition_with_the_os_error_and_keeps_the_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let result = DestinationLock::acquire_with(directory.path(), |_, _| {
            Err(std::io::Error::other("injected identity failure"))
        });
        let lock_path = fs::canonicalize(directory.path())
            .unwrap()
            .join(DELIVERY_LOCK_NAME);
        match result {
            Err(DestinationLockError::IdentityUnverified { path, source }) => {
                assert_eq!(path, lock_path);
                assert!(source.to_string().contains("injected"));
            }
            other => panic!("expected IdentityUnverified, got {:?}", other.err()),
        }
        assert!(lock_path.exists(), "unverified artifact must be preserved");
    }

    #[test]
    fn missing_platform_identity_still_cleans_up_by_owner_contents() {
        let directory = tempfile::tempdir().unwrap();
        let lock_path = fs::canonicalize(directory.path())
            .unwrap()
            .join(DELIVERY_LOCK_NAME);
        let lock = DestinationLock::acquire_with(directory.path(), |_, _| Ok(None)).unwrap();
        assert!(lock_path.exists());
        drop(lock);
        assert!(
            !lock_path.exists(),
            "guard without identity must not strand the lock"
        );
    }

    #[test]
    fn identity_matches_hard_links_and_distinguishes_files() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original");
        let alias = directory.path().join("alias");
        let distinct = directory.path().join("distinct");
        fs::write(&original, b"same").unwrap();
        fs::hard_link(&original, &alias).unwrap();
        fs::write(&distinct, b"same").unwrap();

        let identity = |path: &Path| {
            let metadata = fs::symlink_metadata(path).unwrap();
            FileIdentity::from_path(path, &metadata).unwrap()
        };
        assert_eq!(identity(&original), identity(&alias));
        assert_ne!(identity(&original), identity(&distinct));
    }
}
