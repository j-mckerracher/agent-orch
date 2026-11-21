//! Where Claude keeps a session's transcript, so a finished task whose slot went back to the
//! pool can be continued in a new one (DPES-1106).
//!
//! `claude --resume <id>` finds a session only from the folder it ran in: the transcript lives
//! at `<config>/projects/<folder>/<id>.jsonl`, where `<folder>` is the working directory with
//! every character outside `[A-Za-z0-9]` replaced by `-`. A new slot is a new folder, so the
//! file is moved (not copied, so it takes no extra disk) to the new folder's name. Codex and
//! Copilot find a session by id alone and need no move.

use std::path::{Path, PathBuf};

/// The agent whose resume depends on the working directory.
const CLAUDE_AGENT: &str = "claude-code";

/// Claude's own config folder: `CLAUDE_CONFIG_DIR`, else `~/.claude`.
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".claude")))
}

fn transcript(config: &Path, directory: &Path, session_id: &str) -> PathBuf {
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let folder: String = directory
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    config
        .join("projects")
        .join(folder)
        .join(format!("{session_id}.jsonl"))
}

/// Whether `agent`'s session `session_id`, which ran in `directory`, still has a transcript to
/// move. Always true for an agent that needs no move.
pub(crate) fn has_transcript(agent: &str, directory: &Path, session_id: &str) -> bool {
    agent != CLAUDE_AGENT
        || config_dir().is_some_and(|config| transcript(&config, directory, session_id).is_file())
}

/// Moves `agent`'s session transcript from the folder it ran in to the one it resumes in.
pub(crate) fn follow(
    agent: &str,
    from: &Path,
    to: &Path,
    session_id: &str,
) -> Result<(), &'static str> {
    if agent != CLAUDE_AGENT {
        return Ok(());
    }
    let config = config_dir().ok_or("the agent's config folder is not known")?;
    let (source, target) = (
        transcript(&config, from, session_id),
        transcript(&config, to, session_id),
    );
    // A slot the pool handed out again keeps its path, and a retried resume finds it moved.
    if source == target || (!source.is_file() && target.is_file()) {
        return Ok(());
    }
    let folder = target
        .parent()
        .ok_or("the agent's config folder is not known")?;
    std::fs::create_dir_all(folder).map_err(|_| "the agent's session folder cannot be made")?;
    std::fs::rename(&source, &target)
        .map_err(|_| "the agent's saved conversation could not be moved to the new worktree")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_match_claudes_own() {
        let path = transcript(Path::new("/c"), Path::new("/no/such/a_b c.d"), "s1");
        assert_eq!(path, Path::new("/c/projects/-no-such-a-b-c-d/s1.jsonl"));
    }

    #[test]
    fn follow_moves_the_transcript_once_and_other_agents_need_none() {
        let config = tempfile::tempdir().unwrap();
        let (from, to) = (Path::new("/old/slot"), Path::new("/new/slot"));
        let source = transcript(config.path(), from, "s1");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "{}").unwrap();
        std::env::set_var("CLAUDE_CONFIG_DIR", config.path());
        assert!(has_transcript(CLAUDE_AGENT, from, "s1"));
        assert!(!has_transcript(CLAUDE_AGENT, from, "gone"));
        follow(CLAUDE_AGENT, from, to, "s1").unwrap();
        assert!(!source.exists() && transcript(config.path(), to, "s1").is_file());
        // A second run finds it already moved.
        follow(CLAUDE_AGENT, from, to, "s1").unwrap();
        assert!(follow(CLAUDE_AGENT, from, to, "gone").is_err());
        assert!(has_transcript("codex", from, "gone"));
    }
}
