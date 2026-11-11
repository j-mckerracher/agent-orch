#![allow(dead_code)]

/// Times one client call; the label is the call expression itself. The
/// timings are printed with the daemon log when the test fails.
#[allow(unused_macros)]
macro_rules! timed {
    ($call:expr) => {
        $crate::support::process::timed(stringify!($call), || $call)
    };
}

pub mod process;
pub mod seed;
