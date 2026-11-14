use std::io::{self, Read, Write};
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

pub(crate) trait IpcStream: Read + Write + Send {
    fn set_timeouts(&mut self, timeout: Duration) -> io::Result<()>;
}

#[cfg(unix)]
pub(crate) use crate::unix_transport::{
    connect, connect_fail_fast, create_private_file, random_token, Listener,
};
#[cfg(windows)]
pub(crate) use crate::windows_transport::{
    connect, connect_fail_fast, create_private_file, random_token, Listener,
};
