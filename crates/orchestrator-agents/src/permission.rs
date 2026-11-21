//! Which permission levels each agent's task launch maps (DPES-1105).
//! The level type is `orchestrator_core::permission::PermissionLevel`.
//!
//! Tasks map Extended for Claude Code and GitHub Copilot only, because Codex has
//! no tool allowlist to express it. Chats resolve through `crate::modes`. The
//! mapping and its flags are in `docs/design/agent-permission-levels.md`.

use orchestrator_core::permission::PermissionLevel;

/// The name of the argument mapping a task launch runs with, recorded on its
/// launch event. `None` when the agent has no task mapping for the level, which
/// submission refuses (Extended on Codex).
pub fn task_profile(agent: &str, level: PermissionLevel) -> Option<&'static str> {
    use PermissionLevel::{Auto, Extended, Full, Restricted};
    match (agent, level) {
        ("codex", Restricted) => Some("codex:restricted:task:v1"),
        ("codex", Auto) => Some("codex:auto:task:v1"),
        ("codex", Full) => Some("codex:full:task:v1"),
        ("claude-code", Restricted) => Some("claude-code:restricted:task:v1"),
        ("claude-code", Extended) => Some("claude-code:extended:task:v1"),
        ("claude-code", Auto) => Some("claude-code:auto:task:v1"),
        ("claude-code", Full) => Some("claude-code:full:task:v1"),
        ("github-copilot", Restricted) => Some("github-copilot:restricted:task:v1"),
        ("github-copilot", Extended) => Some("github-copilot:extended:task:v1"),
        ("github-copilot", Auto) => Some("github-copilot:auto:task:v1"),
        ("github-copilot", Full) => Some("github-copilot:full:task:v1"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_profiles_map_every_level_except_extended_on_codex() {
        for agent in ["codex", "claude-code", "github-copilot"] {
            for level in [
                PermissionLevel::Restricted,
                PermissionLevel::Auto,
                PermissionLevel::Full,
            ] {
                let profile = task_profile(agent, level).unwrap();
                assert!(profile.starts_with(agent), "{profile}");
                assert!(profile.contains(":task:"), "{profile}");
            }
        }
        assert_eq!(task_profile("codex", PermissionLevel::Extended), None);
        for agent in ["claude-code", "github-copilot"] {
            assert!(task_profile(agent, PermissionLevel::Extended).is_some());
        }
        assert_eq!(task_profile("pi", PermissionLevel::Restricted), None);
    }
}
