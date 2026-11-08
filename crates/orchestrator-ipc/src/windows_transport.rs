//! Windows named-pipe IPC transport. Updated: 2026-11-08T16:51:50Z.
//!
//! Protected DACLs grant access only to the current process user's SID. Every
//! accepted stream enters the shared server session, which authenticates its
//! handshake before dispatching methods.

use crate::transport::{IpcStream, TransportError};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND,
    ERROR_INSUFFICIENT_BUFFER, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
    ERROR_PIPE_LISTENING, ERROR_SEM_TIMEOUT, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, SetNamedPipeHandleState, WaitNamedPipeW,
    NMPWAIT_USE_DEFAULT_WAIT, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const PIPE_BUFFER_SIZE: u32 = 64 * 1024;
const PIPE_MAX_INSTANCES: u32 = 16;
const PIPE_MODE: u32 =
    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS;
const PIPE_WAIT_MILLISECONDS: u32 = 5_000;

pub(crate) struct Listener {
    name: Vec<u16>,
    security: UserSecurity,
    pending: Option<File>,
    connected: bool,
}

impl Listener {
    pub(crate) fn prepare(data_dir: &Path) -> Result<(), TransportError> {
        if !fs::metadata(data_dir)?.is_dir() {
            return Err(TransportError::UnsafePath);
        }
        let name = pipe_name(data_dir)?;
        // SAFETY: `name` is NUL-terminated UTF-16 and remains live for the call.
        if unsafe { WaitNamedPipeW(name.as_ptr(), 1) } != 0 {
            return Err(TransportError::AlreadyRunning);
        }
        let error = last_error().raw_os_error().unwrap_or_default() as u32;
        if error == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else if error == ERROR_PIPE_BUSY || error == ERROR_SEM_TIMEOUT {
            Err(TransportError::AlreadyRunning)
        } else {
            Err(io::Error::from_raw_os_error(error as i32).into())
        }
    }

    pub(crate) fn bind(data_dir: &Path) -> Result<Self, TransportError> {
        let name = pipe_name(data_dir)?;
        let security = UserSecurity::new("GA")?;
        let pending = create_pipe_instance(&name, &security, true)?;
        Ok(Self {
            name,
            security,
            pending: Some(pending),
            connected: false,
        })
    }

    pub(crate) fn accept(&mut self) -> Result<Box<dyn IpcStream>, TransportError> {
        if self.pending.is_none() {
            self.pending = Some(create_pipe_instance(&self.name, &self.security, false)?);
        }
        let file = self.pending.as_ref().expect("pipe instance exists");
        // SAFETY: `file` owns a valid synchronous named-pipe handle; null OVERLAPPED
        // selects the synchronous operation supported by this nonblocking pipe.
        if unsafe { ConnectNamedPipe(file.as_raw_handle(), std::ptr::null_mut()) } == 0 {
            let error = last_error().raw_os_error().unwrap_or_default() as u32;
            if error == ERROR_PIPE_LISTENING || error == ERROR_NO_DATA {
                if error == ERROR_NO_DATA {
                    drop(self.pending.take());
                }
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

impl Drop for Listener {
    fn drop(&mut self) {
        drop(self.pending.take());
    }
}
fn create_pipe_instance(
    name: &[u16],
    security: &UserSecurity,
    first: bool,
) -> Result<File, TransportError> {
    let open_mode = PIPE_ACCESS_DUPLEX
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    let attributes = security.attributes();
    // SAFETY: `name` is NUL-terminated UTF-16; `attributes` points to a live
    // descriptor owned by `security`; all buffers remain valid for this call.
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            open_mode,
            PIPE_MODE,
            PIPE_MAX_INSTANCES,
            PIPE_BUFFER_SIZE,
            PIPE_BUFFER_SIZE,
            NMPWAIT_USE_DEFAULT_WAIT,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = last_error_code();
        if !first && error == ERROR_PIPE_BUSY {
            return Err(TransportError::Io(io::ErrorKind::WouldBlock));
        }
        return Err(pipe_error(error));
    }
    // SAFETY: successful CreateNamedPipeW returns a unique owned handle that
    // `File` closes exactly once.
    Ok(unsafe { File::from_raw_handle(handle) })
}

fn pipe_error(code: u32) -> TransportError {
    if code == ERROR_ACCESS_DENIED {
        TransportError::AlreadyRunning
    } else {
        io::Error::from_raw_os_error(code as i32).into()
    }
}

fn pipe_name(data_dir: &Path) -> Result<Vec<u16>, TransportError> {
    let path = fs::canonicalize(data_dir)?;
    let path = path.to_string_lossy().to_lowercase();
    let hash = path
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    Ok(format!(r"\\.\pipe\agent-orch-{hash:016x}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect())
}

pub(crate) fn connect(data_dir: &Path) -> Result<Box<dyn IpcStream>, TransportError> {
    let name = pipe_name(data_dir)?;
    let deadline = Instant::now() + Duration::from_millis(u64::from(PIPE_WAIT_MILLISECONDS));
    loop {
        // SAFETY: `name` is NUL-terminated UTF-16 and remains live for the call.
        if unsafe { WaitNamedPipeW(name.as_ptr(), 100) } == 0 {
            let error = last_error_code();
            if error != ERROR_FILE_NOT_FOUND && error != ERROR_SEM_TIMEOUT {
                return Err(pipe_error(error));
            }
            if Instant::now() >= deadline {
                return Err(TransportError::Io(io::ErrorKind::TimedOut));
            }
            continue;
        }

        // SAFETY: `name` is NUL-terminated UTF-16; null security attributes and
        // template handle are valid for opening an existing pipe instance.
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
            let error = last_error_code();
            if error == ERROR_PIPE_BUSY || error == ERROR_FILE_NOT_FOUND {
                continue;
            }
            return Err(pipe_error(error));
        }
        // SAFETY: successful CreateFileW returns a unique owned handle.
        let file = unsafe { File::from_raw_handle(handle) };
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        // SAFETY: `file` owns a valid pipe handle and `mode` points to a live
        // u32 value accepted by SetNamedPipeHandleState.
        if unsafe {
            SetNamedPipeHandleState(
                file.as_raw_handle(),
                &mode,
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::from_raw_os_error(last_error_code() as i32).into());
        }
        return Ok(Box::new(PipeStream {
            file,
            timeout: Duration::from_secs(2),
        }));
    }
}
struct UserSecurity {
    descriptor: LocalAllocation,
}

impl UserSecurity {
    fn new(access: &str) -> Result<Self, TransportError> {
        let sid = current_user_sid()?;
        let sddl = format!("D:P(A;;{access};;;{sid})");
        let sddl: Vec<u16> = std::ffi::OsStr::new(&sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated UTF-16 and `descriptor` is a valid
        // writable output pointer; the API allocates the returned descriptor.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_error().into());
        }
        if descriptor.is_null() {
            return Err(TransportError::Io(io::ErrorKind::InvalidData));
        }
        Ok(Self {
            descriptor: LocalAllocation(descriptor),
        })
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: 0,
        }
    }
}

struct LocalAllocation(*mut std::ffi::c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this pointer came from a Windows LocalAlloc-backed
            // conversion API and has not been freed before this Drop.
            let _ = unsafe { LocalFree(self.0) };
        }
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this is the owned token handle returned by
            // OpenProcessToken and is closed exactly once here.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn current_user_sid() -> Result<String, TransportError> {
    // SAFETY: GetCurrentProcess returns the documented pseudo-handle for this
    // process; OpenProcessToken does not take ownership of that pseudo-handle.
    let process = unsafe { GetCurrentProcess() };
    let mut token = std::ptr::null_mut();
    // SAFETY: `process` is a valid process pseudo-handle and `token` is a
    // writable output pointer for the returned owned token handle.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error().into());
    }
    if token.is_null() {
        return Err(TransportError::Io(io::ErrorKind::InvalidData));
    }
    let token = OwnedHandle(token);
    let mut required = 0u32;
    // SAFETY: null buffer with zero length is the documented size-query call;
    // `required` is a writable output pointer.
    let queried =
        unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut required) };
    if queried != 0 {
        return Err(TransportError::Io(io::ErrorKind::InvalidData));
    }
    let error = last_error_code();
    if error != ERROR_INSUFFICIENT_BUFFER || required < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err(io::Error::from_raw_os_error(error as i32).into());
    }
    let words =
        (required as usize + std::mem::size_of::<usize>() - 1) / std::mem::size_of::<usize>();
    let mut buffer = vec![0usize; words];
    let buffer_len = (buffer.len() * std::mem::size_of::<usize>()) as u32;
    let mut returned = 0u32;
    // SAFETY: `buffer` is initialized, suitably aligned, and large enough for
    // the required TOKEN_USER result; `returned` is a writable output pointer.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            buffer_len,
            &mut returned,
        )
    } == 0
    {
        return Err(last_error().into());
    }
    if returned < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err(TransportError::Io(io::ErrorKind::InvalidData));
    }
    // SAFETY: successful GetTokenInformation initialized a TOKEN_USER at the
    // aligned start of `buffer`, and `returned` covers that structure.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    if user.User.Sid.is_null() {
        return Err(TransportError::Io(io::ErrorKind::InvalidData));
    }
    let mut sid_text = std::ptr::null_mut();
    // SAFETY: SID pointer remains valid inside `buffer`; `sid_text` is a
    // writable output pointer for the allocated NUL-terminated SID string.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid_text) } == 0 {
        return Err(last_error().into());
    }
    if sid_text.is_null() {
        return Err(TransportError::Io(io::ErrorKind::InvalidData));
    }
    let _sid_text = LocalAllocation(sid_text.cast());
    // SAFETY: ConvertSidToStringSidW returned a live NUL-terminated UTF-16
    // string; the slice ends before its terminator and is copied immediately.
    let sid = unsafe {
        let mut length = 0usize;
        while *sid_text.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(sid_text, length))
    };
    Ok(sid)
}

fn last_error_code() -> u32 {
    // SAFETY: GetLastError reads this thread's last Win32 error value.
    unsafe { GetLastError() }
}

fn last_error() -> io::Error {
    io::Error::from_raw_os_error(last_error_code() as i32)
}
pub(crate) fn create_private_file(path: &Path) -> Result<(File, fs::Metadata), TransportError> {
    let security = UserSecurity::new("FA")?;
    let attributes = security.attributes();
    let path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `path` is NUL-terminated UTF-16 and `attributes` points to a
    // live protected descriptor; CREATE_NEW applies its DACL before bytes exist.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error().into());
    }
    // SAFETY: successful CreateFileW returns a unique owned file handle.
    let file = unsafe { File::from_raw_handle(handle) };
    let metadata = file.metadata()?;
    Ok((file, metadata))
}

pub(crate) fn random_token() -> Result<[u8; 32], TransportError> {
    let mut token = [0u8; 32];
    // SAFETY: `token` is writable for its full length; null algorithm selects
    // the documented system-preferred RNG when the corresponding flag is set.
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
