//! The treehouse binary the integration tests run against.
//!
//! Fetch it with `node apps/desktop/scripts/prepare-treehouse-sidecar.mjs --dest target/treehouse`
//! and point `AGENT_ORCH_TREEHOUSE_PROGRAM` at the binary it writes.

use std::path::Path;

use orchestrator_git::Treehouse;

pub fn bundled_treehouse(data_dir: &Path) -> Treehouse {
    let program = std::env::var_os("AGENT_ORCH_TREEHOUSE_PROGRAM").unwrap_or_else(|| {
        panic!(
            "AGENT_ORCH_TREEHOUSE_PROGRAM is not set; fetch treehouse with \
             `node apps/desktop/scripts/prepare-treehouse-sidecar.mjs --dest target/treehouse` \
             and point the variable at the binary"
        )
    });
    Treehouse::new(program, data_dir).expect("treehouse private home")
}
