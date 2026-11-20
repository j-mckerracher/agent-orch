//! Tool call events from each provider's real output shapes (`tests/fixtures/`).
//! The Codex file is assembled from captured `command_execution` items and the documented
//! `file_change`, `mcp_tool_call` and `web_search` items, because the local Codex CLI could
//! not run a model turn.

use orchestrator_agents::claude_code_adapter::StreamParser;
use orchestrator_agents::copilot_parser::CopilotParser;
use orchestrator_agents::{AgentEvent, Parser};

const CLAUDE: &str = include_str!("fixtures/claude/tool_use_read.jsonl");
const CODEX: &str = include_str!("fixtures/codex/tool_items.jsonl");
const COPILOT: &str = include_str!("fixtures/copilot/tool_view.jsonl");

fn claude_events(lines: &str) -> Vec<AgentEvent> {
    let mut parser = StreamParser::new();
    let mut events = Vec::new();
    for line in lines.lines() {
        events.extend(parser.feed(line).map(|event| event.expect("no violation")));
        while let Some(event) = parser.take_pending() {
            events.push(event);
        }
    }
    events
}

fn codex_events(lines: &str) -> Vec<AgentEvent> {
    let mut parser = Parser::new();
    let mut events = Vec::new();
    for line in lines.lines() {
        events.extend(parser.feed(line).map(|event| event.expect("no violation")));
        while let Some(event) = parser.take_pending() {
            events.push(event);
        }
    }
    events
}

fn copilot_events(lines: &str) -> Vec<AgentEvent> {
    let mut parser = CopilotParser::new();
    let mut events = Vec::new();
    for line in lines.lines() {
        events.extend(parser.feed(line));
        while let Some(event) = parser.take_pending() {
            events.push(event);
        }
    }
    events
}

fn started(id: &str, name: &str, summary: &str) -> AgentEvent {
    AgentEvent::ToolStarted {
        tool_id: id.into(),
        name: name.into(),
        summary: summary.into(),
    }
}

fn finished(id: &str, success: bool, summary: &str) -> AgentEvent {
    AgentEvent::ToolFinished {
        tool_id: id.into(),
        success,
        summary: summary.into(),
    }
}

fn tool_events(events: Vec<AgentEvent>) -> Vec<AgentEvent> {
    events
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                AgentEvent::ToolStarted { .. } | AgentEvent::ToolFinished { .. }
            )
        })
        .collect()
}

#[test]
fn claude_tools_interleave_with_thinking_and_messages_in_stream_order() {
    let events = claude_events(CLAUDE);
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| match event {
            AgentEvent::SessionStarted { .. } => "session",
            AgentEvent::ThinkingDelta { .. } => "thinking",
            AgentEvent::ToolStarted { .. } => "tool_started",
            AgentEvent::ToolFinished { .. } => "tool_finished",
            AgentEvent::MessageDelta { .. } => "message",
            AgentEvent::TurnFinished { .. } => "turn",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "session",
            "thinking",
            "thinking",
            "tool_started",
            "tool_started",
            "tool_finished",
            "tool_finished",
            "message",
            "turn"
        ]
    );
}

#[test]
fn claude_reports_each_tool_once_with_success_and_failure() {
    let tools = tool_events(claude_events(CLAUDE));
    let ok = "toolu_01QfnewxXscREfKJH35EWsvk";
    let missing = "toolu_01VgMAkVUVeVuv7paNp3rTb8";
    assert_eq!(tools.len(), 4);
    assert_eq!(tools[0], started(ok, "Read", "/tmp/scratch/Cargo.toml"));
    assert_eq!(
        tools[1],
        started(missing, "Read", "/tmp/scratch/nonexistent.txt")
    );
    assert_eq!(
        tools[2],
        finished(
            missing,
            false,
            "File does not exist. Note: your current working directory is /tmp/scratch."
        )
    );
    match &tools[3] {
        AgentEvent::ToolFinished {
            tool_id,
            success: true,
            summary,
        } => {
            assert_eq!(tool_id, ok);
            assert!(
                summary.starts_with("1\t# Root Cargo workspace."),
                "{summary}"
            );
        }
        other => panic!("expected a successful finish, got {other:?}"),
    }
}

#[test]
fn claude_repeats_of_one_tool_id_produce_no_second_event() {
    let assistant = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#;
    let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a"}]}}"#;
    let events = claude_events(&[assistant, assistant, user, user].join("\n"));
    assert_eq!(
        events,
        [started("t1", "Bash", "ls"), finished("t1", true, "a")]
    );
}

#[test]
fn claude_two_blocks_in_one_message_both_surface() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Grep","input":{"pattern":"foo","path":"src"}},{"type":"text","text":"x"},{"type":"tool_use","id":"b","name":"WebFetch","input":{"url":"https://example.com"}}]}}"#;
    assert_eq!(
        claude_events(line),
        [
            started("a", "Grep", "foo"),
            started("b", "WebFetch", "https://example.com")
        ]
    );
    let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":[{"type":"text","text":"hit"},{"type":"text","text":"more"}]},{"type":"tool_result","tool_use_id":"b","is_error":true,"content":"404"}]}}"#;
    assert_eq!(
        claude_events(user),
        [
            finished("a", true, "hit\nmore"),
            finished("b", false, "404")
        ]
    );
}

#[test]
fn claude_cuts_long_input_and_result_on_a_character_boundary() {
    let command = "é".repeat(5000);
    let line = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"t","name":"Bash","input":{{"command":"{command}"}}}}]}}}}"#
    );
    match &claude_events(&line)[0] {
        AgentEvent::ToolStarted { summary, .. } => {
            assert_eq!(summary.len(), 2048);
            assert!(summary.chars().all(|c| c == 'é'));
        }
        other => panic!("expected ToolStarted, got {other:?}"),
    }
    let line = format!(
        r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"t","content":"{command}"}}]}}}}"#
    );
    match &claude_events(&line)[0] {
        AgentEvent::ToolFinished { summary, .. } => assert_eq!(summary.len(), 2048),
        other => panic!("expected ToolFinished, got {other:?}"),
    }
}

#[test]
fn claude_unknown_tool_shapes_are_ignored_without_error() {
    let lines = [
        r#"{"type":"assistant","message":{"content":"plain text"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"NoId"}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"server_tool_use","id":"s"}]}}"#,
        r#"{"type":"assistant"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"text","text":"hi"}]}}"#,
    ];
    assert!(claude_events(&lines.join("\n")).is_empty());
    let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"Odd","input":{"blob":[1,2]}}]}}"#;
    assert_eq!(claude_events(line), [started("t", "Odd", "")]);
    let line =
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t"}]}}"#;
    assert_eq!(claude_events(line), [finished("t", true, "")]);
}

#[test]
fn codex_tool_items_report_start_and_finish_with_success_from_status() {
    let tools = tool_events(codex_events(CODEX));
    let head = "/bin/zsh -lc 'head -n 3 Cargo.toml'";
    let ls = "/bin/zsh -lc 'ls nonexistent_dir'";
    assert_eq!(
        tools,
        [
            started("item_2", "shell", head),
            finished(
                "item_2",
                true,
                "# Root Cargo workspace.\n#\n# Dependency direction\n"
            ),
            started("item_3", "shell", ls),
            finished(
                "item_3",
                false,
                "ls: nonexistent_dir: No such file or directory\n"
            ),
            started(
                "item_4",
                "apply_patch",
                "/tmp/scratch/src/a.rs, /tmp/scratch/src/b.rs"
            ),
            finished("item_4", true, ""),
            started("item_5", "motif.tasks_list", "demo"),
            finished("item_5", false, "server unavailable"),
            started("item_6", "web_search", "rust cargo workspace dependencies"),
            finished("item_6", true, ""),
        ]
    );
}

#[test]
fn codex_tools_interleave_with_thinking_and_messages() {
    let kinds: Vec<&str> = codex_events(CODEX)
        .iter()
        .map(|event| match event {
            AgentEvent::SessionStarted { .. } => "session",
            AgentEvent::ThinkingDelta { .. } => "thinking",
            AgentEvent::MessageDelta { .. } => "message",
            AgentEvent::ToolStarted { .. } => "start",
            AgentEvent::ToolFinished { .. } => "finish",
            AgentEvent::TurnFinished { .. } => "turn",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "session", "thinking", "message", "start", "finish", "start", "finish", "start",
            "finish", "start", "finish", "start", "finish", "message", "turn"
        ]
    );
}

#[test]
fn codex_repeated_items_and_unknown_shapes_add_nothing() {
    let begin = [
        r#"{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}"#,
        r#"{"type":"turn.started"}"#,
    ];
    let start = r#"{"type":"item.started","item":{"id":"c","type":"command_execution","command":"ls","status":"in_progress"}}"#;
    let done = r#"{"type":"item.completed","item":{"id":"c","type":"command_execution","command":"ls","aggregated_output":"x","exit_code":0,"status":"completed"}}"#;
    let junk = [
        r#"{"type":"item.started","item":{"id":"x","type":"command_execution"}}"#,
        r#"{"type":"item.started","item":{"type":"web_search","query":"q"}}"#,
        r#"{"type":"item.started","item":{"id":"y","type":"mystery"}}"#,
        r#"{"type":"item.completed","item":{"id":"z","type":"file_change"}}"#,
    ];
    let mut lines: Vec<&str> = begin.to_vec();
    lines.extend([start, start, done, done]);
    lines.extend(junk);
    assert_eq!(
        tool_events(codex_events(&lines.join("\n"))),
        [started("c", "shell", "ls"), finished("c", true, "x")]
    );
}

#[test]
fn codex_cuts_a_long_command() {
    let command = "x".repeat(5000);
    let lines = format!(
        concat!(
            r#"{{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}}"#,
            "\n",
            r#"{{"type":"turn.started"}}"#,
            "\n",
            r#"{{"type":"item.started","item":{{"id":"c","type":"command_execution","command":"{}"}}}}"#,
        ),
        command
    );
    match &tool_events(codex_events(&lines))[0] {
        AgentEvent::ToolStarted { summary, .. } => assert_eq!(summary.len(), 2048),
        other => panic!("expected ToolStarted, got {other:?}"),
    }
}

#[test]
fn copilot_tools_report_in_stream_order_around_thinking_and_messages() {
    let events = copilot_events(COPILOT);
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| match event {
            AgentEvent::SessionStarted { .. } => "session",
            AgentEvent::ThinkingDelta { .. } => "thinking",
            AgentEvent::MessageDelta { .. } => "message",
            AgentEvent::ToolStarted { .. } => "start",
            AgentEvent::ToolFinished { .. } => "finish",
            AgentEvent::TurnFinished { .. } => "turn",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        ["session", "thinking", "start", "start", "finish", "finish", "message", "turn"]
    );
}

#[test]
fn copilot_tool_start_and_finish_carry_summaries_and_success() {
    let tools = tool_events(copilot_events(COPILOT));
    let ok = "toolu_01GpW1oXUsVi9k1rcTf1SNRM";
    let missing = "toolu_01TwuUM8N8RgQhbe1qEhRm3J";
    assert_eq!(tools[0], started(ok, "view", "/tmp/scratch/Cargo.toml"));
    assert_eq!(
        tools[1],
        started(missing, "view", "/tmp/scratch/nonexistent.txt")
    );
    assert_eq!(
        tools[2],
        finished(
            missing,
            false,
            "Path /tmp/scratch/nonexistent.txt does not exist. Please provide a valid path."
        )
    );
    match &tools[3] {
        AgentEvent::ToolFinished {
            tool_id,
            success: true,
            summary,
        } => {
            assert_eq!(tool_id, ok);
            assert!(summary.starts_with("# Root Cargo workspace."), "{summary}");
        }
        other => panic!("expected a successful finish, got {other:?}"),
    }
}

#[test]
fn copilot_duplicates_long_input_and_unknown_shapes() {
    let start = r#"{"type":"tool.execution_start","data":{"toolCallId":"t","toolName":"bash","arguments":{"command":"ls"}}}"#;
    let done = r#"{"type":"tool.execution_complete","data":{"toolCallId":"t","success":true,"result":{"content":"a"}}}"#;
    assert_eq!(
        copilot_events(&[start, start, done, done].join("\n")),
        [started("t", "bash", "ls"), finished("t", true, "a")]
    );

    let long = format!(
        r#"{{"type":"tool.execution_start","data":{{"toolCallId":"l","toolName":"bash","arguments":{{"command":"{}"}}}}}}"#,
        "y".repeat(5000)
    );
    match &copilot_events(&long)[0] {
        AgentEvent::ToolStarted { summary, .. } => assert_eq!(summary.len(), 2048),
        other => panic!("expected ToolStarted, got {other:?}"),
    }

    let junk = [
        r#"{"type":"tool.execution_start"}"#,
        r#"{"type":"tool.execution_start","data":{"toolName":"bash"}}"#,
        r#"{"type":"tool.execution_complete","data":{"success":true}}"#,
        r#"{"type":"tool.execution_partial_result","data":{"toolCallId":"t"}}"#,
        "not json",
    ];
    assert!(copilot_events(&junk.join("\n")).is_empty());
    let bare = r#"{"type":"tool.execution_complete","data":{"toolCallId":"e","success":false,"error":{"message":"denied"}}}"#;
    assert_eq!(copilot_events(bare), [finished("e", false, "denied")]);
}
