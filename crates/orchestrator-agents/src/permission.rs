//! Which permission levels each agent's chat argument builder maps (DPES-1105).
//! The level type is `orchestrator_core::permission::PermissionLevel`.
//!
//! Extended has no chat mapping yet, so every agent refuses it. The mapping and
//! its flags are in `docs/design/agent-permission-levels.md`.

use orchestrator_core::permission::PermissionLevel;

/// The name of the argument mapping a chat turn runs with, recorded on the turn.
/// `None` when the agent has no mapping for the level. Change the version when a
/// mapping's arguments change, so a stored turn names the flags it ran with.
pub fn chat_profile(agent: &str, level: PermissionLevel) -> Option<&'static str> {
    use PermissionLevel::{Auto, Full, Restricted};
    match (agent, level) {
        ("codex", Restricted) => Some("codex:restricted:v1"),
        ("codex", Auto) => Some("codex:auto:v1"),
        ("codex", Full) => Some("codex:full:v1"),
        ("claude-code", Restricted) => Some("claude-code:restricted:v1"),
        ("claude-code", Auto) => Some("claude-code:auto:v1"),
        ("claude-code", Full) => Some("claude-code:full:v1"),
        ("github-copilot", Restricted) => Some("github-copilot:restricted:v1"),
        ("github-copilot", Auto) => Some("github-copilot:auto:v1"),
        ("github-copilot", Full) => Some("github-copilot:full:v1"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_has_no_chat_profile_and_the_other_levels_do_for_every_agent() {
        for agent in ["codex", "claude-code", "github-copilot"] {
            assert_eq!(chat_profile(agent, PermissionLevel::Extended), None);
            for level in [
                PermissionLevel::Restricted,
                PermissionLevel::Auto,
                PermissionLevel::Full,
            ] {
                let profile = chat_profile(agent, level).unwrap();
                assert!(profile.starts_with(agent), "{profile}");
                assert!(profile.ends_with(":v1"), "{profile}");
            }
        }
        assert_eq!(chat_profile("unavailable", PermissionLevel::Auto), None);
    }
}
