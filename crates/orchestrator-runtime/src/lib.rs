//! Process supervision, cancellation, and crash recovery for agent subprocesses.
//!
//! The supervisor owns child process handles, explicit working directory,
//! stdout/stderr streaming, process exit status, cancellation, and
//! process-tree cleanup. It reports process facts; it never interprets
//! them as task success.
pub mod command_line;
pub mod supervisor;
pub mod reconcile;

pub use command_line::{check_command_line, CommandLineTooLong};

pub use supervisor::{
    is_process_running, ChildSupervisor, LaunchRequest, StdinWriter, SupervisionEvent,
    SupervisorError,
};
