//! The pstack workflow agent every work attempt runs as, whatever the provider.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Agent id passed to `--agent` (Claude Code, Copilot).
pub const AGENT_ID: &str = "pstack:workflow-agent";

const INSTALLED_FILE: &str = ".claude/plugins/installed_plugins.json";
const PLUGIN_KEY: &str = "pstack@mayo-skills";
const CACHE_DIR: &str = ".claude/plugins/cache/mayo-skills/pstack";
const AGENT_FILE: &str = "agents/workflow-agent.md";
const SKILL_FILE: &str = "workflow-router/SKILL.md";

pub struct WorkflowAgent {
    /// Installed pstack plugin root, passed to `--plugin-dir`.
    pub plugin_dir: PathBuf,
    /// Instructions injected into Codex, which has no named agents.
    pub codex_instructions: String,
}

/// The installed pstack plugin, or `None` when it is not installed.
pub fn locate() -> Option<WorkflowAgent> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    locate_in(Path::new(&home))
}

fn locate_in(home: &Path) -> Option<WorkflowAgent> {
    installed_dirs(home)
        .into_iter()
        .chain(cached_dirs(home))
        .find_map(|dir| plugin_at(&dir))
}

/// Install paths the plugin registry lists for pstack.
fn installed_dirs(home: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(home.join(INSTALLED_FILE)) else {
        return Vec::new();
    };
    let Ok(registry) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    registry["plugins"]
        .get(PLUGIN_KEY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("installPath")?.as_str())
        .map(PathBuf::from)
        .collect()
}

/// Cached pstack versions, highest first. Compared numerically, so 0.10.0 beats 0.9.0.
fn cached_dirs(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(home.join(CACHE_DIR)) else {
        return Vec::new();
    };
    let mut versions: Vec<(Vec<u64>, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let version = entry
                .file_name()
                .to_str()?
                .split('.')
                .map(|part| part.parse::<u64>().ok())
                .collect::<Option<Vec<_>>>()?;
            Some((version, entry.path()))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, path)| path).collect()
}

/// Copies `plugin` into `dest` and returns the copy as the workflow agent, so a run reads a
/// directory the daemon owns and the shared plugin cache is never a path an agent can reach.
/// The source is only read.
pub fn stage(plugin: &WorkflowAgent, dest: &Path) -> io::Result<WorkflowAgent> {
    copy_tree(&plugin.plugin_dir, dest)?;
    plugin_at(dest).ok_or_else(|| io::Error::other("the staged plugin lacks a required file"))
}

/// Copies the files and directories under `from` into `to`. `fs::copy` refuses a link to a
/// directory, so the copy never follows a cycle.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A plugin root, only when both files the agent and its router read exist.
pub fn plugin_at(dir: &Path) -> Option<WorkflowAgent> {
    let agent = fs::read_to_string(dir.join(AGENT_FILE)).ok()?;
    let skill = dir.join(SKILL_FILE);
    if !skill.is_file() {
        return None;
    }
    Some(WorkflowAgent {
        plugin_dir: dir.to_path_buf(),
        codex_instructions: format!(
            "{}\n\nThe workflow-router skill is at {}; read it in full first.",
            body(&agent),
            skill.display()
        ),
    })
}

/// The agent prompt after its YAML frontmatter.
fn body(markdown: &str) -> &str {
    markdown
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---"))
        .map_or(markdown, |(_, text)| text)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plugin root with the two files `plugin_at` requires.
    fn write_plugin(dir: &Path) {
        fs::create_dir_all(dir.join("agents")).unwrap();
        fs::create_dir_all(dir.join("workflow-router")).unwrap();
        fs::write(
            dir.join(AGENT_FILE),
            "---\nname: workflow-agent\n---\n\nRead the skill.\n",
        )
        .unwrap();
        fs::write(dir.join(SKILL_FILE), "# Router\n").unwrap();
    }

    #[test]
    fn finds_the_plugin_the_registry_lists() {
        let home = tempfile::tempdir().unwrap();
        let install = home.path().join("plugins/pstack");
        write_plugin(&install);
        let registry = serde_json::json!({
            "plugins": { "pstack@mayo-skills": [{ "installPath": install.to_str() }] }
        });
        fs::create_dir_all(home.path().join(".claude/plugins")).unwrap();
        fs::write(home.path().join(INSTALLED_FILE), registry.to_string()).unwrap();

        let agent = locate_in(home.path()).unwrap();
        assert_eq!(agent.plugin_dir, install);
        assert!(agent.codex_instructions.starts_with("Read the skill."));
        assert!(agent
            .codex_instructions
            .contains(&install.join(SKILL_FILE).display().to_string()));
    }

    #[test]
    fn falls_back_to_the_highest_cached_version() {
        let home = tempfile::tempdir().unwrap();
        let cache = home.path().join(CACHE_DIR);
        write_plugin(&cache.join("0.9.0"));
        write_plugin(&cache.join("0.10.0"));

        let agent = locate_in(home.path()).unwrap();
        assert_eq!(agent.plugin_dir, cache.join("0.10.0"));
    }

    #[test]
    fn stage_copies_the_plugin_and_leaves_the_source_untouched() {
        let home = tempfile::tempdir().unwrap();
        let source = home.path().join("cache/0.10.0");
        write_plugin(&source);
        fs::create_dir_all(source.join("workflow-router/nested")).unwrap();
        fs::write(source.join("workflow-router/nested/extra.md"), "extra\n").unwrap();
        let plugin = plugin_at(&source).unwrap();
        let dest = home.path().join("status/plugin");

        let staged = stage(&plugin, &dest).unwrap();

        assert_eq!(staged.plugin_dir, dest);
        assert!(staged
            .codex_instructions
            .contains(&dest.join(SKILL_FILE).display().to_string()));
        assert_eq!(
            fs::read_to_string(dest.join("workflow-router/nested/extra.md")).unwrap(),
            "extra\n"
        );
        fs::write(dest.join(SKILL_FILE), "# Edited copy\n").unwrap();
        assert_eq!(
            fs::read_to_string(source.join(SKILL_FILE)).unwrap(),
            "# Router\n"
        );
    }

    #[test]
    fn none_when_a_required_file_is_missing() {
        let home = tempfile::tempdir().unwrap();
        let cache = home.path().join(CACHE_DIR);
        write_plugin(&cache.join("0.10.0"));
        fs::remove_file(cache.join("0.10.0").join(SKILL_FILE)).unwrap();

        assert!(locate_in(home.path()).is_none());
    }
}
