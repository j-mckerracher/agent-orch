//! Raw one-line summaries of tool calls, bounded before they leave an adapter.
//!
//! Nothing here redacts. The daemon redacts and re-bounds before anything is stored or shown.

use serde_json::Value;

/// Longest summary an adapter emits, in bytes.
pub(crate) const SUMMARY_MAX_BYTES: usize = 2048;

/// Input fields that identify a call, best first. `Grep` and `Glob` carry both a pattern and a
/// path, so the pattern ranks above the path.
const IDENTIFYING_KEYS: [&str; 7] = [
    "command",
    "pattern",
    "url",
    "query",
    "file_path",
    "notebook_path",
    "path",
];

/// Cut `text` to [`SUMMARY_MAX_BYTES`] on a character boundary.
pub(crate) fn bound(text: &str) -> String {
    let mut end = text.len().min(SUMMARY_MAX_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// The field of a tool input that names the call: a command, pattern, URL, query or path.
/// Empty when the input has none of those; other fields may hold secrets or content.
pub(crate) fn input_summary(input: &Value) -> String {
    let Some(object) = input.as_object() else {
        return String::new();
    };
    IDENTIFYING_KEYS
        .iter()
        .find_map(|key| object.get(*key)?.as_str())
        .map(bound)
        .unwrap_or_default()
}

/// Result text from a plain string or an array of `{"type": "text", "text": ...}` blocks, bounded.
pub(crate) fn result_summary(content: &Value) -> String {
    match content {
        Value::String(text) => bound(text),
        Value::Array(blocks) => {
            let mut joined = String::new();
            for text in blocks
                .iter()
                .filter_map(|block| block.get("text")?.as_str())
            {
                if joined.len() >= SUMMARY_MAX_BYTES {
                    break;
                }
                if !joined.is_empty() {
                    joined.push('\n');
                }
                joined.push_str(text);
            }
            bound(&joined)
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bound_cuts_on_a_character_boundary() {
        let text = "é".repeat(SUMMARY_MAX_BYTES);
        let cut = bound(&text);
        assert_eq!(cut.len(), SUMMARY_MAX_BYTES);
        assert_eq!(bound("short"), "short");
        let odd = format!("a{}", "é".repeat(SUMMARY_MAX_BYTES));
        assert_eq!(bound(&odd).len(), SUMMARY_MAX_BYTES - 1);
    }

    #[test]
    fn input_summary_picks_the_identifying_field() {
        assert_eq!(
            input_summary(&json!({"file_path": "/a", "content": "x"})),
            "/a"
        );
        assert_eq!(
            input_summary(&json!({"command": "ls", "description": "d"})),
            "ls"
        );
        assert_eq!(
            input_summary(&json!({"path": "/p", "pattern": "*.rs"})),
            "*.rs"
        );
        assert_eq!(input_summary(&json!({"url": "https://x"})), "https://x");
        assert_eq!(input_summary(&json!({"description": "do it", "n": 1})), "");
        assert_eq!(
            input_summary(&json!({"password": "hunter2", "content": "short secret"})),
            ""
        );
        assert_eq!(input_summary(&json!(["not", "an", "object"])), "");
    }

    #[test]
    fn result_summary_reads_strings_and_text_blocks() {
        assert_eq!(result_summary(&json!("plain")), "plain");
        let blocks = json!([{"type": "text", "text": "a"}, {"type": "image"}, {"type": "text", "text": "b"}]);
        assert_eq!(result_summary(&blocks), "a\nb");
        assert_eq!(result_summary(&json!(null)), "");
    }
}
