//! Windows process-tree cleanup.
//!
//! Primary: a Job Object created with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
//! The child (and everything it spawns — child processes inherit the job)
//! belongs to the job; closing the handle terminates the whole tree.
//! Fallback: a Toolhelp32 snapshot walk that terminates the target and
//! every descendant by parent-PID chain (also serves to detect whether
//! the job actually covered the tree).

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};

/// Create a kill-on-close job and assign `pid` to it. Returns the job
/// handle, or `None` when creation or assignment failed (the snapshot
/// fallback then covers cancellation).
pub(super) fn assign_to_job(pid: u32) -> Option<HANDLE> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return None;
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        unsafe { CloseHandle(job) };
        return None;
    }
    let proc = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SET_QUOTA, 0, pid) };
    if proc.is_null() {
        unsafe { CloseHandle(job) };
        return None;
    }
    let attached = unsafe { AssignProcessToJobObject(job, proc) };
    unsafe { CloseHandle(proc) };
    if attached == 0 {
        unsafe { CloseHandle(job) };
        return None;
    }
    Some(job)
}

/// Close the job handle; `KILL_ON_JOB_CLOSE` terminates the whole tree
/// synchronously. If the process is still visible afterwards, the job
/// did not cover it, so fall back to the snapshot tree kill.
pub(super) fn kill_tree_via_job(pid: u32, job: HANDLE) {
    unsafe { CloseHandle(job) };
    if pid_running(pid) {
        kill_tree(pid);
    }
}

/// Snapshot the process table and terminate the target and every
/// descendant of it (by parent-PID chain).
pub(super) fn kill_tree(root_pid: u32) {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        return;
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // Fixpoint walk: the snapshot is a single point-in-time image, so
    // grandchildren whose parent was scanned before it entered the tree
    // are only found on a second pass. Repeat until no new descendants
    // appear.
    let mut tree: Vec<u32> = Vec::new();
    loop {
        let before = tree.len();
        if unsafe { Process32FirstW(snapshot, &mut entry) } != 0 {
            loop {
                let p = entry.th32ProcessID;
                let parent = entry.th32ParentProcessID;
                if p != 0 && p != root_pid && !tree.contains(&p)
                    && (parent == root_pid || tree.contains(&parent))
                {
                    tree.push(p);
                }
                if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                    break;
                }
            }
        }
        if tree.len() == before {
            break;
        }
    }
    unsafe { CloseHandle(snapshot) };
    for p in tree {
        kill_pid(p);
    }
    kill_pid(root_pid);
}

fn kill_pid(pid: u32) {
    let h = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if h.is_null() {
        return;
    }
    unsafe {
        TerminateProcess(h, 1);
        CloseHandle(h);
    }
}

/// True if a process with `pid` is still running. A terminated process
/// lingers as a zombie until reaped (openable, but not running), so the
/// exit code decides.
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
