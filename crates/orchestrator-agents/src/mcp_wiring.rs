//! How a CLI is told about the per-turn read-only MCP server (DPES-1105 phase 6b).
//!
//! Only Claude Code is wired. Codex and Copilot stay design text until their forms are
//! verified. The server entry is the daemon's own `--mcp-relay` child; its environment
//! holds the per-turn secret, so a [`McpWiring`] never prints it and never leaves the
//! process except in the argv of the one turn's CLI.

use std::fmt;

use serde_json::{Map, Value};

/// The name the server is registered under; tool names are `mcp__mcao__<tool>`.
pub const MCP_SERVER_NAME: &str = "mcao";

/// The three read-only tools the server offers.
pub const MCP_TOOLS: [&str; 3] = ["task_detail", "session_events", "task_report"];

/// What a CLI needs to start the relay. Debug redacts the environment values.
pub struct McpWiring {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl fmt::Debug for McpWiring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpWiring")
            .field("command", &self.command)
            .field("args", &self.args)
            .field("env", &"[REDACTED]")
            .finish()
    }
}

impl McpWiring {
    /// The value of Claude's `--mcp-config`: one stdio server. Built by serde_json, so a
    /// quote or backslash in the executable path is escaped, never concatenated.
    pub fn claude_config_json(&self) -> String {
        let env: Map<String, Value> = self
            .env
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect();
        let mut server = Map::new();
        server.insert("type".into(), Value::String("stdio".into()));
        server.insert("command".into(), Value::String(self.command.clone()));
        server.insert(
            "args".into(),
            Value::Array(self.args.iter().cloned().map(Value::String).collect()),
        );
        server.insert("env".into(), Value::Object(env));
        let mut servers = Map::new();
        servers.insert(MCP_SERVER_NAME.into(), Value::Object(server));
        let mut root = Map::new();
        root.insert("mcpServers".into(), Value::Object(servers));
        Value::Object(root).to_string()
    }

    /// The `--allowedTools` entries: one `mcp__mcao__<tool>` per tool.
    pub fn claude_allowed_tools() -> Vec<String> {
        MCP_TOOLS
            .iter()
            .map(|tool| format!("mcp__{MCP_SERVER_NAME}__{tool}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiring(command: &str) -> McpWiring {
        McpWiring {
            command: command.to_owned(),
            args: vec!["--mcp-relay".into(), "--port".into(), "4242".into()],
            env: vec![("MCAO_MCP_SECRET".into(), "s3cr3t-value".into())],
        }
    }

    #[test]
    fn claude_config_json_has_the_one_server_and_the_env() {
        let value: Value =
            serde_json::from_str(&wiring("/usr/bin/daemon").claude_config_json()).unwrap();
        let servers = value["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 1);
        let server = &servers["mcao"];
        assert_eq!(server["type"], "stdio");
        assert_eq!(server["command"], "/usr/bin/daemon");
        assert_eq!(
            server["args"],
            serde_json::json!(["--mcp-relay", "--port", "4242"])
        );
        assert_eq!(
            server["env"],
            serde_json::json!({"MCAO_MCP_SECRET": "s3cr3t-value"})
        );
    }

    #[test]
    fn claude_config_json_round_trips_windows_paths_and_quotes() {
        for command in [
            r"C:\Program Files\MCAO\orchestrator-daemon.exe",
            r#"C:\Users\a "quoted" dir\daemon.exe"#,
            "/tmp/with space/and\"quote/daemon",
        ] {
            let json = wiring(command).claude_config_json();
            let value: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(value["mcpServers"]["mcao"]["command"], command, "{json}");
        }
    }

    #[test]
    fn the_allowed_tools_name_each_tool_under_the_server() {
        assert_eq!(
            McpWiring::claude_allowed_tools(),
            [
                "mcp__mcao__task_detail",
                "mcp__mcao__session_events",
                "mcp__mcao__task_report"
            ]
        );
    }

    #[test]
    fn wiring_debug_redacts_the_secret() {
        let shown = format!("{:?}", wiring("/usr/bin/daemon"));
        assert!(!shown.contains("s3cr3t-value"), "{shown}");
        assert!(shown.contains("REDACTED"), "{shown}");
    }
}
