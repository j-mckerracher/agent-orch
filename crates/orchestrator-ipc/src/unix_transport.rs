use crate::transport::{IpcStream, TransportError};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
pub(crate) struct Listener {
    listener: UnixListener,
    path: PathBuf,
    identity: Identity,
}

#[cfg(unix)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct Identity {
    dev: u64,
    ino: u64,
    uid: u32,
}

#[cfg(unix)]
impl Identity {
    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            uid: metadata.uid(),
        }
    }
}

#[cfg(unix)]
impl Listener {
    pub(crate) fn prepare(data_dir: &Path) -> Result<(), TransportError> {
        prepare_socket_path(data_dir).map(|_| ())
    }

    pub(crate) fn bind(data_dir: &Path) -> Result<Self, TransportError> {
        let (path, owner) = prepare_socket_path(data_dir)?;
        let listener = UnixListener::bind(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::AddrInUse {
                TransportError::AlreadyRunning
            } else {
                error.into()
            }
        })?;
        let initial = fs::symlink_metadata(&path)?;
        let owned = Self {
            listener,
            path,
            identity: Identity::of(&initial),
        };
        if !initial.file_type().is_socket() || initial.uid() != owner {
            return Err(TransportError::UnsafePath);
        }
        fs::set_permissions(&owned.path, fs::Permissions::from_mode(0o600))?;
        let metadata = fs::symlink_metadata(&owned.path)?;
        if !metadata.file_type().is_socket()
            || metadata.uid() != owner
            || metadata.mode() & 0o777 != 0o600
        {
            return Err(TransportError::UnsafePath);
        }
        Ok(owned)
    }

    pub(crate) fn accept(&mut self) -> Result<Box<dyn IpcStream>, TransportError> {
        let (stream, _) = self.listener.accept().map_err(report_transient)?;
        stream.set_nonblocking(false)?;
        Ok(Box::new(stream))
    }

    pub(crate) fn set_nonblocking(&self, enabled: bool) -> Result<(), TransportError> {
        self.listener.set_nonblocking(enabled).map_err(Into::into)
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const ENOBUFS: i32 = 105;
#[cfg(target_os = "macos")]
const ENOBUFS: i32 = 55;
#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
const ENOBUFS: i32 = -1; // unknown errno on this target: stays fatal

/// Accept failures that describe the connection or a momentary resource
/// shortage, not the listener. EMFILE/ENFILE/ENOBUFS have no stable
/// `ErrorKind`, so they are matched by errno. Anything else (EBADF, EINVAL,
/// ENOTSOCK, ...) means the listener is unusable and stays fatal.
#[cfg(unix)]
fn report_transient(error: io::Error) -> TransportError {
    const ENFILE: i32 = 23;
    const EMFILE: i32 = 24;
    let transient = matches!(
        error.kind(),
        io::ErrorKind::ConnectionAborted | io::ErrorKind::Interrupted | io::ErrorKind::OutOfMemory
    ) || matches!(error.raw_os_error(), Some(ENFILE | EMFILE | ENOBUFS));
    if !transient {
        return error.into();
    }
    eprintln!("orchestrator-ipc: accept failed, retrying: {error}");
    // The server polls on WouldBlock with a short sleep, which also keeps a
    // still-queued connection from spinning the loop while descriptors are out.
    TransportError::Io(io::ErrorKind::WouldBlock)
}

#[cfg(unix)]
fn prepare_socket_path(data_dir: &Path) -> Result<(PathBuf, u32), TransportError> {
    let owner = fs::metadata(data_dir)?.uid();
    let dir = data_dir.join("ipc");
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_dir() && meta.uid() == owner => {
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(_) => return Err(TransportError::UnsafePath),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700).create(&dir)?;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        Err(error) => return Err(error.into()),
    }
    let directory = fs::symlink_metadata(&dir)?;
    if !directory.file_type().is_dir()
        || directory.uid() != owner
        || directory.mode() & 0o777 != 0o700
    {
        return Err(TransportError::UnsafePath);
    }
    let path = dir.join("daemon.sock");
    if fs::symlink_metadata(&path).is_ok() {
        reclaim_stale(&path, owner)?;
    }
    Ok((path, owner))
}

#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        if let Ok(meta) = fs::symlink_metadata(&self.path) {
            if meta.file_type().is_socket() && Identity::of(&meta) == self.identity {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(unix)]
fn reclaim_stale(path: &Path, owner: u32) -> Result<(), TransportError> {
    let observed = fs::symlink_metadata(path).map_err(TransportError::from)?;
    if !observed.file_type().is_socket() || observed.uid() != owner {
        return Err(TransportError::UnsafePath);
    }
    match UnixStream::connect(path) {
        Ok(_) => return Err(TransportError::AlreadyRunning),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(TransportError::UnsafePath);
        }
        Err(error) => return Err(error.into()),
    }
    let current = fs::symlink_metadata(path)?;
    if !current.file_type().is_socket() || Identity::of(&current) != Identity::of(&observed) {
        return Err(TransportError::UnsafePath);
    }
    fs::remove_file(path)?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_timeouts(
    stream: &std::os::unix::net::UnixStream,
    timeout: Duration,
) -> io::Result<()> {
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))
}

#[cfg(unix)]
impl IpcStream for UnixStream {
    fn set_timeouts(&mut self, timeout: Duration) -> io::Result<()> {
        set_timeouts(self, timeout)
    }
}

#[cfg(unix)]
pub(crate) fn connect(data_dir: &Path) -> Result<Box<dyn IpcStream>, TransportError> {
    Ok(Box::new(UnixStream::connect(
        data_dir.join("ipc/daemon.sock"),
    )?))
}

/// A refused or missing socket already fails at once on Unix.
#[cfg(unix)]
pub(crate) fn connect_fail_fast(data_dir: &Path) -> Result<Box<dyn IpcStream>, TransportError> {
    connect(data_dir)
}

#[cfg(unix)]
pub(crate) fn create_private_file(path: &Path) -> Result<(File, fs::Metadata), TransportError> {
    let file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let metadata = file.metadata()?;
    Ok((file, metadata))
}

#[cfg(unix)]
pub(crate) fn random_token() -> Result<[u8; 32], TransportError> {
    let mut token = [0u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut token)?;
    Ok(token)
}

#[cfg(test)]
#[path = "timeout_tests.rs"]
mod tests;
