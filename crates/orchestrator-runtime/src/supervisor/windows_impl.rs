//! Windows process-tree ownership and liveness checks.

use std::io;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, OpenThread, ResumeThread, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    PROCESS_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
};

pub(super) const PROCESS_CREATION_FLAGS: u32 = CREATE_NO_WINDOW | CREATE_SUSPENDED;

/// Assign the suspended child to a kill-on-close job, then resume its
/// primary thread. The caller kills the child on error; closing the job
/// also kills it if assignment already succeeded.
pub(super) fn assign_suspended_to_job(pid: u32, process: HANDLE) -> io::Result<HANDLE> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }

    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 {
        let error = io::Error::last_os_error();
        unsafe { CloseHandle(job) };
        return Err(error);
    }

    if unsafe { AssignProcessToJobObject(job, process) } == 0 {
        let error = io::Error::last_os_error();
        unsafe { CloseHandle(job) };
        return Err(error);
    }

    if let Err(error) = resume_primary_thread(pid) {
        unsafe { CloseHandle(job) };
        return Err(error);
    }

    Ok(job)
}

fn resume_primary_thread(pid: u32) -> io::Result<()> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut thread_id = None;
    if unsafe { Thread32First(snapshot, &mut entry) } != 0 {
        loop {
            if entry.th32OwnerProcessID == pid {
                thread_id = Some(entry.th32ThreadID);
                break;
            }
            if unsafe { Thread32Next(snapshot, &mut entry) } == 0 {
                break;
            }
        }
    }
    unsafe { CloseHandle(snapshot) };

    let thread_id = thread_id.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "suspended process primary thread was not found",
        )
    })?;
    let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
    if thread.is_null() {
        return Err(io::Error::last_os_error());
    }

    let previous_suspend_count = unsafe { ResumeThread(thread) };
    let error = match previous_suspend_count {
        u32::MAX => Some(io::Error::last_os_error()),
        1 => None,
        count => Some(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("suspended primary thread had unexpected suspend count {count}"),
        )),
    };
    unsafe { CloseHandle(thread) };
    error.map_or(Ok(()), Err)
}

/// Terminate all current job members and close the kill-on-close handle.
/// The process ID is deliberately not checked after closing the job because
/// that identifier may already belong to an unrelated process.
pub(super) fn terminate_job(job: HANDLE) {
    if job.is_null() {
        return;
    }
    unsafe {
        TerminateJobObject(job, 1);
        CloseHandle(job);
    }
}

/// True if a process with `pid` is still running. A terminated process
/// lingers as a zombie until reaped (openable, but not running), so the exit
/// code decides.
pub(super) fn pid_running(pid: u32) -> bool {
    const STILL_ACTIVE: u32 = 259;
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return false;
    }
    let mut code: u32 = 0;
    let alive = unsafe { GetExitCodeProcess(h, &mut code) } != 0 && code == STILL_ACTIVE;
    unsafe { CloseHandle(h) };
    alive
}
