//! Exclusive process lock for one task-store data directory.
//!
//! `create_new` provides cross-process exclusion. A stale lock is reported but
//! never reclaimed because this process did not create its lock file. The
//! owner removes its own file on drop after confirming file identity.
//!
//! Updated: 2026-11-08T06:21:27+00:00.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

const LOCK_FILE_NAME: &str = ".orchestrator.lock";
static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(0);

/// Held while one engine owns reconciliation and scheduling for a data directory.
///
/// The PID is diagnostic only. `create_new` is the exclusion mechanism; PID
/// liveness is used only to distinguish stale locks from live owners.
pub struct DataDirectoryLock {
    path: PathBuf,
    file: Option<File>,
    owner_contents: String,
    identity: Option<FileIdentity>,
}

impl DataDirectoryLock {
    /// Creates `data_dir` if needed and acquires its exclusive process lock.
    pub fn acquire(data_dir: impl AsRef<Path>) -> Result<Self, DataDirectoryLockError> {
        fs::create_dir_all(data_dir.as_ref())?;
        let path = data_dir.as_ref().join(LOCK_FILE_NAME);
        let pid = std::process::id();
        let owner_contents = format!("pid={pid}\nowner={}\n", next_owner_id(pid));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(existing_lock_error(&path));
            }
            Err(error) => return Err(DataDirectoryLockError::Io(error)),
        };
        let identity = file
            .metadata()
            .ok()
            .and_then(|metadata| FileIdentity::from_metadata(&metadata));
        if let Err(error) = file
            .write_all(owner_contents.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            remove_if_same_file(&path, identity.as_ref());
            return Err(DataDirectoryLockError::Io(error));
        }
        Ok(Self {
            path,
            file: Some(file),
            owner_contents,
            identity,
        })
    }
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
        match (&self.identity, FileIdentity::from_metadata(metadata)) {
            (Some(expected), Some(actual)) => *expected == actual,
            _ => fs::read_to_string(&self.path)
                .map(|contents| contents == self.owner_contents)
                .unwrap_or(false),
        }
    }
}

/// Typed acquisition failures preserve holder identity when the lock file has it.
#[derive(Debug, Error)]
pub enum DataDirectoryLockError {
    #[error("data directory is locked by live process {holder_pid}")]
    AlreadyHeld { holder_pid: u32 },
    #[error("data-directory lock exists but its holder PID is unknown")]
    HolderUnknown,
    #[error("data directory has stale lock from dead process {holder_pid}")]
    StaleLock { holder_pid: u32 },
    #[error("cannot verify liveness of lock holder process {holder_pid}: {source}")]
    LivenessUnknown {
        holder_pid: u32,
        #[source]
        source: io::Error,
    },
    #[error("data-directory lock I/O failed: {0}")]
    Io(#[from] io::Error),
}

fn existing_lock_error(path: &Path) -> DataDirectoryLockError {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => return DataDirectoryLockError::Io(error),
    };
    let Some(pid) = contents
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|pid| pid.parse::<u32>().ok())
        .filter(|pid| *pid > 0)
    else {
        return DataDirectoryLockError::HolderUnknown;
    };
    match process_is_alive(pid) {
        Ok(true) => DataDirectoryLockError::AlreadyHeld { holder_pid: pid },
        Ok(false) => DataDirectoryLockError::StaleLock { holder_pid: pid },
        Err(source) => DataDirectoryLockError::LivenessUnknown {
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
    volume: Option<u32>,
    #[cfg(windows)]
    index: Option<u64>,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Option<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            let volume = metadata.volume_serial_number();
            let index = metadata.file_index();
            (volume.is_some() && index.is_some()).then_some(Self { volume, index })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = metadata;
            None
        }
    }
}

fn remove_if_same_file(path: &Path, identity: Option<&FileIdentity>) {
    let (Some(identity), Ok(metadata)) = (identity, fs::symlink_metadata(path)) else {
        return;
    };
    if metadata.file_type().is_file() && FileIdentity::from_metadata(&metadata) == Some(*identity) {
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
    // Signal zero probes PID existence without delivering a signal.
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
