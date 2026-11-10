//! Exclusive process lock for one task-store data directory.
//!
//! `create_new` provides exclusion. Records include PID, process start
//! identity, and token. Takeovers serialize on the observed file, rename stale
//! locks into unique temporary directories, and remove only token-verified
//! files. Dead legacy `owner=` records remain readable; live ones fail closed.
//! Updated: 2026-11-08T11:24:15+00:00.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use thiserror::Error;

const LOCK_FILE_NAME: &str = ".orchestrator.lock";
const MAX_ACQUIRE_ATTEMPTS: usize = 128;
const RETRY_DELAY: Duration = Duration::from_millis(10);
static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(0);

/// Held while one engine owns reconciliation and scheduling for a data directory.
///
/// Records created by this version include process start identity. Legacy
/// `owner=` records remain readable; live legacy owners fail closed.
pub struct DataDirectoryLock {
    path: PathBuf,
    file: Option<File>,
    owner_contents: String,
    identity: Option<FileIdentity>,
}

impl DataDirectoryLock {
    /// Creates `data_dir` owner-only (an existing one loses other users'
    /// access) and acquires its exclusive process lock.
    pub fn acquire(data_dir: impl AsRef<Path>) -> Result<Self, DataDirectoryLockError> {
        orchestrator_private_fs::create_private_dir(data_dir.as_ref())?;
        let path = data_dir.as_ref().join(LOCK_FILE_NAME);
        let pid = std::process::id();
        let start_identity = process_start_identity(pid)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "current process vanished"))?;
        let token = format!("{start_identity}-{}", next_owner_id(pid));
        let owner_contents = format!("pid={pid}\nstart={start_identity}\ntoken={token}\n");
        let mut last_holder_pid = pid;

        for _ in 0..MAX_ACQUIRE_ATTEMPTS {
            match create_lock(&path, &owner_contents, FileIdentity::from_file) {
                Ok(lock) => return Ok(lock),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    match take_over_existing(&path, &owner_contents, pid, FileIdentity::from_file)?
                    {
                        TakeoverOutcome::Acquired(lock) => return Ok(lock),
                        TakeoverOutcome::Retry { holder_pid } => {
                            last_holder_pid = holder_pid;
                            thread::sleep(RETRY_DELAY);
                        }
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(DataDirectoryLockError::TakeoverRace {
            holder_pid: last_holder_pid,
        })
    }
}

fn create_lock(
    path: &Path,
    contents: &str,
    identity_of: FileIdentityFn,
) -> io::Result<DataDirectoryLock> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    // On Windows the new file starts with an inherited, unprotected DACL. Make
    // it owner-only before anything is written; a lock that cannot be
    // protected is removed rather than left behind with a wider ACL.
    if let Err(error) = orchestrator_private_fs::restrict_to_owner(path) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    // The new file is unverified if its identity cannot be read or the
    // platform has none: fail with the real cause and leave the file in place
    // for inspection. A guard is only ever returned with a verified identity.
    let identity = file
        .metadata()
        .and_then(|metadata| identity_of(&file, &metadata))
        .and_then(|identity| {
            identity.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "file identity is unsupported on this platform",
                )
            })
        })?;
    if let Err(error) = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        remove_if_same_file(path, Some(&identity));
        return Err(error);
    }
    Ok(DataDirectoryLock {
        path: path.to_path_buf(),
        file: Some(file),
        owner_contents: contents.to_owned(),
        identity: Some(identity),
    })
}

impl Drop for DataDirectoryLock {
    fn drop(&mut self) {
        drop(self.file.take());
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if !metadata.file_type().is_file() || !self.owns_file(&metadata) {
            return;
        }
        let Ok(contents) = fs::read_to_string(&self.path) else {
            return;
        };
        if contents == self.owner_contents {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl DataDirectoryLock {
    fn owns_file(&self, metadata: &fs::Metadata) -> bool {
        // Ownership is proven by file identity only; an unreadable identity
        // leaves the file in place rather than guessing from its contents.
        matches!(
            (&self.identity, FileIdentity::from_path(&self.path, metadata)),
            (Some(expected), Some(actual)) if *expected == actual
        )
    }
}

/// Typed acquisition failures retain the holder identity when it is available.
#[derive(Debug, Error)]
pub enum DataDirectoryLockError {
    #[error("data directory is locked by live process {holder_pid}")]
    AlreadyHeld { holder_pid: u32 },
    #[error("legacy lock from live process {holder_pid} has no start identity; refusing takeover")]
    LegacyLockHeld { holder_pid: u32 },
    #[error("data-directory lock exists but its holder PID is unknown")]
    HolderUnknown,
    #[error("stale lock from process {holder_pid} has no verifiable token; refusing takeover")]
    StaleLock { holder_pid: u32 },
    #[error("lock changed during takeover; holder process {holder_pid} must be retried")]
    TakeoverRace { holder_pid: u32 },
    #[error("cannot verify process start identity for holder {holder_pid}: {source}")]
    LivenessUnknown {
        holder_pid: u32,
        #[source]
        source: io::Error,
    },
    #[error("data-directory lock I/O failed: {0}")]
    Io(#[from] io::Error),
}

enum TakeoverOutcome {
    Acquired(DataDirectoryLock),
    Retry { holder_pid: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockRecord {
    pid: u32,
    start_identity: Option<String>,
    token: Option<String>,
}

fn parse_lock_record(contents: &str) -> Option<LockRecord> {
    let mut pid = None;
    let mut start_identity = None;
    let mut token = None;
    let mut old_owner = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("pid=") {
            if pid.replace(value.to_owned()).is_some() {
                return None;
            }
        } else if let Some(value) = line.strip_prefix("start=") {
            if start_identity.replace(value.to_owned()).is_some() {
                return None;
            }
        } else if let Some(value) = line.strip_prefix("token=") {
            if token.replace(value.to_owned()).is_some() {
                return None;
            }
        } else if let Some(value) = line.strip_prefix("owner=") {
            if old_owner.replace(value.to_owned()).is_some() {
                return None;
            }
        }
    }
    if token.is_some() && old_owner.is_some() {
        return None;
    }
    let pid = pid?.parse::<u32>().ok().filter(|pid| *pid > 0)?;
    if start_identity.as_deref() == Some("") {
        return None;
    }
    let token = token.or(old_owner);
    if token.as_deref() == Some("") {
        return None;
    }
    Some(LockRecord {
        pid,
        start_identity,
        token,
    })
}

fn read_lock_record(file: &mut File) -> io::Result<Option<LockRecord>> {
    file.seek(SeekFrom::Start(0))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(parse_lock_record(&contents))
}

fn path_matches_file(path: &Path, identity: Option<FileIdentity>) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    metadata.file_type().is_file()
        && identity.is_some()
        && FileIdentity::from_path(path, &metadata) == identity
}

/// Reads a held file's identity. `Err` is a real OS failure and must surface;
/// `Ok(None)` means the platform has no identity.
type FileIdentityFn = fn(&File, &fs::Metadata) -> io::Result<Option<FileIdentity>>;

fn take_over_existing(
    path: &Path,
    owner_contents: &str,
    owner_pid: u32,
    identity_of: FileIdentityFn,
) -> Result<TakeoverOutcome, DataDirectoryLockError> {
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => return Err(DataDirectoryLockError::HolderUnknown),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(TakeoverOutcome::Retry {
                holder_pid: owner_pid,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let mut file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(TakeoverOutcome::Retry {
                holder_pid: owner_pid,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let file_metadata = file.metadata()?;
    let identity = identity_of(&file, &file_metadata)?;
    let path_identity = match FileIdentity::try_from_path(path, &path_metadata) {
        Ok(identity) => identity,
        // The path vanished or is mid-delete (Windows reports delete-pending
        // as access denied): another takeover is in flight, so retry.
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) =>
        {
            return Ok(TakeoverOutcome::Retry {
                holder_pid: owner_pid,
            });
        }
        Err(error) => return Err(error.into()),
    };
    if identity.is_none() || path_identity != identity {
        return Ok(TakeoverOutcome::Retry {
            holder_pid: owner_pid,
        });
    }
    let observed = read_lock_record(&mut file)?.ok_or(DataDirectoryLockError::HolderUnknown)?;
    if !try_lock_file(&file)? {
        return Ok(TakeoverOutcome::Retry {
            holder_pid: observed.pid,
        });
    }
    if !path_matches_file(path, identity) {
        return Ok(TakeoverOutcome::Retry {
            holder_pid: observed.pid,
        });
    }
    let current = read_lock_record(&mut file)?.ok_or(DataDirectoryLockError::HolderUnknown)?;
    if current != observed {
        return Ok(TakeoverOutcome::Retry {
            holder_pid: current.pid,
        });
    }

    let current_start = process_start_identity(current.pid).map_err(|source| {
        DataDirectoryLockError::LivenessUnknown {
            holder_pid: current.pid,
            source,
        }
    })?;
    match (current.start_identity.as_deref(), current_start.as_deref()) {
        (Some(expected), Some(actual)) if expected == actual => {
            return Err(DataDirectoryLockError::AlreadyHeld {
                holder_pid: current.pid,
            });
        }
        (None, Some(_)) => {
            return Err(DataDirectoryLockError::LegacyLockHeld {
                holder_pid: current.pid,
            });
        }
        _ => {}
    }
    let Some(token) = current.token.as_deref() else {
        return Err(DataDirectoryLockError::StaleLock {
            holder_pid: current.pid,
        });
    };

    let temporary_directory = make_takeover_directory(path, owner_pid)?;
    let moved_path = temporary_directory.join("stale-lock");
    if let Err(error) = fs::rename(path, &moved_path) {
        let _ = fs::remove_dir(&temporary_directory);
        if error.kind() == io::ErrorKind::NotFound {
            return Ok(TakeoverOutcome::Retry {
                holder_pid: current.pid,
            });
        }
        return Err(error.into());
    }
    let moved_metadata = fs::symlink_metadata(&moved_path);
    let moved_record = fs::read_to_string(&moved_path)
        .ok()
        .and_then(|contents| parse_lock_record(&contents));
    let verified = moved_metadata.is_ok_and(|metadata| {
        metadata.file_type().is_file()
            && FileIdentity::from_path(&moved_path, &metadata) == identity
    }) && moved_record
        .as_ref()
        .is_some_and(|record| record.token.as_deref() == Some(token));
    if !verified {
        let holder_pid = moved_record.map_or(current.pid, |record| record.pid);
        match fs::hard_link(&moved_path, path) {
            Ok(()) => return Ok(TakeoverOutcome::Retry { holder_pid }),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Ok(TakeoverOutcome::Retry { holder_pid });
            }
            Err(_) => return Err(DataDirectoryLockError::TakeoverRace { holder_pid }),
        }
    }

    drop(file);
    fs::remove_file(&moved_path)?;
    let _ = fs::remove_dir(&temporary_directory);
    match create_lock(path, owner_contents, identity_of) {
        Ok(lock) => Ok(TakeoverOutcome::Acquired(lock)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(TakeoverOutcome::Retry {
            holder_pid: current.pid,
        }),
        Err(error) => Err(error.into()),
    }
}

fn make_takeover_directory(path: &Path, owner_pid: u32) -> io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for _ in 0..MAX_ACQUIRE_ATTEMPTS {
        let candidate = parent.join(format!(
            "{LOCK_FILE_NAME}.takeover-{}",
            next_owner_id(owner_pid)
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "cannot allocate unique stale-lock name",
    ))
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

    /// Comparison-only lookup: an unreadable path is simply "not the same file".
    fn from_path(path: &Path, metadata: &fs::Metadata) -> Option<Self> {
        Self::try_from_path(path, metadata).ok().flatten()
    }

    fn try_from_path(path: &Path, metadata: &fs::Metadata) -> io::Result<Option<Self>> {
        #[cfg(unix)]
        {
            let _ = path;
            Ok(Self::from_metadata(metadata))
        }
        #[cfg(windows)]
        {
            let _ = metadata;
            let file = File::open(path)?;
            Self::from_windows_file(&file)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (path, metadata);
            Ok(None)
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
fn try_lock_file(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;

    #[link(name = "c")]
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    // SAFETY: `file` owns this descriptor for the duration of the advisory lock.
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(windows)]
fn try_lock_file(file: &File) -> io::Result<bool> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    // Windows byte-range locks are mandatory: while one handle locks bytes, every
    // other handle's read of them fails with `ERROR_LOCK_VIOLATION`. Takeover
    // reads the record through other handles (a contender, the renamed file), so
    // lock a one-byte range far beyond the record's end (4 GiB); locking past
    // EOF is allowed. Unix `flock` is advisory and needs no such range.
    const LOCK_OFFSET_HIGH: u32 = 1;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "LockFile"]
        fn lock_file(
            handle: *mut c_void,
            offset_low: u32,
            offset_high: u32,
            length_low: u32,
            length_high: u32,
        ) -> i32;
    }

    // SAFETY: `file` owns this handle and the byte range is valid for its lock.
    if unsafe { lock_file(file.as_raw_handle().cast(), 0, LOCK_OFFSET_HIGH, 1, 0) } != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(not(any(unix, windows)))]
fn try_lock_file(_file: &File) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "file locking is unsupported on this platform",
    ))
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct ProcBsdInfo {
    _flags: u32,
    _status: u32,
    _xstatus: u32,
    pid: u32,
    _ppid: u32,
    _uid: u32,
    _gid: u32,
    _ruid: u32,
    _rgid: u32,
    _svuid: u32,
    _svgid: u32,
    _reserved: u32,
    _command: [u8; 16],
    _name: [u8; 32],
    _open_files: u32,
    _process_group: u32,
    _job_control: u32,
    _terminal_device: u32,
    _terminal_process_group: u32,
    _nice: i32,
    start_seconds: u64,
    start_microseconds: u64,
}

#[cfg(target_os = "macos")]
fn process_is_alive(pid: u32) -> io::Result<bool> {
    const EPERM: i32 = 1;
    const ESRCH: i32 = 3;
    let pid = i32::try_from(pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PID exceeds platform range"))?;
    #[link(name = "c")]
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // SAFETY: signal zero probes this PID without delivering a signal.
    if unsafe { kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(EPERM) => Ok(true),
        Some(ESRCH) => Ok(false),
        _ => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn process_start_identity(pid: u32) -> io::Result<Option<String>> {
    if !process_is_alive(pid)? {
        return Ok(None);
    }
    const PROC_PIDTBSDINFO: i32 = 3;
    let os_pid = i32::try_from(pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PID exceeds platform range"))?;
    let size = i32::try_from(std::mem::size_of::<ProcBsdInfo>())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "process info too large"))?;
    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            argument: u64,
            buffer: *mut std::ffi::c_void,
            buffer_size: i32,
        ) -> i32;
    }
    let mut info = std::mem::MaybeUninit::<ProcBsdInfo>::uninit();
    // SAFETY: `proc_pidinfo` writes `ProcBsdInfo`; read it only after full-size success.
    let received =
        unsafe { proc_pidinfo(os_pid, PROC_PIDTBSDINFO, 0, info.as_mut_ptr().cast(), size) };
    if received != size {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    if info.pid != pid {
        return Ok(None);
    }
    let start = info
        .start_seconds
        .checked_mul(1_000_000)
        .and_then(|seconds| seconds.checked_add(info.start_microseconds))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "start time overflow"))?;
    Ok(Some(format!("macos:{start}")))
}

#[cfg(target_os = "linux")]
fn process_start_identity(pid: u32) -> io::Result<Option<String>> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let (recorded_pid, rest) = stat
        .split_once(' ')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process stat"))?;
    if recorded_pid.parse::<u32>().ok() != Some(pid) {
        return Ok(None);
    }
    let mut fields = rest
        .rsplit_once(')')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process stat"))?
        .1
        .split_whitespace();
    if fields
        .next()
        .is_some_and(|state| matches!(state, "Z" | "X"))
    {
        return Ok(None);
    }
    let start_ticks = fields
        .nth(18)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing process start time"))?;
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    Ok(Some(format!("linux:{}:{start_ticks}", boot_id.trim())))
}
#[cfg(windows)]
fn process_start_identity(pid: u32) -> io::Result<Option<String>> {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_INVALID_PARAMETER: i32 = 87;

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "OpenProcess"]
        fn open_process(access: u32, inherit: i32, process_id: u32) -> Handle;
        #[link_name = "GetProcessTimes"]
        fn get_process_times(
            process: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        #[link_name = "GetExitCodeProcess"]
        fn get_exit_code_process(process: Handle, exit_code: *mut u32) -> i32;
        #[link_name = "CloseHandle"]
        fn close_handle(handle: Handle) -> i32;
    }

    let handle = unsafe { open_process(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let mut creation = FileTime::default();
    let mut exit = FileTime::default();
    let mut kernel = FileTime::default();
    let mut user = FileTime::default();
    let times_result =
        unsafe { get_process_times(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    if times_result == 0 {
        let error = io::Error::last_os_error();
        unsafe { close_handle(handle) };
        return Err(error);
    }
    let mut exit_code = 0;
    let exit_result = unsafe { get_exit_code_process(handle, &mut exit_code) };
    let error = (exit_result == 0).then(io::Error::last_os_error);
    unsafe { close_handle(handle) };
    if let Some(error) = error {
        return Err(error);
    }
    if exit_code != STILL_ACTIVE {
        return Ok(None);
    }
    let creation = (u64::from(creation.high) << 32) | u64::from(creation.low);
    Ok(Some(format!("windows:{creation}")))
}

#[cfg(not(any(unix, windows)))]
fn process_start_identity(_pid: u32) -> io::Result<Option<String>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "process start identity is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_file_is_owner_only_when_created_and_when_taken_over() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let path = data.join(LOCK_FILE_NAME);

        let lock = DataDirectoryLock::acquire(&data).unwrap();
        assert!(orchestrator_private_fs::is_owner_only(&path).unwrap());
        drop(lock);

        // A stale lock written without protection (inherited ACL on Windows,
        // the process umask on Unix) is replaced by an owner-only one.
        fs::write(&path, format!("pid={}\nstart=x\ntoken=t\n", i32::MAX)).unwrap();
        let _lock = DataDirectoryLock::acquire(&data).unwrap();
        assert!(orchestrator_private_fs::is_owner_only(&path).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn acquire_creates_a_private_directory_and_lock_and_tightens_an_existing_one() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let root = tempfile::tempdir().unwrap();

        let fresh = root.path().join("fresh/data");
        let lock = DataDirectoryLock::acquire(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o700);
        assert_eq!(mode(&fresh.join(LOCK_FILE_NAME)), 0o600);
        drop(lock);

        let existing = root.path().join("existing");
        fs::create_dir(&existing).unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o755)).unwrap();
        let _lock = DataDirectoryLock::acquire(&existing).unwrap();
        assert_eq!(mode(&existing), 0o700);
    }

    #[test]
    fn range_lock_does_not_block_other_handles_reading_the_record() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(LOCK_FILE_NAME);
        fs::write(&path, "pid=1\n").unwrap();
        let locked = File::open(&path).unwrap();
        assert!(try_lock_file(&locked).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "pid=1\n");
        let contender = File::open(&path).unwrap();
        assert!(!try_lock_file(&contender).unwrap());
    }

    fn failing_identity(_: &File, _: &fs::Metadata) -> io::Result<Option<FileIdentity>> {
        Err(io::Error::other("injected identity failure"))
    }

    fn stale_lock(directory: &Path) -> (PathBuf, &'static str) {
        let path = directory.join(LOCK_FILE_NAME);
        let record = "pid=4294967295\nstart=x\ntoken=t\n";
        fs::write(&path, record).unwrap();
        (path, record)
    }

    #[test]
    fn identity_lookup_failure_is_an_io_error_not_a_retry_and_preserves_the_lock() {
        let directory = tempfile::tempdir().unwrap();
        let (path, record) = stale_lock(directory.path());
        let result = take_over_existing(&path, "pid=1\n", 1, failing_identity);
        assert!(
            matches!(result, Err(DataDirectoryLockError::Io(ref error)) if error.to_string().contains("injected")),
            "{:?}",
            result.err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), record);
    }

    #[test]
    fn fresh_creation_identity_failure_is_an_io_error_and_keeps_the_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(LOCK_FILE_NAME);
        let result = create_lock(&path, "pid=1\n", failing_identity);
        assert!(
            matches!(result, Err(ref error) if error.to_string().contains("injected")),
            "{:?}",
            result.err()
        );
        assert!(path.exists(), "unverified artifact must be preserved");
    }

    #[test]
    fn fresh_creation_without_platform_identity_fails_explicitly_and_keeps_the_artifact() {
        fn none(_: &File, _: &fs::Metadata) -> io::Result<Option<FileIdentity>> {
            Ok(None)
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(LOCK_FILE_NAME);
        let error = create_lock(&path, "pid=1\n", none).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert!(path.exists());
    }

    #[test]
    fn verified_creation_is_cleaned_up_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(LOCK_FILE_NAME);
        let lock = create_lock(&path, "pid=1\n", FileIdentity::from_file).unwrap();
        assert!(path.exists());
        drop(lock);
        assert!(!path.exists());
    }

    #[test]
    fn platform_without_identity_still_retries_and_preserves_the_lock() {
        fn none(_: &File, _: &fs::Metadata) -> io::Result<Option<FileIdentity>> {
            Ok(None)
        }
        let directory = tempfile::tempdir().unwrap();
        let (path, record) = stale_lock(directory.path());
        let result = take_over_existing(&path, "pid=1\n", 1, none);
        assert!(matches!(result, Ok(TakeoverOutcome::Retry { .. })));
        assert_eq!(fs::read_to_string(&path).unwrap(), record);
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
