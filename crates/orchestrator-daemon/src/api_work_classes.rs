//! DPES-677: named work classes, each with optional default agent, model and
//! effort, persisted in the data directory and applied when a task is submitted.
//!
//! * `settings.workClasses.get` returns the classes and a `state`.
//! * `settings.workClasses.set` replaces the whole set. Every class is checked
//!   against the live agent catalog first; one refused class refuses the call and
//!   writes nothing. The refusal is a typed result (`saved: false` with the
//!   class and field), because an RPC error carries no detail.
//! * `tasks.submit` takes an optional `work_class`. Each of agent, model and
//!   effort comes from the task when it names one, else from the class.
//!
//! Persistence follows `usage-token-warnings.json`: a small versioned JSON file
//! beside the database, not a table. The store holds task history and has no
//! settings table, so a table would mean a schema migration for one small document.
//! The file is replaced atomically (owner-only temp file, flushed, renamed). A file
//! that cannot be read is reported as `invalid_configuration` or `unavailable` and
//! behaves as no classes, so a broken file never blocks a submission without a class.
//!
//! A class with a model or effort but no agent is checked for shape only when it
//! is saved: the agent it is checked against is the task's, at submit time.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use orchestrator_agents::SelectionReason;
use orchestrator_core::contract::ModelSelection;
use orchestrator_store::{create_private_file, restrict_to_owner};
use serde::Deserialize;
use serde_json::{json, Value};

use super::models::check;
use super::*;
use crate::executor::log_safe;

pub(super) const CONFIG_FILE: &str = "work-classes.json";
const SCHEMA_VERSION: u8 = 1;
const MAX_CONFIG_BYTES: u64 = 32 * 1024;
const MAX_CLASSES: usize = 16;
const MAX_NAME_LEN: usize = 32;
const MAX_AGENT_LEN: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    schema_version: u8,
    classes: BTreeMap<String, Defaults>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassParams {
    name: String,
    #[serde(flatten)]
    defaults: Defaults,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    classes: Vec<ClassParams>,
}

enum Loaded {
    Off,
    Ok(BTreeMap<String, Defaults>),
    Invalid,
    Unavailable,
}

impl Loaded {
    fn state(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ok(_) => "ok",
            Self::Invalid => "invalid_configuration",
            Self::Unavailable => "unavailable",
        }
    }
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

/// The first structural problem in one class, as `(field, reason)`. Catalog
/// membership is not checked here.
fn shape_error(name: &str, defaults: &Defaults) -> Option<(&'static str, &'static str)> {
    if !is_valid_name(name) {
        return Some(("name", "invalid_name"));
    }
    if let Some(agent) = &defaults.agent {
        if agent.len() > MAX_AGENT_LEN || canonical_agent_id(agent).is_none() {
            return Some(("agent", "unknown_agent"));
        }
    }
    // Shape is the contract's own rule, so a class value that would be refused
    // on a task is refused here.
    let alone = |model: Option<&String>, effort: Option<&String>| {
        ModelSelection {
            model: model.cloned(),
            effort: effort.cloned(),
        }
        .validate()
        .is_err()
    };
    if defaults.model.is_some() && alone(defaults.model.as_ref(), None) {
        return Some(("model", "invalid_model"));
    }
    if defaults.effort.is_some() && alone(None, defaults.effort.as_ref()) {
        return Some(("effort", "invalid_effort"));
    }
    None
}

fn load(data_dir: &Path) -> Loaded {
    let path = data_dir.join(CONFIG_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Loaded::Off,
        Err(_) => return Loaded::Unavailable,
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Loaded::Invalid;
    }
    let mut bytes = Vec::new();
    match File::open(&path) {
        Ok(file) => {
            if file
                .take(MAX_CONFIG_BYTES + 1)
                .read_to_end(&mut bytes)
                .is_err()
            {
                return Loaded::Unavailable;
            }
        }
        Err(_) => return Loaded::Unavailable,
    }
    let Ok(stored) = serde_json::from_slice::<Stored>(&bytes) else {
        return Loaded::Invalid;
    };
    if bytes.len() as u64 > MAX_CONFIG_BYTES
        || stored.schema_version != SCHEMA_VERSION
        || stored.classes.len() > MAX_CLASSES
        || stored
            .classes
            .iter()
            .any(|(name, defaults)| shape_error(name, defaults).is_some())
    {
        return Loaded::Invalid;
    }
    Loaded::Ok(stored.classes)
}

/// Replaces the file atomically: an owner-only temp file in the same directory
/// is written and flushed, then renamed over the target.
fn store(data_dir: &Path, classes: &BTreeMap<String, Defaults>) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(&Stored {
        schema_version: SCHEMA_VERSION,
        classes: classes.clone(),
    })
    .map_err(std::io::Error::other)?;
    let temporary = data_dir.join(format!("{CONFIG_FILE}.{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| {
        create_private_file(&temporary)?;
        let mut file = fs::OpenOptions::new().write(true).open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, data_dir.join(CONFIG_FILE))?;
        restrict_to_owner(&data_dir.join(CONFIG_FILE))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

fn listing(loaded: &Loaded) -> Value {
    let classes: Vec<Value> = match loaded {
        Loaded::Ok(classes) => classes
            .iter()
            .map(|(name, defaults)| {
                json!({
                    "name": name,
                    "agent": defaults.agent,
                    "model": defaults.model,
                    "effort": defaults.effort,
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    json!({
        "schema_version": SCHEMA_VERSION,
        "state": loaded.state(),
        "classes": classes,
        "max_classes": MAX_CLASSES,
    })
}

fn refusal(class: Option<&str>, field: &str, reason: &str, detail: &str) -> Value {
    json!({
        "saved": false,
        "error": {
            "code": "invalid_work_class",
            "class": class,
            "field": field,
            "reason": reason,
            "detail": detail,
        },
    })
}

fn catalog_field(reason: SelectionReason) -> &'static str {
    match reason {
        SelectionReason::AgentUnavailable
        | SelectionReason::AuthenticationFailed
        | SelectionReason::AgentStateUnknown
        | SelectionReason::AgentUnsupported => "agent",
        SelectionReason::ModelsUnsupported
        | SelectionReason::ModelsUnknown
        | SelectionReason::UnknownModel => "model",
        SelectionReason::EffortRequiresModel
        | SelectionReason::EffortUnsupported
        | SelectionReason::EffortUnknown => "effort",
    }
}

/// What a submission resolved to. `class` is the requested class name, kept to
/// name it in a refusal.
pub(super) struct Resolved {
    pub(super) agent: String,
    pub(super) selection: Option<ModelSelection>,
    pub(super) class: Option<String>,
}

/// Field-wise: what the task names wins, then the class, then nothing. With no
/// class the task's own values pass through unchanged. `agent` is the task's
/// agent, already canonical, or blank when the task names none.
pub(super) fn resolve(
    data_dir: &Path,
    class: Option<&str>,
    agent: &str,
    selection: Option<&ModelSelection>,
) -> Result<Resolved, RpcError> {
    let Some(name) = class else {
        return Ok(Resolved {
            agent: agent.to_owned(),
            selection: selection.cloned(),
            class: None,
        });
    };
    let defaults = match load(data_dir) {
        Loaded::Ok(mut classes) => classes.remove(name),
        other => {
            eprintln!(
                "orchestrator-daemon: work class {} refused: work class settings are {}",
                log_safe(name),
                other.state()
            );
            None
        }
    };
    let Some(defaults) = defaults else {
        eprintln!(
            "orchestrator-daemon: work class {} refused: no such work class",
            log_safe(name)
        );
        return Err(RpcError::InvalidParams);
    };
    let agent = if agent.trim().is_empty() {
        defaults.agent.unwrap_or_default()
    } else {
        agent.to_owned()
    };
    // A selection that names nothing is left for the contract check to refuse.
    let explicit_empty = selection.is_some_and(|s| s.model.is_none() && s.effort.is_none());
    let selection = if explicit_empty {
        selection.cloned()
    } else {
        let model = selection.and_then(|s| s.model.clone()).or(defaults.model);
        let effort = selection.and_then(|s| s.effort.clone()).or(defaults.effort);
        (model.is_some() || effort.is_some()).then_some(ModelSelection { model, effort })
    };
    Ok(Resolved {
        agent,
        selection,
        class: Some(name.to_owned()),
    })
}

impl DaemonApi {
    pub(super) fn work_classes_get(&self, params: Value) -> Result<Value, RpcError> {
        require_empty_params(&params)?;
        Ok(listing(&load(&self.config.data_dir)))
    }

    pub(super) fn work_classes_set(&self, params: Value) -> Result<Value, RpcError> {
        let params: SetParams = parse_params(params)?;
        if params.classes.len() > MAX_CLASSES {
            return Ok(refusal(
                None,
                "classes",
                "too_many_classes",
                &format!("at most {MAX_CLASSES} work classes are allowed"),
            ));
        }
        let mut classes = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for class in params.classes {
            if !seen.insert(class.name.clone()) {
                return Ok(refusal(
                    Some(&class.name),
                    "name",
                    "duplicate_name",
                    "a work class name may appear once",
                ));
            }
            if let Some((field, reason)) = shape_error(&class.name, &class.defaults) {
                let shown = is_valid_name(&class.name).then_some(class.name.as_str());
                return Ok(refusal(
                    shown,
                    field,
                    reason,
                    &format!("the {field} of this work class is not valid"),
                ));
            }
            let mut defaults = class.defaults;
            defaults.agent = defaults
                .agent
                .as_deref()
                .and_then(canonical_agent_id)
                .map(str::to_owned);
            classes.insert(class.name, defaults);
        }
        let mut catalogs = BTreeMap::new();
        for (name, defaults) in &classes {
            let Some(agent) = &defaults.agent else {
                continue;
            };
            if defaults.model.is_none() && defaults.effort.is_none() {
                continue;
            }
            let selection = ModelSelection {
                model: defaults.model.clone(),
                effort: defaults.effort.clone(),
            };
            let catalog = catalogs
                .entry(agent.clone())
                .or_insert_with(|| self.catalog(agent));
            if let Err(error) = check(catalog, &selection) {
                return Ok(refusal(
                    Some(name),
                    catalog_field(error.reason),
                    &serde_json::to_value(error.reason)
                        .ok()
                        .and_then(|reason| reason.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    &error.detail,
                ));
            }
        }
        if store(&self.config.data_dir, &classes).is_err() {
            return Err(RpcError::Internal);
        }
        let mut result = listing(&Loaded::Ok(classes));
        result["saved"] = json!(true);
        Ok(result)
    }

    /// Submit-time gate for a resolved selection. The refusal is the same as
    /// `check_selection`; the class is added to the daemon log, since an RPC
    /// error carries no detail.
    pub(super) fn check_resolved_selection(
        &self,
        class: Option<&str>,
        catalog: &orchestrator_agents::AgentCatalog,
        selection: &ModelSelection,
    ) -> Result<(), RpcError> {
        self.check_selection(catalog, selection).map_err(|error| {
            if let Some(class) = class {
                eprintln!(
                    "orchestrator-daemon: work class {} resolved to a selection that was refused",
                    log_safe(class)
                );
            }
            error
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults(agent: Option<&str>, model: Option<&str>, effort: Option<&str>) -> Defaults {
        Defaults {
            agent: agent.map(str::to_owned),
            model: model.map(str::to_owned),
            effort: effort.map(str::to_owned),
        }
    }

    fn selection(model: Option<&str>, effort: Option<&str>) -> Option<ModelSelection> {
        Some(ModelSelection {
            model: model.map(str::to_owned),
            effort: effort.map(str::to_owned),
        })
    }

    #[test]
    fn names_are_lowercase_slugs_of_at_most_32_characters() {
        for good in ["debugging", "a", "doc-2", "-", &"x".repeat(32)] {
            assert!(is_valid_name(good), "{good}");
        }
        for bad in ["", "Debug", "a_b", "a b", "é", "a/b", &"x".repeat(33)] {
            assert!(!is_valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn shape_errors_name_the_field() {
        assert_eq!(shape_error("ok", &defaults(None, None, None)), None);
        assert_eq!(
            shape_error("Bad", &defaults(None, None, None)),
            Some(("name", "invalid_name"))
        );
        assert_eq!(
            shape_error("c", &defaults(Some("nope"), None, None)),
            Some(("agent", "unknown_agent"))
        );
        assert_eq!(
            shape_error("c", &defaults(Some("codex"), Some("-bad"), None)),
            Some(("model", "invalid_model"))
        );
        assert_eq!(
            shape_error("c", &defaults(Some("codex"), Some("m"), Some("HIGH"))),
            Some(("effort", "invalid_effort"))
        );
    }

    #[test]
    fn a_missing_file_is_off_and_a_bad_file_is_disclosed_not_fatal() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(load(directory.path()).state(), "off");
        for text in [
            "not json",
            r#"{"schema_version":2,"classes":{}}"#,
            r#"{"schema_version":1,"classes":{"Bad":{}}}"#,
            r#"{"schema_version":1,"classes":{"a":{"agent":"nope"}}}"#,
            r#"{"schema_version":1,"classes":{"a":{"extra":1}}}"#,
        ] {
            fs::write(directory.path().join(CONFIG_FILE), text).unwrap();
            let loaded = load(directory.path());
            assert_eq!(loaded.state(), "invalid_configuration", "{text}");
            assert_eq!(listing(&loaded)["classes"], json!([]));
        }
        fs::write(directory.path().join(CONFIG_FILE), vec![b' '; 40_000]).unwrap();
        assert_eq!(load(directory.path()).state(), "invalid_configuration");
    }

    #[test]
    fn store_round_trips_atomically_and_leaves_no_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let mut classes = BTreeMap::new();
        classes.insert("debugging".to_owned(), defaults(Some("codex"), None, None));
        classes.insert("review".to_owned(), defaults(None, Some("m"), Some("high")));
        store(directory.path(), &classes).unwrap();
        let Loaded::Ok(read) = load(directory.path()) else {
            panic!("file should load");
        };
        assert_eq!(read, classes);
        let names: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [CONFIG_FILE]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(directory.path().join(CONFIG_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn resolution_is_field_wise_and_the_task_wins() {
        let directory = tempfile::tempdir().unwrap();
        let mut classes = BTreeMap::new();
        classes.insert(
            "full".to_owned(),
            defaults(Some("codex"), Some("class-model"), Some("high")),
        );
        store(directory.path(), &classes).unwrap();
        let data = directory.path();

        let none = resolve(
            data,
            None,
            "claude-code",
            selection(Some("m"), None).as_ref(),
        )
        .unwrap();
        assert_eq!(none.agent, "claude-code");
        assert_eq!(none.selection, selection(Some("m"), None));
        assert_eq!(none.class, None);

        let all = resolve(data, Some("full"), "", None).unwrap();
        assert_eq!(all.agent, "codex");
        assert_eq!(all.selection, selection(Some("class-model"), Some("high")));

        let mixed = resolve(
            data,
            Some("full"),
            "claude-code",
            selection(Some("task-model"), None).as_ref(),
        )
        .unwrap();
        assert_eq!(mixed.agent, "claude-code");
        assert_eq!(mixed.selection, selection(Some("task-model"), Some("high")));

        let empty_explicit =
            resolve(data, Some("full"), "", selection(None, None).as_ref()).unwrap();
        assert_eq!(empty_explicit.selection, selection(None, None));

        assert!(matches!(
            resolve(data, Some("missing"), "codex", None),
            Err(RpcError::InvalidParams)
        ));
    }

    #[test]
    fn a_broken_settings_file_refuses_a_named_class_but_not_a_plain_submission() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(CONFIG_FILE), "not json").unwrap();
        assert!(matches!(
            resolve(directory.path(), Some("x"), "codex", None),
            Err(RpcError::InvalidParams)
        ));
        let plain = resolve(directory.path(), None, "codex", None).unwrap();
        assert_eq!(plain.agent, "codex");
    }
}
