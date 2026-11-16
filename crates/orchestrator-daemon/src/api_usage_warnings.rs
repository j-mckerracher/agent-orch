//! WI-DPES-747: optional warnings over recorded tokens, without pricing or execution authority.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::num::NonZeroU64;
use std::path::Path;

use orchestrator_store::{UsageFigures, UsageWindowSummary};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) const CONFIG_FILE: &str = "usage-token-warnings.json";
const MAX_CONFIG_BYTES: u64 = 16 * 1024;
const MAX_RULES: usize = 32;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
enum Metric {
    #[serde(rename = "input_tokens")]
    Input,
    #[serde(rename = "cache_read_tokens")]
    CacheRead,
    #[serde(rename = "cache_write_tokens")]
    CacheWrite,
    #[serde(rename = "output_tokens")]
    Output,
}

impl Metric {
    fn name(self) -> &'static str {
        match self {
            Self::Input => "input_tokens",
            Self::CacheRead => "cache_read_tokens",
            Self::CacheWrite => "cache_write_tokens",
            Self::Output => "output_tokens",
        }
    }

    fn count(self, figures: &UsageFigures) -> Option<u64> {
        match self {
            Self::Input => figures.input_tokens,
            Self::CacheRead => figures.cache_read_tokens,
            Self::CacheWrite => figures.cache_write_tokens,
            Self::Output => figures.output_tokens,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    provider: String,
    metric: Metric,
    limit: NonZeroU64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u8,
    rules: Vec<Rule>,
}

enum ConfigError {
    Invalid,
    Unavailable,
}

fn load(data_dir: &Path) -> Result<Option<Config>, ConfigError> {
    let path = data_dir.join(CONFIG_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ConfigError::Unavailable),
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::Invalid);
    }
    let file = File::open(path).map_err(|_| ConfigError::Unavailable)?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ConfigError::Unavailable)?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(ConfigError::Invalid);
    }
    let config: Config = serde_json::from_slice(&bytes).map_err(|_| ConfigError::Invalid)?;
    if config.schema_version != 1 || config.rules.len() > MAX_RULES {
        return Err(ConfigError::Invalid);
    }
    let mut seen = BTreeSet::new();
    for rule in &config.rules {
        if rule.provider.is_empty()
            || rule.provider.len() > 64
            || !rule
                .provider
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            || !seen.insert((&rule.provider, rule.metric))
        {
            return Err(ConfigError::Invalid);
        }
    }
    Ok(Some(config))
}

pub(super) fn evaluate(data_dir: &Path, summary: &UsageWindowSummary) -> Value {
    let config = match load(data_dir) {
        Ok(None) => return json!({"schema_version": 1, "state": "off", "rules": []}),
        Ok(Some(config)) => config,
        Err(error) => {
            let state = match error {
                ConfigError::Invalid => "invalid_configuration",
                ConfigError::Unavailable => "unavailable",
            };
            return json!({"schema_version": 1, "state": state, "rules": []});
        }
    };
    let rules: Vec<Value> = config
        .rules
        .iter()
        .map(|rule| {
            let observed = summary
                .providers
                .iter()
                .find(|provider| provider.provider == rule.provider)
                .and_then(|provider| rule.metric.count(&provider.bucket.reported));
            json!({
                "provider": rule.provider,
                "metric": rule.metric.name(),
                "limit": rule.limit.to_string(),
                "observed": observed.map(|count| count.to_string()),
                "exceeded": observed.map(|count| count > rule.limit.get()),
            })
        })
        .collect();
    json!({"schema_version": 1, "state": "ok", "rules": rules})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_configuration_is_off_and_bad_configuration_is_disclosed() {
        let directory = tempfile::tempdir().unwrap();
        let summary = UsageWindowSummary::default();
        assert_eq!(evaluate(directory.path(), &summary)["state"], "off");
        for text in [
            "not json",
            r#"{"schema_version":2,"rules":[]}"#,
            r#"{"schema_version":1,"rules":[{"provider":"codex","metric":"output_tokens","limit":0}]}"#,
            r#"{"schema_version":1,"rules":[{"provider":"codex","metric":"USD","limit":5}]}"#,
            r#"{"schema_version":1,"rules":[{"provider":"codex","metric":"reasoning_tokens","limit":5}]}"#,
            r#"{"schema_version":1,"rules":[{"provider":"codex","metric":"output_tokens","limit":5},{"provider":"codex","metric":"output_tokens","limit":6}]}"#,
        ] {
            fs::write(directory.path().join(CONFIG_FILE), text).unwrap();
            let result = evaluate(directory.path(), &summary);
            assert_eq!(result["state"], "invalid_configuration");
            assert_eq!(result["rules"], json!([]));
        }
        fs::write(directory.path().join(CONFIG_FILE), vec![b' '; 16_385]).unwrap();
        assert_eq!(
            evaluate(directory.path(), &summary)["state"],
            "invalid_configuration"
        );
    }
}
