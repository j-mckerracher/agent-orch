//! Windows implementation: a protected DACL that grants full control to the
//! current user's SID only.

use crate::sole_owner_ace;
use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
    GetNamedSecurityInfoW, SetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(crate) fn create_private_dir(path: &Path) -> io::Result<()> {
    // Parents that do not exist yet are made owner-only as well, like the
    // 0700 mode Unix gives every directory the builder creates.
    let mut missing: Vec<PathBuf> = Vec::new();
    let mut cursor = Some(path);
    while let Some(candidate) = cursor.filter(|candidate| !candidate.as_os_str().is_empty()) {
        if candidate.exists() {
            break;
        }
        missing.push(candidate.to_path_buf());
        cursor = candidate.parent();
    }
    std::fs::create_dir_all(path)?;
    for created in missing.iter().rev() {
        restrict_to_owner(created)?;
    }
    restrict_to_owner(path)
}

pub(crate) fn restrict_to_owner(path: &Path) -> io::Result<()> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let sid = current_user_sid()?;
    if is_owner_only(path, &sid)? {
        return Ok(());
    }
    // A directory's ACE is inherited by files and directories created in it
    // later; a file's ACE has nothing to inherit.
    let inherit = if metadata.is_dir() { "OICI" } else { "" };
    apply_sddl(path, &format!("D:P(A;{inherit};FA;;;{sid})"))
}

/// True when `path` has a protected DACL whose only ACE allows the user.
pub(crate) fn is_owner_only(path: &Path, sid: &str) -> io::Result<bool> {
    let sddl = read_dacl_sddl(path)?;
    Ok(sole_owner_ace(&sddl, |trustee| same_sid(trustee, sid)) == Some(true))
}

pub(crate) fn read_dacl_sddl(path: &Path) -> io::Result<String> {
    let wide = wide(path.as_os_str());
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated UTF-16; only the DACL is requested and
    // `descriptor` is a writable output pointer to a LocalAlloc'd descriptor.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let descriptor = LocalAllocation(descriptor);
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: `descriptor` is the live descriptor returned above and `text` is
    // a writable output pointer for the allocated NUL-terminated string.
    if unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(last_error());
    }
    let text = LocalAllocation(text.cast());
    Ok(read_wide(text.0.cast()))
}

pub(crate) fn apply_sddl(path: &Path, sddl: &str) -> io::Result<()> {
    let wide_path = wide(path.as_os_str());
    let wide_sddl = wide(std::ffi::OsStr::new(sddl));
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide_sddl` is NUL-terminated UTF-16 and `descriptor` is a
    // writable output pointer; the API allocates the returned descriptor.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(last_error());
    }
    let descriptor = LocalAllocation(descriptor);
    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl = std::ptr::null_mut();
    // SAFETY: `descriptor` is live and the three outputs are writable; `dacl`
    // points into `descriptor`, which outlives the call below.
    if unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut dacl, &mut defaulted) }
        == 0
    {
        return Err(last_error());
    }
    if present == 0 || dacl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "security descriptor has no DACL",
        ));
    }
    // SAFETY: `wide_path` is NUL-terminated UTF-16 and `dacl` is valid. The
    // protected flag stops the parent's ACEs from applying; an inheritable
    // ACE on a directory is propagated to its existing unprotected children.
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            dacl,
            std::ptr::null(),
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

/// SDDL spells some SIDs as aliases (the built-in Administrator account reads
/// back as `LA`), so equal SIDs are compared as SIDs, not as text.
pub(crate) fn same_sid(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let (Some(left), Some(right)) = (sid_from_string(left), sid_from_string(right)) else {
        return false;
    };
    // SAFETY: both pointers are live SIDs allocated by ConvertStringSidToSidW.
    unsafe { EqualSid(left.0, right.0) != 0 }
}

fn sid_from_string(text: &str) -> Option<LocalAllocation> {
    let wide = wide(std::ffi::OsStr::new(text));
    let mut sid: *mut c_void = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated UTF-16 and `sid` is a writable output
    // pointer for the LocalAlloc'd SID.
    (unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } != 0 && !sid.is_null())
        .then_some(LocalAllocation(sid))
}

pub(crate) fn current_user_sid() -> io::Result<String> {
    // SAFETY: GetCurrentProcess returns the documented pseudo-handle for this
    // process; OpenProcessToken does not take ownership of it.
    let process = unsafe { GetCurrentProcess() };
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is a valid pseudo-handle and `token` is a writable
    // output pointer for the returned owned token handle.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(last_error());
    }
    if token.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let token = OwnedHandle(token);
    let mut required = 0u32;
    // SAFETY: a null buffer of zero length is the documented size query.
    let queried =
        unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut required) };
    // SAFETY: GetLastError reads this thread's last Win32 error value.
    let error = unsafe { GetLastError() };
    if queried != 0 || error != ERROR_INSUFFICIENT_BUFFER {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    if (required as usize) < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut buffer = vec![0usize; (required as usize).div_ceil(std::mem::size_of::<usize>())];
    let buffer_len = (buffer.len() * std::mem::size_of::<usize>()) as u32;
    let mut returned = 0u32;
    // SAFETY: `buffer` is aligned and at least `required` bytes long.
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
        return Err(last_error());
    }
    if (returned as usize) < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // SAFETY: a successful call initialized a TOKEN_USER at the start of the
    // aligned `buffer`, and the SID it points to lives inside `buffer`.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    if user.User.Sid.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: the SID is live and `text` is a writable output pointer.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(last_error());
    }
    if text.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let text = LocalAllocation(text.cast());
    Ok(read_wide(text.0.cast()))
}

fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Copies a NUL-terminated UTF-16 string owned by Windows.
fn read_wide(text: *const u16) -> String {
    // SAFETY: callers pass a live NUL-terminated string; the slice ends before
    // the terminator and is copied before the owner frees it.
    unsafe {
        let mut length = 0usize;
        while *text.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    }
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from a LocalAlloc-backed Windows API
            // and is freed exactly once here.
            let _ = unsafe { LocalFree(self.0) };
        }
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this is the token handle from OpenProcessToken, closed
            // exactly once here.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_private_dir, create_private_file, restrict_to_owner};

    fn sddl(path: &Path) -> String {
        read_dacl_sddl(path).unwrap()
    }

    fn owner_only(path: &Path) -> bool {
        let sid = current_user_sid().unwrap();
        sole_owner_ace(&sddl(path), |trustee| same_sid(trustee, &sid)) == Some(true)
    }

    /// Stands in for a directory that inherited a broad ACL such as the
    /// `CodexSandboxUsers` Modify ACE: it grants Everyone full control.
    fn widen(path: &Path) {
        apply_sddl(path, "D:P(A;OICI;FA;;;WD)").unwrap();
        assert!(!owner_only(path));
    }

    #[test]
    fn a_new_directory_tree_and_file_are_owner_only() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a").join("b");
        create_private_dir(&nested).unwrap();
        assert!(owner_only(&nested), "{}", sddl(&nested));
        assert!(owner_only(&root.path().join("a")));
        let file = nested.join("store.sqlite");
        create_private_file(&file).unwrap();
        assert!(owner_only(&file), "{}", sddl(&file));
    }

    #[test]
    fn existing_wide_open_paths_are_tightened_without_losing_content() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("store.sqlite");
        std::fs::write(&file, b"kept").unwrap();
        widen(&dir);
        widen(&file);
        create_private_dir(&dir).unwrap();
        create_private_file(&file).unwrap();
        assert!(owner_only(&dir), "{}", sddl(&dir));
        assert!(owner_only(&file), "{}", sddl(&file));
        assert_eq!(std::fs::read(&file).unwrap(), b"kept");
    }

    #[test]
    fn the_directory_ace_is_inherited_by_files_created_later() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        create_private_dir(&dir).unwrap();
        let sid = current_user_sid().unwrap();
        for name in ["store.sqlite-wal", "store.sqlite-shm"] {
            let file = dir.join(name);
            std::fs::write(&file, b"x").unwrap();
            assert!(
                sole_owner_ace(&sddl(&file), |trustee| same_sid(trustee, &sid)).is_some(),
                "{name}: {}",
                sddl(&file)
            );
        }
    }

    #[test]
    fn tightening_a_directory_also_tightens_files_it_already_holds() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("worktree-file.txt");
        std::fs::write(&file, b"x").unwrap();
        widen(&dir);
        create_private_dir(&dir).unwrap();
        let sid = current_user_sid().unwrap();
        assert!(
            sole_owner_ace(&sddl(&file), |trustee| same_sid(trustee, &sid)).is_some(),
            "{}",
            sddl(&file)
        );
    }

    #[test]
    fn a_missing_path_is_not_an_error_and_a_private_path_is_not_rewritten() {
        let root = tempfile::tempdir().unwrap();
        restrict_to_owner(&root.path().join("absent")).unwrap();
        let file = root.path().join("f");
        create_private_file(&file).unwrap();
        let before = sddl(&file);
        restrict_to_owner(&file).unwrap();
        assert_eq!(sddl(&file), before);
    }

    #[test]
    fn a_file_with_only_an_inherited_owner_ace_is_not_owner_only_until_restricted() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        create_private_dir(&dir).unwrap();
        // Written with std, as `.orchestrator.lock` was: the DACL is inherited
        // from the directory and unprotected (`<user>:(I)(F)` in `icacls`).
        let file = dir.join(".orchestrator.lock");
        std::fs::write(&file, b"pid=1\n").unwrap();
        assert!(!crate::is_owner_only(&file).unwrap(), "{}", sddl(&file));
        assert!(!owner_only(&file));
        restrict_to_owner(&file).unwrap();
        assert!(crate::is_owner_only(&file).unwrap(), "{}", sddl(&file));
        assert!(owner_only(&file), "{}", sddl(&file));
        assert_eq!(std::fs::read(&file).unwrap(), b"pid=1\n");
    }

    #[test]
    fn the_owner_sid_text_matches_itself() {
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"));
        assert!(same_sid(&sid, &sid));
        assert!(!same_sid(&sid, "WD"));
    }
}
