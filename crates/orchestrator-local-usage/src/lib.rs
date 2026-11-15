//! Machine-wide AI usage from the coding tools on this machine (DPES-1105).
//!
//! The daemon's `usage.summary` covers only runs MCAO started. This crate
//! reads the session files that Claude Code, Codex and GitHub Copilot leave
//! in the user's home folder, through the pinned `codeburn` reader, and
//! reports them as ESTIMATES. Costs are list-price estimates from the
//! reader's own price table; they are never provider-reported and never
//! added to MCAO's ledger.
//!
//! # Rules
//!
//! * Opt-in and read-only. Nothing is read until the user turns it on; turning
//!   it off kills a running reader, drops the cache and starts no new read.
//! * The reader is never installed, updated or launched through a shell. Its
//!   location is found in this order: the bundle beside the daemon executable
//!   (`codeburn/node[.exe]` and `codeburn/pkg/dist/cli.js`), then, in debug
//!   builds only, `AGENT_ORCH_CODEBURN_BIN`, then `PATH` and the common global
//!   folders. Release builds never consult `PATH`. Every location is checked by
//!   package name, the exact pinned version and the SHA-256 of the files it runs,
//!   and is started as `node <cli.js> ...` with argv only.
//! * The reader gets an allowlisted environment: no proxy variables, no
//!   `NODE_OPTIONS`, `CODEBURN_PRICING_SNAPSHOT_ONLY=1` (bundled prices, no
//!   price download), `CODEBURN_FX_NO_FETCH=1` (no exchange rates) and a
//!   cache directory inside MCAO's data directory. Only the `report` command
//!   is run. `quota`, `sync`, `update`, `guard`, `act` and `optimize` are
//!   never run.
//! * Only usage counters leave the reader's JSON: tokens, calls, sessions and
//!   costs, and the number of unpriced models. Model names, project names,
//!   paths and session ids are dropped by the parser and never stored.
//! * Never blocks a request. The daemon reads a cached snapshot; a background
//!   thread runs the reader with a timeout, a stdout size cap and
//!   kill-on-timeout (process group on Unix, Job Object on Windows).
//!
//! # Upgrading the pinned reader (maintainers)
//!
//! 1. Check the new release notes and diff the package against the old one:
//!    `npm pack codeburn@<new> --pack-destination <empty dir>`, extract it into
//!    another empty directory, and review changes to network calls, credential
//!    reads, telemetry, `sync` and `update`. The `quota` command reads login
//!    tokens and refreshes them, so it must stay unused.
//! 2. Verify the tarball: `npm view codeburn@<new> dist.integrity` must equal
//!    the SHA-512 of the packed file (`openssl dgst -sha512 -binary <tgz> |
//!    openssl base64 -A`, prefixed with `sha512-`).
//! 3. Update `codeburn-pin.json`: `version`, `tarball_integrity`, and the
//!    SHA-256 of `dist/cli.js` (without its first line, the `#!` interpreter
//!    line, which package managers rewrite), `dist/main.js` and
//!    `dist/parse-worker.js` from the extracted package.
//! 4. Re-run `codeburn report --format json -p today --provider <p>` on a
//!    synthetic home. If the JSON fields in `parse.rs` changed, update the
//!    parser and the recorded fixture in `tests/fixtures`.
//! 5. Run the Windows smoke test (`scripts/smoke/usage-reader.smoke.mjs`) and
//!    the crate tests. The popover copy derives the version from the pin.

mod capture;
mod locate;
mod model;
mod parse;
mod pin;
mod service;

pub use model::{
    DataWindow, ProviderUsage, ReaderState, ReaderStatus, Snapshot, Window, PROVIDER_IDS,
    SCHEMA_VERSION,
};
pub use pin::{pin, Pin};
pub use service::{
    CodeburnSource, LocalUsageService, ReaderFault, ReaderLocations, ReaderOrigin, ReaderSource,
    ResolvedReader, Timing, BUNDLE_DIR,
};
