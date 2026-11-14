//! Seeds one reviewable task into a daemon data directory for end-to-end tests.
//!
//! Public RPC cannot create a task with a recorded workspace and a passed
//! verification run, so the desktop e2e suite and the desktop host's
//! real-daemon tests run this against a stopped daemon's `store.sqlite`, then
//! start the real daemon over the same data. The seeding itself lives in
//! `tests/support/seed.rs`, shared with the daemon's process tests.
//!
//! Usage: `seed_review <data-dir> <repo> <worktrees-dir> [task-id]`
//! Prints one JSON object describing the seeded records on stdout.

#[path = "../tests/support/seed.rs"]
mod seed;

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, repo, worktrees, rest @ ..] = args.as_slice() else {
        eprintln!("usage: seed_review <data-dir> <repo> <worktrees-dir> [task-id]");
        std::process::exit(2);
    };
    let task_id = rest.first().map_or("e2e-task", String::as_str);
    let seeded = seed::seed_review(
        Path::new(data_dir),
        Path::new(repo),
        Path::new(worktrees),
        task_id,
    );
    println!("{seeded}");
}
