use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum TransportError {
    #[error("local IPC path is owned by another process or has ambiguous state")]
    UnsafePath,
    #[error("local IPC listener already exists")]
    AlreadyRunning,
    #[error("local IPC transport I/O failed: {0}")]
    Io(io::ErrorKind),
}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

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
    pub(crate) fn bind(data_dir: &Path) -> Result<Self, TransportError> {
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

    pub(crate) fn accept(&self) -> Result<UnixStream, TransportError> {
        let (stream, _) = self.listener.accept()?;
        stream.set_nonblocking(false)?;
        Ok(stream)
    }
    pub(crate) fn set_nonblocking(&self, enabled: bool) -> Result<(), TransportError> {
        self.listener.set_nonblocking(enabled).map_err(Into::into)
    }
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
