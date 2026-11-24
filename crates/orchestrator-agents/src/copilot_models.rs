//! The Copilot CLI's models and their reasoning levels (DPES-1105).
//!
//! The CLI has no list command. Its Agent Client Protocol server (`copilot --acp`)
//! answers `session/new` with the models the signed-in account can use, and
//! `session/set_config_option` for one model with the reasoning levels that model
//! supports. Neither call reaches a model, so neither is billable.
//!
//! Each `session/new` leaves an empty directory in Copilot's session store, even after
//! `session/close`. A read is therefore kept for [`CACHE_TTL`] per executable, and a catalog
//! request does not start the server every time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines, Take};
use tokio::process::{ChildStdin, ChildStdout, Command};

use crate::catalog::{display_name, safe_effort, safe_model_id, ModelOption, ProcessGroup};

const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
/// All the server writes before the read is abandoned. A full read is about 300 KB.
const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

type Cache = Mutex<HashMap<PathBuf, (Instant, Vec<ModelOption>)>>;

/// The models and levels of the Copilot at `program`, or `None` when the server did not answer
/// in `limit`. A failed read is never kept.
pub(crate) async fn models(program: &Path, limit: Duration) -> Option<Vec<ModelOption>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Cache::default);
    let lock = || cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some((read_at, items)) = lock().get(program) {
        if read_at.elapsed() < CACHE_TTL {
            return Some(items.clone());
        }
    }
    let items = read(program, limit).await?;
    lock().insert(program.to_path_buf(), (Instant::now(), items.clone()));
    Some(items)
}

/// One JSON-RPC line per message. Notifications and requests from the server are skipped: this
/// client answers none, and the calls it makes need none.
struct Server {
    stdin: ChildStdin,
    lines: Lines<BufReader<Take<ChildStdout>>>,
    last_id: u64,
}

impl Server {
    /// The `result` of the call, or `None` for an error reply or a closed server.
    async fn call(&mut self, method: &str, params: Value) -> Option<Value> {
        self.last_id += 1;
        let id = self.last_id;
        let mut line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await.ok()?;
        self.stdin.flush().await.ok()?;
        while let Some(text) = self.lines.next_line().await.ok()? {
            let Ok(message) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if message.get("method").is_none() && message.get("id").and_then(Value::as_u64) == Some(id) {
                return message.get("result").cloned();
            }
        }
        None
    }
}

async fn read(program: &Path, limit: Duration) -> Option<Vec<ModelOption>> {
    // A neutral directory: the server reads the instruction files of its working directory.
    let scratch = std::env::temp_dir();
    let mut command = Command::new(program);
    command
        .arg("--acp")
        .current_dir(&scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().ok()?;
    let (stdin, stdout) = (child.stdin.take()?, child.stdout.take()?);
    let group = ProcessGroup::of(&child);
    let mut server = Server {
        stdin,
        lines: BufReader::new(stdout.take(MAX_OUTPUT_BYTES)).lines(),
        last_id: 0,
    };
    let items = tokio::time::timeout(limit, async {
        server
            .call("initialize", json!({"protocolVersion": 1, "clientCapabilities": {}}))
            .await?;
        let created = server
            .call("session/new", json!({"cwd": scratch, "mcpServers": []}))
            .await?;
        let session = created.get("sessionId")?.as_str()?.to_owned();
        let mut items = Vec::new();
        for (id, name) in parse_models(&created) {
            // A failed call abandons the read: a model without its levels would be kept as "none".
            let switched = server
                .call(
                    "session/set_config_option",
                    json!({"sessionId": session, "configId": "model", "value": id}),
                )
                .await?;
            let (efforts, default_effort) = parse_efforts(switched.get("configOptions"));
            items.push(ModelOption {
                id,
                display_name: name,
                efforts,
                default_effort,
            });
        }
        server.call("session/close", json!({"sessionId": session})).await;
        (!items.is_empty()).then_some(items)
    })
    .await
    .ok()
    .flatten();
    group.kill();
    let _ = child.wait().await;
    group.disarm();
    items
}

/// The models of a `session/new` result as (id, name), each once. A model the account has not
/// enabled is left out, and so is an id that is not safe to put on a command line.
fn parse_models(created: &Value) -> Vec<(String, String)> {
    let mut models: Vec<(String, String)> = Vec::new();
    let listed = created.pointer("/models/availableModels").and_then(Value::as_array);
    for item in listed.into_iter().flatten() {
        let Some(id) = item.get("modelId").and_then(Value::as_str) else {
            continue;
        };
        let enabled = item
            .pointer("/_meta/copilotEnablement")
            .and_then(Value::as_str)
            .is_none_or(|state| state == "enabled");
        if enabled && safe_model_id(id) && !models.iter().any(|(seen, _)| seen == id) {
            let name = display_name(item.get("name").and_then(Value::as_str), id);
            models.push((id.to_owned(), name));
        }
    }
    models
}

/// The levels in a `configOptions` list, and the one selected now. Both are empty when the
/// selected model has no reasoning level.
fn parse_efforts(config: Option<&Value>) -> (Vec<String>, Option<String>) {
    let option = config
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|option| option.get("id").and_then(Value::as_str) == Some("reasoning_effort"));
    let Some(option) = option else {
        return (Vec::new(), None);
    };
    let mut levels: Vec<String> = Vec::new();
    for entry in option.get("options").and_then(Value::as_array).into_iter().flatten() {
        if let Some(level) = entry.get("value").and_then(Value::as_str) {
            if safe_effort(level) && !levels.iter().any(|seen| seen == level) {
                levels.push(level.to_owned());
            }
        }
    }
    let current = option
        .get("currentValue")
        .and_then(Value::as_str)
        .filter(|level| levels.iter().any(|seen| seen == level))
        .map(str::to_owned);
    (levels, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped like a real `session/new` result: a duplicate `auto`, a model the account has not
    /// enabled, and an id that is not safe to pass on a command line.
    fn created() -> Value {
        json!({"sessionId": "s", "models": {"availableModels": [
            {"modelId": "auto", "name": "Auto", "description": "Let Copilot pick"},
            {"modelId": "auto", "name": "Auto", "_meta": {"copilotEnablement": "enabled"}},
            {"modelId": "claude-sonnet-5.5", "name": "Claude Sonnet 5.5", "_meta": {"copilotEnablement": "enabled"}},
            {"modelId": "gpt-hidden", "name": "Hidden", "_meta": {"copilotEnablement": "disabled"}},
            {"modelId": "--model", "name": "Flag"},
            {"modelId": "grok-4.7"}
        ]}})
    }

    #[test]
    fn models_are_listed_once_and_skip_disabled_and_unsafe_ids() {
        assert_eq!(
            parse_models(&created()),
            [
                ("auto".to_owned(), "Auto".to_owned()),
                ("claude-sonnet-5.5".to_owned(), "Claude Sonnet 5.5".to_owned()),
                ("grok-4.7".to_owned(), "grok-4.7".to_owned()),
            ]
        );
        assert!(parse_models(&json!({})).is_empty());
    }

    #[test]
    fn efforts_are_the_selected_models_levels_with_its_current_one() {
        let config = json!([
            {"id": "mode", "options": [{"value": "agent"}]},
            {"id": "reasoning_effort", "currentValue": "high",
             "options": [{"value": "low"}, {"value": "high"}, {"value": "high"}, {"value": "Max!"}]},
        ]);
        assert_eq!(
            parse_efforts(Some(&config)),
            (vec!["low".to_owned(), "high".to_owned()], Some("high".to_owned()))
        );
        let without = json!([{"id": "mode", "options": [{"value": "agent"}]}]);
        assert_eq!(parse_efforts(Some(&without)), (Vec::new(), None));
        assert_eq!(parse_efforts(None), (Vec::new(), None));
        // A current level that is not offered is not reported as the default.
        let stale = json!([{"id": "reasoning_effort", "currentValue": "max", "options": [{"value": "low"}]}]);
        assert_eq!(parse_efforts(Some(&stale)), (vec!["low".to_owned()], None));
    }
}
