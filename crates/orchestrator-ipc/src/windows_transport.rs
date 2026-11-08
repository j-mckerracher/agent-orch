//! Windows named-pipe transport. Updated: 2026-11-08T14:48:33+00:00.
//! Pipe names derive from the canonical data directory. Pipes reject remote
//! clients and use the process token's default DACL. Every connection still
//! requires IPC token authentication.

use crate::transport::{IpcStream, TransportError};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY,
    ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, ERROR_SEM_TIMEOUT, GENERIC_READ, GENERIC_WRITE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, SetNamedPipeHandleState, WaitNamedPipeW,
    NMPWAIT_USE_DEFAULT_WAIT, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
};

const PIPE_BUFFER_SIZE: u32 = 64 * 1024;
const PIPE_MODE: u32 =
    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS;
const PIPE_WAIT_MILLISECONDS: u32 = 5_000;

pub(crate) struct Listener {
    name: Vec<u16>,
    pending: Option<File>,
    connected: bool,
}

impl Listener {
    pub(crate) fn prepare(data_dir: &Path) -> Result<(), TransportError> {
        if !fs::metadata(data_dir)?.is_dir() {
            return Err(TransportError::UnsafePath);
        }
        let name = pipe_name(data_dir);
        if unsafe { WaitNamedPipeW(name.as_ptr(), 1) } != 0 {
            return Err(TransportError::AlreadyRunning);
        }
        let error = unsafe { GetLastError() };
        if error == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else if error == ERROR_PIPE_BUSY || error == ERROR_SEM_TIMEOUT {
            Err(TransportError::AlreadyRunning)
        } else {
            Err(io::Error::from_raw_os_error(error as i32).into())
        }
    }

    pub(crate) fn bind(data_dir: &Path) -> Result<Self, TransportError> {
        let name = pipe_name(data_dir);
        let pending = create_pipe_instance(&name, true)?;
        Ok(Self {
            name,
            pending: Some(pending),
            connected: false,
        })
    }

    pub(crate) fn accept(&mut self) -> Result<Box<dyn IpcStream>, TransportError> {
        if self.pending.is_none() {
            self.pending = Some(create_pipe_instance(&self.name, !self.connected)?);
        }
        let file = self.pending.as_ref().expect("pipe instance exists");
        if unsafe { ConnectNamedPipe(file.as_raw_handle(), std::ptr::null_mut()) } == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_PIPE_LISTENING {
                return Err(TransportError::Io(io::ErrorKind::WouldBlock));
            }
            if error == ERROR_NO_DATA {
                self.pending.take();
                return Err(TransportError::Io(io::ErrorKind::WouldBlock));
            }
            if error != ERROR_PIPE_CONNECTED {
                return Err(pipe_error(error));
            }
        }
        self.connected = true;
        let file = self.pending.take().expect("connected pipe instance exists");
        Ok(Box::new(PipeStream {
            file,
            timeout: Duration::from_secs(2),
        }))
    }

    pub(crate) fn set_nonblocking(&self, _enabled: bool) -> Result<(), TransportError> {
        Ok(())
    }
}

fn create_pipe_instance(name: &[u16], first: bool) -> Result<File, TransportError> {
    let open_mode = PIPE_ACCESS_DUPLEX
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            open_mode,
            PIPE_MODE,
            PIPE_UNLIMITED_INSTANCES,
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            NMPWAIT_USE_DEFAULT_WAIT,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(pipe_error(unsafe { GetLastError() }));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn pipe_error(code: u32) -> TransportError {
    if code == ERROR_ACCESS_DENIED || code == ERROR_PIPE_BUSY {
        TransportError::AlreadyRunning
    } else {
        io::Error::from_raw_os_error(code as i32).into()
    }
}

fn pipe_name(data_dir: &Path) -> Vec<u16> {
    let path = fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let path = path.to_string_lossy().to_lowercase();
    let hash = path
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!(r"\\.\pipe\agent-orch-{hash:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

pub(crate) fn connect(data_dir: &Path) -> Result<Box<dyn IpcStream>, TransportError> {
    let name = pipe_name(data_dir);
    let deadline = Instant::now() + Duration::from_millis(u64::from(PIPE_WAIT_MILLISECONDS));
    loop {
        if unsafe { WaitNamedPipeW(name.as_ptr(), 100) } == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_FILE_NOT_FOUND && error != ERROR_SEM_TIMEOUT {
                return Err(pipe_error(error));
            }
            if Instant::now() >= deadline {
                return Err(TransportError::Io(io::ErrorKind::TimedOut));
            }
            continue;
        }

        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let error = unsafe { GetLastError() };
            if error == ERROR_PIPE_BUSY || error == ERROR_FILE_NOT_FOUND {
                continue;
            }
            return Err(pipe_error(error));
        }
        let file = unsafe { File::from_raw_handle(handle) };
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        if unsafe {
            SetNamedPipeHandleState(
                file.as_raw_handle(),
                &mode,
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(pipe_error(unsafe { GetLastError() }));
        }
        return Ok(Box::new(PipeStream {
            file,
            timeout: Duration::from_secs(2),
        }));
    }
}

struct PipeStream {
    file: File,
    timeout: Duration,
}

impl IpcStream for PipeStream {
    fn set_timeouts(&mut self, timeout: Duration) -> io::Result<()> {
        self.timeout = timeout;
        Ok(())
    }
}

impl Read for PipeStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.file.read(buffer) {
                Err(error) if is_pipe_would_block(&error) => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "pipe read timed out",
                        ));
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                result => return result,
            }
        }
    }
}

impl Write for PipeStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.file.write(buffer) {
                Ok(0) => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "pipe write timed out",
                        ));
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) if is_pipe_would_block(&error) => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "pipe write timed out",
                        ));
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn is_pipe_would_block(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock || error.raw_os_error() == Some(ERROR_NO_DATA as i32)
}

pub(crate) fn create_private_file(path: &Path) -> Result<(File, fs::Metadata), TransportError> {
    let file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)?;
    let metadata = file.metadata()?;
    Ok((file, metadata))
}

pub(crate) fn random_token() -> Result<[u8; 32], TransportError> {
    let mut token = [0u8; 32];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            token.as_mut_ptr(),
            token.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(TransportError::Io(io::ErrorKind::Other));
    }
    Ok(token)
}
