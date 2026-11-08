use crate::protocol::{Capability, Handshake};
use crate::transport::{create_private_file, random_token, TransportError};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use subtle::ConstantTimeEq;
use thiserror::Error;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Debug, Error)]
pub(crate) enum AuthError {
    #[error("authentication failed")]
    Failed,
    #[error("requested capability is unavailable")]
    UnsupportedCapability,
    #[error("no common protocol version")]
    UnsupportedVersion,
    #[error("token storage has unsafe ownership or permissions")]
    UnsafeTokenFile,
    #[error("authentication I/O failed: {0}")]
    Io(io::ErrorKind),
    #[error(transparent)]
    Transport(#[from] TransportError),
}

pub(crate) struct Secret {
    bytes: [u8; 32],
    path: PathBuf,
    #[cfg(unix)]
    identity: (u64, u64, u32),
}
impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret([REDACTED])")
    }
}

impl Secret {
    pub(crate) fn create(data_dir: &Path) -> Result<Self, AuthError> {
        let directory = fs::metadata(data_dir).map_err(|error| AuthError::Io(error.kind()))?;
        if !directory.is_dir() {
            return Err(AuthError::UnsafeTokenFile);
        }
        #[cfg(unix)]
        if directory.mode() & 0o022 != 0 {
            return Err(AuthError::UnsafeTokenFile);
        }

        let bytes = random_token()?;
        let (temporary_path, mut file, metadata) = create_token_temp(data_dir)?;
        let _temporary = PendingFile(temporary_path.clone());
        #[cfg(unix)]
        if metadata.uid() != directory.uid() {
            return Err(AuthError::UnsafeTokenFile);
        }
        remove_stale_token(data_dir)?;

        let path = data_dir.join("daemon.token");
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| AuthError::Io(error.kind()))?;
        write!(file, "{}", encode(&bytes)).map_err(|error| AuthError::Io(error.kind()))?;
        file.sync_all()
            .map_err(|error| AuthError::Io(error.kind()))?;
        drop(file);
        fs::rename(&temporary_path, &path).map_err(|error| AuthError::Io(error.kind()))?;

        #[cfg(unix)]
        let identity = (metadata.dev(), metadata.ino(), metadata.uid());
        let secret = Self {
            bytes,
            path,
            #[cfg(unix)]
            identity,
        };
        let installed =
            fs::symlink_metadata(&secret.path).map_err(|error| AuthError::Io(error.kind()))?;
        if !installed.file_type().is_file() {
            return Err(AuthError::UnsafeTokenFile);
        }
        #[cfg(unix)]
        if (installed.dev(), installed.ino(), installed.uid()) != identity
            || installed.mode() & 0o777 != 0o600
        {
            return Err(AuthError::UnsafeTokenFile);
        }
        Ok(secret)
    }

    pub(crate) fn matches(&self, supplied: &str) -> bool {
        let Some(supplied) = decode(supplied) else {
            return false;
        };
        bool::from(self.bytes.as_slice().ct_eq(supplied.as_slice()))
    }

    pub(crate) fn remove_owned(&self) {
        #[cfg(unix)]
        if let Ok(metadata) = fs::symlink_metadata(&self.path) {
            if metadata.is_file()
                && (metadata.dev(), metadata.ino(), metadata.uid()) == self.identity
            {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn create_token_temp(data_dir: &Path) -> Result<(PathBuf, File, fs::Metadata), AuthError> {
    for _ in 0..128 {
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = data_dir.join(format!(".daemon-token.{}.{sequence}", std::process::id()));
        match create_private_file(&path) {
            Ok((file, metadata)) => return Ok((path, file, metadata)),
            Err(TransportError::Io(io::ErrorKind::AlreadyExists)) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(AuthError::UnsafeTokenFile)
}

struct PendingFile(PathBuf);

impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(unix)]
fn remove_stale_token(data_dir: &Path) -> Result<(), AuthError> {
    let path = data_dir.join("daemon.token");
    let before = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AuthError::Io(error.kind())),
    };
    let owner = fs::metadata(data_dir)
        .map_err(|error| AuthError::Io(error.kind()))?
        .uid();
    if !before.file_type().is_file() || before.uid() != owner {
        return Err(AuthError::UnsafeTokenFile);
    }
    let after = fs::symlink_metadata(&path).map_err(|error| AuthError::Io(error.kind()))?;
    if !after.file_type().is_file()
        || (before.dev(), before.ino(), before.uid()) != (after.dev(), after.ino(), after.uid())
    {
        return Err(AuthError::UnsafeTokenFile);
    }
    fs::remove_file(path).map_err(|error| AuthError::Io(error.kind()))
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.remove_owned();
    }
}

pub(crate) fn authenticate(
    secret: &Secret,
    handshake: &Handshake,
    available: &[Capability],
) -> Result<(u16, Vec<Capability>), AuthError> {
    if !handshake
        .token
        .as_deref()
        .is_some_and(|token| secret.matches(token))
    {
        return Err(AuthError::Failed);
    }
    let version = handshake
        .versions
        .iter()
        .copied()
        .filter(|version| *version == 1)
        .max()
        .ok_or(AuthError::UnsupportedVersion)?;
    let mut capabilities = Vec::new();
    for requested in &handshake.capabilities {
        let Some(capability) = Capability::from_str(requested) else {
            return Err(AuthError::UnsupportedCapability);
        };
        if available.contains(&capability) && !capabilities.contains(&capability) {
            capabilities.push(capability);
        }
    }
    Ok((version, capabilities))
}

fn encode(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}

fn decode(input: &str) -> Option<[u8; 32]> {
    if input.len() != 64 || !input.is_ascii() {
        return None;
    }
    let mut bytes = [0; 32];
    for (index, pair) in input.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Some(bytes)
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
