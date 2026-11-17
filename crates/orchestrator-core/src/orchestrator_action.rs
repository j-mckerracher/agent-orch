//! Action proposals parsed from an orchestrator reply (DPES-1105).
//!
//! A completed reply may end with one `mcao-actions` fenced block. This module
//! holds the pure parts: [`ReplyFilter`] keeps the block out of the streamed
//! reply, [`parse_block`] validates it strictly, and [`Action::canonical`]
//! gives the form, digest, and supersede subject the store deduplicates on.
//! A parsed block carries no authority. The store records pending proposals,
//! and only a later user approval can act on them.
//!
//! Design: `docs/design/orchestrator-actions.md`.

use crate::approval::ParameterDigest;
use crate::authorization::Authorization;
use crate::contract::{DeliveryPolicy, TaskKind};
use crate::task_envelope::{check_task, Denial};
use crate::workstream::{
    validate_id, validate_text, validate_title, WorkstreamRuleError, WorkstreamState,
    INITIAL_REVISION, MAX_GATES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Info string of the fence that wraps an action block.
pub const ACTION_FENCE: &str = "```mcao-actions";
/// Largest block body, between the fences.
pub const MAX_ACTION_BLOCK_BYTES: usize = 16 * 1024;
/// Most actions one block may hold.
pub const MAX_ACTIONS_PER_BLOCK: usize = 10;
/// Longest `ref` name, in characters.
pub const MAX_REF_CHARS: usize = 32;
/// Longest proposed task objective, in bytes. The contract allows 64 KiB; the
/// card shows the whole text and the canonical JSON must fit the store's
/// `params_json` limit (20 KiB), so chat proposals stop at 8 KiB.
pub const MAX_TASK_OBJECTIVE_BYTES: usize = 8 * 1024;
/// Agents a chat proposal may name. Mirrors `canonicalAgentId` in
/// `apps/desktop/src/ui/submission.ts`.
pub const TASK_AGENTS: [&str; 2] = ["codex", "claude-code"];

/// Most text the filter holds: the block plus its fences and slack.
const HELD_LIMIT: usize = MAX_ACTION_BLOCK_BYTES + 256;

/// Kind of action a block may propose. Wire names are the `kind` strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    WorkstreamCreate,
    WorkstreamUpdate,
    ObjectiveAdd,
    ObjectiveRevise,
    TaskPropose,
    TaskCancel,
    TaskRetry,
    WorkstreamLinkTask,
}

impl ActionKind {
    /// Every kind, in wire order.
    pub const ALL: [Self; 8] = [
        Self::WorkstreamCreate,
        Self::WorkstreamUpdate,
        Self::ObjectiveAdd,
        Self::ObjectiveRevise,
        Self::TaskPropose,
        Self::TaskCancel,
        Self::TaskRetry,
        Self::WorkstreamLinkTask,
    ];

    /// Wire name, such as `workstream.create`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkstreamCreate => "workstream.create",
            Self::WorkstreamUpdate => "workstream.update",
            Self::ObjectiveAdd => "objective.add",
            Self::ObjectiveRevise => "objective.revise",
            Self::TaskPropose => "task.propose",
            Self::TaskCancel => "task.cancel",
            Self::TaskRetry => "task.retry",
            Self::WorkstreamLinkTask => "workstream.link_task",
        }
    }

    /// Parses a wire name.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

/// One action in a block, tagged by its wire `kind`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind")]
pub enum Action {
    #[serde(rename = "workstream.create")]
    WorkstreamCreate(CreateWorkstream),
    #[serde(rename = "workstream.update")]
    WorkstreamUpdate(UpdateWorkstream),
    #[serde(rename = "objective.add")]
    ObjectiveAdd(AddObjective),
    #[serde(rename = "objective.revise")]
    ObjectiveRevise(ReviseObjective),
    #[serde(rename = "task.propose")]
    TaskPropose(ProposeTask),
    #[serde(rename = "task.cancel")]
    TaskCancel(CancelTask),
    #[serde(rename = "task.retry")]
    TaskRetry(RetryTask),
    #[serde(rename = "workstream.link_task")]
    WorkstreamLinkTask(LinkTask),
}

/// `workstream.create`. `ref` names the workstream for later actions in the block.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkstream {
    #[serde(rename = "ref", default, skip_serializing)]
    pub reference: Option<String>,
    pub title: String,
    #[serde(default)]
    pub summary: String,
}

/// `workstream.update`. Only the fields present change.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateWorkstream {
    pub workstream_id: String,
    pub expected_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<WorkstreamState>,
}

/// `objective.add`. Exactly one of `workstream_id` or `workstream_ref` is set.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AddObjective {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream_ref: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Gate descriptions. The store maps each to an acceptance gate.
    #[serde(default)]
    pub gates: Vec<String>,
}

/// `objective.revise`. Only the fields present change.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviseObjective {
    pub objective_id: String,
    pub expected_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub add_gates: Vec<String>,
}

fn default_task_kind() -> TaskKind {
    TaskKind::Implement
}

fn default_delivery() -> DeliveryPolicy {
    DeliveryPolicy::LocalReview
}

/// `task.propose`. At most one of `workstream_id` or `workstream_ref` is set;
/// `objective_id` needs `workstream_id`. Both defaults are written into the
/// canonical form, so the card shows what would run. A proposal never names
/// autonomy, verification, permission, or attempt limits: the envelope and the
/// daemon fix them.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProposeTask {
    /// The `p-xxxxxxxx` code the snapshot shows after a project's folder name.
    pub project: String,
    pub objective: String,
    pub agent: String,
    #[serde(default = "default_task_kind")]
    pub task_kind: TaskKind,
    #[serde(default = "default_delivery")]
    pub delivery: DeliveryPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workstream_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective_id: Option<String>,
}

/// `task.cancel`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelTask {
    pub task_id: String,
}

/// `task.retry`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetryTask {
    pub task_id: String,
}

/// `workstream.link_task`. Links an existing task to a workstream, and to one
/// of its objectives when `objective_id` is set.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkTask {
    pub workstream_id: String,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective_id: Option<String>,
}

/// A validated block. `parent(i)` is the index of the earlier
/// `workstream.create` that action `i`'s `workstream_ref` names.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionBlock {
    actions: Vec<Action>,
    parents: Vec<Option<usize>>,
    /// The envelope's refusal of each action. All `None` until [`Self::checked`].
    denials: Vec<Option<Denial>>,
}

impl ActionBlock {
    /// Actions in block order.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// Index of the create action that action `index` refers to, if any.
    pub fn parent(&self, index: usize) -> Option<usize> {
        self.parents.get(index).copied().flatten()
    }

    /// The block with each `task.propose` checked against `envelope`.
    /// [`parse_block`] leaves every denial `None`, so a block that was never
    /// checked is never denied. The daemon calls this once, after the scope
    /// checks.
    pub fn checked(mut self, envelope: &Authorization) -> Self {
        self.denials = self
            .actions
            .iter()
            .map(|action| match action {
                Action::TaskPropose(propose) => check_task(propose, envelope).err(),
                _ => None,
            })
            .collect();
        self
    }

    /// The envelope's refusal of action `index`, if any. Always `None` for the other kinds.
    pub fn denial(&self, index: usize) -> Option<Denial> {
        self.denials.get(index).copied().flatten()
    }
}

/// Where a block was written. A workstream conversation may change only its own workstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockScope<'a> {
    Global,
    Workstream(&'a str),
}

/// Why a block was rejected. Diagnostics only; never stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockError {
    Unterminated,
    TextAfterBlock,
    MultipleBlocks,
    TooLarge,
    Malformed,
    Version,
    Empty,
    TooManyActions,
    BadReference,
    InvalidField(&'static str),
    OutOfScope,
}

impl BlockError {
    /// Stable diagnostic code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unterminated => "unterminated",
            Self::TextAfterBlock => "text_after_block",
            Self::MultipleBlocks => "multiple_blocks",
            Self::TooLarge => "too_large",
            Self::Malformed => "malformed",
            Self::Version => "version",
            Self::Empty => "empty",
            Self::TooManyActions => "too_many_actions",
            Self::BadReference => "bad_reference",
            Self::InvalidField(_) => "invalid_field",
            Self::OutOfScope => "out_of_scope",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    mcao_actions: u32,
    actions: Vec<Action>,
}

/// Parses held text, which starts at the fence line, into a validated block.
pub fn parse_block(held: &str, scope: BlockScope<'_>) -> Result<ActionBlock, BlockError> {
    let (first, after_first) = held.split_once('\n').unwrap_or((held, ""));
    if first.trim_end() != ACTION_FENCE {
        return Err(BlockError::Malformed);
    }
    let (body, remainder) = split_block(after_first).ok_or(BlockError::Unterminated)?;
    if remainder.lines().any(|line| line.starts_with(ACTION_FENCE)) {
        return Err(BlockError::MultipleBlocks);
    }
    if !remainder.trim().is_empty() {
        return Err(BlockError::TextAfterBlock);
    }
    if body.len() > MAX_ACTION_BLOCK_BYTES {
        return Err(BlockError::TooLarge);
    }
    let envelope: Envelope = serde_json::from_str(body).map_err(|_| BlockError::Malformed)?;
    if envelope.mcao_actions != 1 {
        return Err(BlockError::Version);
    }
    let actions = envelope.actions;
    if actions.is_empty() {
        return Err(BlockError::Empty);
    }
    if actions.len() > MAX_ACTIONS_PER_BLOCK {
        return Err(BlockError::TooManyActions);
    }
    let parents = resolve_refs(&actions)?;
    for action in &actions {
        check_fields(action)?;
    }
    check_scope(&actions, scope)?;
    let denials = vec![None; actions.len()];
    Ok(ActionBlock {
        actions,
        parents,
        denials,
    })
}

/// Splits the text after the fence line into the body and the text after the closing fence line.
fn split_block(text: &str) -> Option<(&str, &str)> {
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        if line.trim() == "```" {
            return Some((&text[..start], &text[start + line.len()..]));
        }
        start += line.len();
    }
    None
}

/// Whether a complete line is the block's opening fence.
fn is_fence_line(line: &str) -> bool {
    line.trim_end() == ACTION_FENCE
}

/// Whether a partial line could still become the opening fence line. Trailing
/// whitespace is allowed up to 64 bytes; a longer line is shown as text.
fn could_be_fence_line(partial: &str) -> bool {
    ACTION_FENCE.starts_with(partial)
        || (partial.len() <= ACTION_FENCE.len() + 64 && partial.trim_end() == ACTION_FENCE)
}

/// Whether held text (from the fence line on) has its closing fence line.
fn is_closed(held: &str) -> bool {
    held.split_once('\n')
        .is_some_and(|(_, after_fence)| split_block(after_fence).is_some())
}

/// Checks `ref` names and resolves each `workstream_ref` to the create action it names.
fn resolve_refs(actions: &[Action]) -> Result<Vec<Option<usize>>, BlockError> {
    let mut creates = HashMap::new();
    for (index, action) in actions.iter().enumerate() {
        if let Action::WorkstreamCreate(CreateWorkstream {
            reference: Some(reference),
            ..
        }) = action
        {
            let valid = !reference.is_empty() && reference.chars().count() <= MAX_REF_CHARS;
            if !valid || creates.insert(reference.as_str(), index).is_some() {
                return Err(BlockError::BadReference);
            }
        }
    }
    let resolve = |reference: &str, index: usize| match creates.get(reference) {
        Some(&create) if create < index => Ok(Some(create)),
        _ => Err(BlockError::BadReference),
    };
    actions
        .iter()
        .enumerate()
        .map(|(index, action)| match action {
            Action::ObjectiveAdd(add) => match (&add.workstream_id, &add.workstream_ref) {
                (Some(_), None) => Ok(None),
                (None, Some(reference)) => resolve(reference, index),
                _ => Err(BlockError::InvalidField("workstream")),
            },
            // A task names no workstream, an existing one, or one created earlier in the block.
            Action::TaskPropose(task) => match (&task.workstream_id, &task.workstream_ref) {
                (_, None) => Ok(None),
                (None, Some(reference)) => resolve(reference, index),
                (Some(_), Some(_)) => Err(BlockError::BadReference),
            },
            _ => Ok(None),
        })
        .collect()
}

/// Applies the workstream and objective field rules to one action.
fn check_fields(action: &Action) -> Result<(), BlockError> {
    match action {
        Action::WorkstreamCreate(create) => {
            check(validate_title("title", &create.title), "title")?;
            check(validate_text("summary", &create.summary, false), "summary")
        }
        Action::WorkstreamUpdate(update) => {
            check_revision(update.expected_revision)?;
            // An update that changes nothing is not a proposal.
            if update.title.is_none() && update.summary.is_none() && update.state.is_none() {
                return Err(BlockError::InvalidField("changes"));
            }
            check(
                validate_id("workstream_id", &update.workstream_id),
                "workstream_id",
            )?;
            if let Some(title) = &update.title {
                check(validate_title("title", title), "title")?;
            }
            if let Some(summary) = &update.summary {
                check(validate_text("summary", summary, false), "summary")?;
            }
            Ok(())
        }
        Action::ObjectiveAdd(add) => {
            if let Some(workstream_id) = &add.workstream_id {
                check(validate_id("workstream_id", workstream_id), "workstream_id")?;
            }
            check(validate_title("title", &add.title), "title")?;
            check(
                validate_text("description", &add.description, false),
                "description",
            )?;
            check_gates(&add.gates)
        }
        Action::ObjectiveRevise(revise) => {
            check_revision(revise.expected_revision)?;
            if revise.title.is_none() && revise.description.is_none() && revise.add_gates.is_empty()
            {
                return Err(BlockError::InvalidField("changes"));
            }
            check(
                validate_id("objective_id", &revise.objective_id),
                "objective_id",
            )?;
            if let Some(title) = &revise.title {
                check(validate_title("title", title), "title")?;
            }
            if let Some(description) = &revise.description {
                check(
                    validate_text("description", description, false),
                    "description",
                )?;
            }
            check_gates(&revise.add_gates)
        }
        Action::TaskPropose(propose) => check_propose(propose),
        Action::TaskCancel(cancel) => check(validate_id("task_id", &cancel.task_id), "task_id"),
        Action::TaskRetry(retry) => check(validate_id("task_id", &retry.task_id), "task_id"),
        Action::WorkstreamLinkTask(link) => {
            check(
                validate_id("workstream_id", &link.workstream_id),
                "workstream_id",
            )?;
            check(validate_id("task_id", &link.task_id), "task_id")?;
            if let Some(objective_id) = &link.objective_id {
                check(validate_id("objective_id", objective_id), "objective_id")?;
            }
            Ok(())
        }
    }
}

/// Applies the `task.propose` field rules. The workstream and ref pairing is
/// checked earlier, in `resolve_refs`.
fn check_propose(propose: &ProposeTask) -> Result<(), BlockError> {
    if !is_project_code(&propose.project) {
        return Err(BlockError::InvalidField("project"));
    }
    if propose.objective.trim().is_empty()
        || propose.objective.len() > MAX_TASK_OBJECTIVE_BYTES
        || propose.objective.chars().any(is_disallowed_objective_char)
    {
        return Err(BlockError::InvalidField("objective"));
    }
    if !TASK_AGENTS.contains(&propose.agent.as_str()) {
        return Err(BlockError::InvalidField("agent"));
    }
    if let Some(workstream_id) = &propose.workstream_id {
        check(validate_id("workstream_id", workstream_id), "workstream_id")?;
    }
    if let Some(objective_id) = &propose.objective_id {
        if propose.workstream_id.is_none() {
            return Err(BlockError::InvalidField("objective_id"));
        }
        check(validate_id("objective_id", objective_id), "objective_id")?;
    }
    Ok(())
}

/// Whether `code` is `p-` and exactly eight lowercase hex digits.
fn is_project_code(code: &str) -> bool {
    code.strip_prefix("p-").is_some_and(|digits| {
        digits.len() == 8 && digits.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    })
}

/// Whether `c` may not appear in a proposed task objective: any control
/// character except tab, LF and CR, and the bidirectional formatting controls.
/// The same set `contract::check_free_text` refuses; a test keeps the two in step.
fn is_disallowed_objective_char(c: char) -> bool {
    let bidi_control = matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    );
    (c.is_control() && !matches!(c, '\n' | '\r' | '\t')) || bidi_control
}

fn check(result: Result<(), WorkstreamRuleError>, field: &'static str) -> Result<(), BlockError> {
    result.map_err(|_| BlockError::InvalidField(field))
}

fn check_revision(expected_revision: i64) -> Result<(), BlockError> {
    if expected_revision < INITIAL_REVISION {
        return Err(BlockError::InvalidField("expected_revision"));
    }
    Ok(())
}

fn check_gates(gates: &[String]) -> Result<(), BlockError> {
    if gates.len() > MAX_GATES {
        return Err(BlockError::InvalidField("gates"));
    }
    for gate in gates {
        check(validate_text("gate", gate, true), "gates")?;
    }
    Ok(())
}

/// Refuses actions outside a workstream conversation's workstream.
fn check_scope(actions: &[Action], scope: BlockScope<'_>) -> Result<(), BlockError> {
    let BlockScope::Workstream(workstream) = scope else {
        return Ok(());
    };
    for action in actions {
        let inside = match action {
            Action::WorkstreamCreate(_) => false,
            Action::WorkstreamUpdate(update) => update.workstream_id == workstream,
            Action::ObjectiveAdd(add) => {
                add.workstream_ref.is_none() && add.workstream_id.as_deref() == Some(workstream)
            }
            // The owning workstream of a revised objective is checked against the store.
            Action::ObjectiveRevise(_) => true,
            // A task proposed here is always linked to this workstream.
            Action::TaskPropose(propose) => {
                propose.workstream_ref.is_none()
                    && propose.workstream_id.as_deref() == Some(workstream)
            }
            // The workstream that owns a cancelled or retried task is checked against the store.
            Action::TaskCancel(_) | Action::TaskRetry(_) => true,
            Action::WorkstreamLinkTask(link) => link.workstream_id == workstream,
        };
        if !inside {
            return Err(BlockError::OutOfScope);
        }
    }
    Ok(())
}

/// The canonical form of one action, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canonical {
    pub json: String,
    pub digest: ParameterDigest,
    pub subject: String,
}

impl Action {
    /// The kind of this action.
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::WorkstreamCreate(_) => ActionKind::WorkstreamCreate,
            Self::WorkstreamUpdate(_) => ActionKind::WorkstreamUpdate,
            Self::ObjectiveAdd(_) => ActionKind::ObjectiveAdd,
            Self::ObjectiveRevise(_) => ActionKind::ObjectiveRevise,
            Self::TaskPropose(_) => ActionKind::TaskPropose,
            Self::TaskCancel(_) => ActionKind::TaskCancel,
            Self::TaskRetry(_) => ActionKind::TaskRetry,
            Self::WorkstreamLinkTask(_) => ActionKind::WorkstreamLinkTask,
        }
    }

    /// Canonical JSON, digest, and supersede subject. `parent` is the effective
    /// proposal id that this action's `workstream_ref` resolves to.
    pub fn canonical(&self, parent: Option<&str>) -> Canonical {
        let mut value = serde_json::to_value(self).expect("action fields serialize to JSON");
        if let (Some(parent), Some(object)) = (parent, value.as_object_mut()) {
            if let Some(reference) = object.get_mut("workstream_ref") {
                *reference = Value::String(parent.to_owned());
            }
        }
        // Keys are sorted because serde_json keeps object keys in a BTreeMap
        // (the `preserve_order` feature is off); a test guards this.
        let json = value.to_string();
        let digest = ParameterDigest::from_sha256(Sha256::digest(json.as_bytes()).into());
        Canonical {
            json,
            digest,
            subject: self.subject(parent),
        }
    }

    /// The objective a `objective.revise` targets, for the daemon's workstream-scope check.
    pub fn revised_objective(&self) -> Option<&str> {
        match self {
            Self::ObjectiveRevise(revise) => Some(revise.objective_id.as_str()),
            _ => None,
        }
    }

    /// The task a `task.cancel` or `task.retry` names, which must already
    /// belong to the conversation's workstream: the daemon's workstream-scope
    /// check. A `workstream.link_task` is not listed, because the task it
    /// names is not linked yet.
    pub fn target_task(&self) -> Option<&str> {
        match self {
            Self::TaskCancel(cancel) => Some(cancel.task_id.as_str()),
            Self::TaskRetry(retry) => Some(retry.task_id.as_str()),
            _ => None,
        }
    }

    /// The workstream a `task.propose` (by id) or a `workstream.link_task` names.
    pub fn linked_workstream(&self) -> Option<&str> {
        match self {
            Self::TaskPropose(propose) => propose.workstream_id.as_deref(),
            Self::WorkstreamLinkTask(link) => Some(link.workstream_id.as_str()),
            _ => None,
        }
    }

    /// The objective a `task.propose` or a `workstream.link_task` names.
    pub fn linked_objective(&self) -> Option<&str> {
        match self {
            Self::TaskPropose(propose) => propose.objective_id.as_deref(),
            Self::WorkstreamLinkTask(link) => link.objective_id.as_deref(),
            _ => None,
        }
    }

    /// Supersede key: a pending proposal with the same kind and subject but a
    /// different digest is replaced by the newer one.
    fn subject(&self, parent: Option<&str>) -> String {
        match self {
            Self::WorkstreamCreate(create) => normalize(&create.title),
            Self::WorkstreamUpdate(update) => update.workstream_id.clone(),
            Self::ObjectiveAdd(add) => {
                let workstream = add.workstream_id.as_deref().or(parent).unwrap_or_default();
                format!("objective.add:{workstream}/{}", normalize(&add.title))
            }
            Self::ObjectiveRevise(revise) => revise.objective_id.clone(),
            // The hash keeps the subject inside the store's 512-byte limit.
            Self::TaskPropose(propose) => {
                let objective = Sha256::digest(normalize(&propose.objective).as_bytes());
                format!("{}/{}", propose.project, hex(&objective))
            }
            Self::TaskCancel(cancel) => cancel.task_id.clone(),
            Self::TaskRetry(retry) => retry.task_id.clone(),
            Self::WorkstreamLinkTask(link) => link.task_id.clone(),
        }
    }
}

/// Trimmed, lowercased, with inner whitespace collapsed to one space.
fn normalize(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Proposal id for the action at `idx` of a turn. Equal to the daemon's
/// `stable_id("proposal", "{turn_id}:{idx}")`.
pub fn proposal_id(turn_id: &str, idx: usize) -> String {
    let digest = Sha256::digest(format!("{turn_id}:{idx}").as_bytes());
    format!("proposal-{}", hex(&digest))
}

/// Lowercase hex of a parameter digest, as stored in `action_proposals.digest`.
pub fn digest_hex(digest: &ParameterDigest) -> String {
    hex(digest.as_bytes())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Splits streamed reply text into the part that may be shown and a held action block.
///
/// A block starts only at a line that is exactly the fence (trailing whitespace
/// allowed) at column zero. Text that could still become that line is held in
/// `line` until it can be told apart.
#[derive(Default)]
pub struct ReplyFilter {
    /// Partial line that could still become the fence.
    line: String,
    /// Whitespace-only lines not yet shown; they are dropped if a block follows.
    blank: String,
    /// Text from the fence line on, once a block has started.
    held: Option<String>,
    /// True when the current partial line already showed text, so no fence can start it.
    mid_line: bool,
    overflow: bool,
}

impl ReplyFilter {
    /// Returns the text that may be stored and streamed now (possibly empty).
    pub fn push(&mut self, text: &str) -> String {
        if self.overflow {
            return String::new();
        }
        if let Some(held) = &mut self.held {
            if held.len() + text.len() <= HELD_LIMIT {
                held.push_str(text);
                return String::new();
            }
            self.held = None;
            self.overflow = true;
            return String::new();
        }

        let buf = std::mem::take(&mut self.line) + text;
        let mut shown = String::new();
        let mut mid_line = self.mid_line;
        let mut rest = buf.as_str();
        while let Some(end) = rest.find('\n') {
            let line = &rest[..end];
            if !mid_line && is_fence_line(line) {
                self.hold(rest);
                return shown;
            }
            if !mid_line && line.trim().is_empty() {
                self.blank.push_str(line);
                self.blank.push('\n');
            } else {
                shown.push_str(&self.blank);
                shown.push_str(line);
                shown.push('\n');
                self.blank.clear();
            }
            mid_line = false;
            rest = &rest[end + 1..];
        }

        let seg = rest;
        if seg.is_empty() {
            // Blank lines stay pending until the next visible text or finish.
        } else if !mid_line && could_be_fence_line(seg) {
            self.line = seg.to_owned();
        } else {
            shown.push_str(&self.blank);
            self.blank.clear();
            shown.push_str(seg);
            mid_line = true;
        }
        self.mid_line = mid_line;
        shown
    }

    /// Ends the reply. `tail` is text still to show; `block` is the held block, if any.
    /// A held block without its closing fence was never a block, so it is shown.
    pub fn finish(self) -> FilterEnd {
        if self.overflow {
            return FilterEnd {
                tail: String::new(),
                block: Some(HeldBlock::Overflow),
            };
        }
        match self.held {
            Some(held) if is_closed(&held) => FilterEnd {
                tail: String::new(),
                block: Some(HeldBlock::Text(held)),
            },
            Some(held) => FilterEnd {
                tail: self.blank + &held,
                block: None,
            },
            None => FilterEnd {
                tail: self.blank + &self.line,
                block: None,
            },
        }
    }

    /// Starts holding a block whose text begins with `held`. `blank` is kept
    /// so it can be shown again if the block never closes.
    fn hold(&mut self, held: &str) {
        self.mid_line = false;
        if held.len() > HELD_LIMIT {
            self.overflow = true;
        } else {
            self.held = Some(held.to_owned());
        }
    }
}

/// What [`ReplyFilter::finish`] leaves: the last text to show, and the held block.
#[derive(Debug, PartialEq, Eq)]
pub struct FilterEnd {
    pub tail: String,
    pub block: Option<HeldBlock>,
}

/// A block the filter withheld from the reply.
#[derive(Debug, PartialEq, Eq)]
pub enum HeldBlock {
    /// The held text, from the fence line on.
    Text(String),
    /// The block exceeded the hold limit and was dropped.
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{validate_proposal, Validation};
    use crate::task_envelope::{chat_task_envelope, task_proposal};
    use serde_json::json;

    const CREATE: &str = r#"{"kind":"workstream.create","ref":"a","title":"Post-MVP work","summary":"Work after the MVP."}"#;
    const ADD_REF: &str = r#"{"kind":"objective.add","workstream_ref":"a","title":"Plan post-MVP scope","gates":["Scope list approved"]}"#;
    const PROPOSE: &str = r#"{"kind":"task.propose","project":"p-3fa9c1d2","objective":"Add unit tests for the retry module.","agent":"codex"}"#;
    const CANCEL: &str = r#"{"kind":"task.cancel","task_id":"task-1"}"#;
    const RETRY: &str = r#"{"kind":"task.retry","task_id":"task-1"}"#;
    const LINK: &str =
        r#"{"kind":"workstream.link_task","workstream_id":"ws-1","task_id":"task-1"}"#;

    /// A `task.propose` action: the valid base with `fields` set over it.
    fn propose(fields: &[(&str, Value)]) -> String {
        let mut action = json!({
            "kind": "task.propose",
            "project": "p-3fa9c1d2",
            "objective": "Add unit tests for the retry module.",
            "agent": "codex",
        });
        for (key, value) in fields {
            action[*key] = value.clone();
        }
        action.to_string()
    }

    fn fenced(body: &str) -> String {
        format!("{ACTION_FENCE}\n{body}\n```")
    }

    fn block(actions: &str) -> String {
        fenced(&format!("{{\"mcao_actions\":1,\"actions\":[{actions}]}}"))
    }

    fn parse(text: &str) -> Result<ActionBlock, BlockError> {
        parse_block(text, BlockScope::Global)
    }

    /// Feeds chunks through a filter; returns the shown text and the finish result.
    fn reply(chunks: &[&str]) -> (String, FilterEnd) {
        let mut filter = ReplyFilter::default();
        let mut shown = String::new();
        for chunk in chunks {
            shown.push_str(&filter.push(chunk));
        }
        (shown, filter.finish())
    }

    #[test]
    fn an_update_or_revise_that_changes_nothing_rejects_the_block() {
        for action in [
            r#"{"kind":"workstream.update","workstream_id":"ws-1","expected_revision":1}"#,
            r#"{"kind":"objective.revise","objective_id":"obj-1","expected_revision":1}"#,
        ] {
            assert_eq!(
                parse(&block(action)).err(),
                Some(BlockError::InvalidField("changes")),
                "{action}"
            );
        }
    }

    #[test]
    fn one_valid_block_parses_in_order() {
        let parsed = parse(&block(&format!("{CREATE},{ADD_REF}"))).expect("valid block");
        assert_eq!(parsed.actions().len(), 2);
        assert_eq!(parsed.actions()[0].kind(), ActionKind::WorkstreamCreate);
        assert_eq!(parsed.actions()[1].kind(), ActionKind::ObjectiveAdd);
        assert_eq!(parsed.parent(0), None);
        assert_eq!(parsed.parent(1), Some(0));
    }

    #[test]
    fn malformed_json_unknown_kind_unknown_field_duplicate_key_and_wrong_version_reject() {
        let cases = [
            (
                fenced(r#"{"mcao_actions":1,"actions":["#),
                BlockError::Malformed,
            ),
            (
                fenced(
                    r#"{"mcao_actions":1,"actions":[{"kind":"workstream.delete","title":"T"}]}"#,
                ),
                BlockError::Malformed,
            ),
            (
                fenced(
                    r#"{"mcao_actions":1,"actions":[{"kind":"workstream.create","title":"T","colour":"red"}]}"#,
                ),
                BlockError::Malformed,
            ),
            (
                fenced(
                    r#"{"mcao_actions":1,"actions":[{"kind":"workstream.create","title":"T","title":"U"}]}"#,
                ),
                BlockError::Malformed,
            ),
            (
                fenced(
                    r#"{"mcao_actions":1,"mcao_actions":1,"actions":[{"kind":"workstream.create","title":"T"}]}"#,
                ),
                BlockError::Malformed,
            ),
            (
                fenced(
                    r#"{"mcao_actions":1,"actions":[{"kind":"workstream.create","title":"T"}],"extra":true}"#,
                ),
                BlockError::Malformed,
            ),
            (
                fenced(
                    r#"{"mcao_actions":2,"actions":[{"kind":"workstream.create","title":"T"}]}"#,
                ),
                BlockError::Version,
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(parse(&text), Err(expected), "{text}");
        }
    }

    #[test]
    fn a_body_over_16_kib_rejects() {
        let body = format!(
            "{{\"mcao_actions\":1,\"actions\":[{CREATE}]}}{}",
            " ".repeat(MAX_ACTION_BLOCK_BYTES)
        );
        assert_eq!(parse(&fenced(&body)), Err(BlockError::TooLarge));
    }

    #[test]
    fn eleven_actions_reject_and_an_empty_list_rejects() {
        let eleven = (0..11)
            .map(|n| {
                format!(r#"{{"kind":"workstream.update","workstream_id":"ws-{n}","expected_revision":1,"title":"T"}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(parse(&block(&eleven)), Err(BlockError::TooManyActions));
        assert_eq!(parse(&block("")), Err(BlockError::Empty));
    }

    #[test]
    fn two_blocks_reject_both() {
        let text = format!("{}\n{}", block(CREATE), block(CREATE));
        assert_eq!(parse(&text), Err(BlockError::MultipleBlocks));
    }

    #[test]
    fn text_after_the_block_rejects_and_trailing_whitespace_does_not() {
        let after = format!("{}\nhello", block(CREATE));
        assert_eq!(parse(&after), Err(BlockError::TextAfterBlock));
        let trailing = format!("{}\n  \n", block(CREATE));
        assert!(parse(&trailing).is_ok());
    }

    #[test]
    fn an_unterminated_block_or_other_info_string_rejects() {
        let unterminated = format!("{ACTION_FENCE}\n{{\"mcao_actions\":1,\"actions\":[{CREATE}]}}");
        assert_eq!(parse(&unterminated), Err(BlockError::Unterminated));
        let other = format!("```json\n{{\"mcao_actions\":1,\"actions\":[{CREATE}]}}\n```");
        assert_eq!(parse(&other), Err(BlockError::Malformed));
        let versioned =
            format!("```mcao-actions-v2\n{{\"mcao_actions\":1,\"actions\":[{CREATE}]}}\n```");
        assert_eq!(parse(&versioned), Err(BlockError::Malformed));
    }

    #[test]
    fn workstream_ref_must_name_an_earlier_create() {
        let forward = format!("{ADD_REF},{CREATE}");
        assert_eq!(parse(&block(&forward)), Err(BlockError::BadReference));
        let unknown = format!("{CREATE},{}", ADD_REF.replace(r#""a""#, r#""zz""#));
        assert_eq!(parse(&block(&unknown)), Err(BlockError::BadReference));
        let duplicate = format!("{CREATE},{CREATE}");
        assert_eq!(parse(&block(&duplicate)), Err(BlockError::BadReference));
    }

    #[test]
    fn objective_add_needs_exactly_one_target() {
        let none = r#"{"kind":"objective.add","title":"Plan"}"#;
        assert_eq!(
            parse(&block(none)),
            Err(BlockError::InvalidField("workstream"))
        );
        let both = format!(
            "{CREATE},{}",
            r#"{"kind":"objective.add","workstream_id":"ws-1","workstream_ref":"a","title":"Plan"}"#
        );
        assert_eq!(
            parse(&block(&both)),
            Err(BlockError::InvalidField("workstream"))
        );
    }

    #[test]
    fn fields_follow_the_workstream_limits() {
        let long_title = format!(
            r#"{{"kind":"workstream.create","title":"{}"}}"#,
            "x".repeat(201)
        );
        assert_eq!(
            parse(&block(&long_title)),
            Err(BlockError::InvalidField("title"))
        );
        let blank_title = r#"{"kind":"workstream.create","title":"  "}"#;
        assert_eq!(
            parse(&block(blank_title)),
            Err(BlockError::InvalidField("title"))
        );
        let revision = r#"{"kind":"workstream.update","workstream_id":"ws-1","expected_revision":0,"title":"T"}"#;
        assert_eq!(
            parse(&block(revision)),
            Err(BlockError::InvalidField("expected_revision"))
        );
        let spaced_id = r#"{"kind":"workstream.update","workstream_id":"ws 1","expected_revision":1,"title":"T"}"#;
        assert_eq!(
            parse(&block(spaced_id)),
            Err(BlockError::InvalidField("workstream_id"))
        );
        let blank_gate =
            r#"{"kind":"objective.add","workstream_id":"ws-1","title":"Plan","gates":["  "]}"#;
        assert_eq!(
            parse(&block(blank_gate)),
            Err(BlockError::InvalidField("gates"))
        );
    }

    #[test]
    fn a_workstream_scope_refuses_create_and_foreign_targets() {
        let scope = BlockScope::Workstream("ws-1");
        let refused = [
            CREATE.to_owned(),
            r#"{"kind":"workstream.update","workstream_id":"ws-2","expected_revision":1,"title":"T"}"#
                .to_owned(),
            r#"{"kind":"objective.add","workstream_id":"ws-2","title":"Plan"}"#.to_owned(),
        ];
        for action in refused {
            assert_eq!(
                parse_block(&block(&action), scope),
                Err(BlockError::OutOfScope),
                "{action}"
            );
        }
        let allowed = [
            r#"{"kind":"workstream.update","workstream_id":"ws-1","expected_revision":1,"title":"T"}"#,
            r#"{"kind":"objective.add","workstream_id":"ws-1","title":"Plan"}"#,
            r#"{"kind":"objective.revise","objective_id":"obj-9","expected_revision":1,"title":"T"}"#,
        ];
        for action in allowed {
            assert!(parse_block(&block(action), scope).is_ok(), "{action}");
        }
        assert!(parse_block(&block(CREATE), BlockScope::Global).is_ok());
    }

    #[test]
    fn canonical_json_sorts_keys_and_omits_ref() {
        let parsed = parse(&block(CREATE)).expect("valid block");
        let canonical = parsed.actions()[0].canonical(None);
        assert_eq!(
            canonical.json,
            r#"{"kind":"workstream.create","summary":"Work after the MVP.","title":"Post-MVP work"}"#
        );
        assert_eq!(canonical.subject, "post-mvp work");
    }

    #[test]
    fn digest_ignores_ref_and_defaults_and_binds_the_resolved_parent() {
        let digest_of = |text: &str| {
            let parsed = parse(&block(text)).expect("valid block");
            parsed.actions()[0].canonical(None).digest
        };
        let bare = digest_of(r#"{"kind":"workstream.create","title":"Post-MVP work"}"#);
        let explicit = digest_of(
            r#"{"kind":"workstream.create","ref":"zz","title":"Post-MVP work","summary":""}"#,
        );
        assert_eq!(bare, explicit);
        assert_ne!(
            bare,
            digest_of(r#"{"kind":"workstream.create","title":"Other"}"#)
        );

        let parsed = parse(&block(&format!("{CREATE},{ADD_REF}"))).expect("valid block");
        let action = &parsed.actions()[1];
        let first = action.canonical(Some("proposal-1"));
        let second = action.canonical(Some("proposal-2"));
        assert_ne!(first.digest, second.digest);
        assert!(first.json.contains(r#""workstream_ref":"proposal-1""#));
        assert_eq!(
            first.subject,
            "objective.add:proposal-1/plan post-mvp scope"
        );
        assert_eq!(digest_hex(&first.digest).len(), 64);
    }

    #[test]
    fn kinds_round_trip_by_wire_name() {
        for kind in ActionKind::ALL {
            assert_eq!(ActionKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(ActionKind::parse("workstream.delete"), None);
    }

    #[test]
    fn the_action_kind_list_names_all_eight_kinds() {
        let names = ActionKind::ALL.map(ActionKind::as_str);
        assert_eq!(
            names,
            [
                "workstream.create",
                "workstream.update",
                "objective.add",
                "objective.revise",
                "task.propose",
                "task.cancel",
                "task.retry",
                "workstream.link_task",
            ]
        );
        assert_eq!(
            ActionKind::parse("task.propose"),
            Some(ActionKind::TaskPropose)
        );
        assert_eq!(ActionKind::parse("task.delete"), None);
        for text in [PROPOSE, CANCEL, RETRY, LINK] {
            let parsed = parse(&block(text)).expect("valid block");
            assert!(
                names.contains(&parsed.actions()[0].kind().as_str()),
                "{text}"
            );
        }
    }

    #[test]
    fn task_propose_parses_and_defaults_are_written_into_the_canonical_form() {
        let parsed = parse(&block(PROPOSE)).expect("valid block");
        let action = &parsed.actions()[0];
        assert_eq!(action.kind(), ActionKind::TaskPropose);
        let Action::TaskPropose(task) = action else {
            panic!("expected a task.propose action");
        };
        assert_eq!(task.task_kind, TaskKind::Implement);
        assert_eq!(task.delivery, DeliveryPolicy::LocalReview);
        assert_eq!(parsed.parent(0), None);
        assert_eq!(parsed.denial(0), None);
        assert_eq!(
            action.canonical(None).json,
            r#"{"agent":"codex","delivery":"LocalReview","kind":"task.propose","objective":"Add unit tests for the retry module.","project":"p-3fa9c1d2","task_kind":"Implement"}"#
        );

        let explicit = propose(&[
            ("task_kind", json!("Investigate")),
            ("delivery", json!("ReportOnly")),
            ("workstream_id", json!("ws-1")),
            ("objective_id", json!("obj-1")),
        ]);
        let parsed = parse(&block(&explicit)).expect("valid block");
        let action = &parsed.actions()[0];
        assert_eq!(action.linked_workstream(), Some("ws-1"));
        assert_eq!(action.linked_objective(), Some("obj-1"));
        assert_eq!(action.target_task(), None);
        assert_eq!(
            action.canonical(None).json,
            r#"{"agent":"codex","delivery":"ReportOnly","kind":"task.propose","objective":"Add unit tests for the retry module.","objective_id":"obj-1","project":"p-3fa9c1d2","task_kind":"Investigate","workstream_id":"ws-1"}"#
        );

        // `PullRequest` parses so that the envelope, not the parser, denies it.
        let pull_request = propose(&[("delivery", json!("PullRequest"))]);
        assert!(parse(&block(&pull_request)).is_ok());
        for (field, value) in [("delivery", "Merge"), ("task_kind", "Refactor")] {
            let bad = propose(&[(field, json!(value))]);
            assert_eq!(parse(&block(&bad)), Err(BlockError::Malformed), "{field}");
        }
    }

    #[test]
    fn an_omitted_delivery_and_an_explicit_local_review_share_one_digest() {
        let digest_of = |text: &str| {
            let parsed = parse(&block(text)).expect("valid block");
            parsed.actions()[0].canonical(None).digest
        };
        let bare = digest_of(&propose(&[]));
        let explicit = digest_of(&propose(&[
            ("delivery", json!("LocalReview")),
            ("task_kind", json!("Implement")),
        ]));
        assert_eq!(bare, explicit);
        assert_ne!(
            bare,
            digest_of(&propose(&[("delivery", json!("ReportOnly"))]))
        );
        assert_ne!(
            bare,
            digest_of(&propose(&[("task_kind", json!("Investigate"))]))
        );
    }

    #[test]
    fn task_propose_rejects_a_bad_project_code_an_empty_or_oversize_objective_a_control_character_and_an_unknown_agent(
    ) {
        let oversize = "x".repeat(MAX_TASK_OBJECTIVE_BYTES + 1);
        let rejected = [
            ("project", ""),
            ("project", "p-3fa9c1d"),
            ("project", "p-3fa9c1d22"),
            ("project", "p-3FA9C1D2"),
            ("project", "q-3fa9c1d2"),
            ("project", "p-3fa9c1dg"),
            ("project", "/Users/someone/repo"),
            ("objective", ""),
            ("objective", " \n\t "),
            ("objective", oversize.as_str()),
            ("objective", "a\u{0}b"),
            ("objective", "a\u{7}b"),
            ("objective", "a\u{1b}[31mb"),
            ("objective", "a\u{7f}b"),
            ("objective", "a\u{85}b"),
            ("objective", "a\u{202e}b"),
            ("objective", "a\u{200f}b"),
            ("objective", "a\u{2066}b"),
            ("agent", ""),
            ("agent", "gemini"),
            ("agent", "Codex"),
            ("agent", "claude"),
            ("agent", "codex claude-code"),
        ];
        for (field, value) in rejected {
            let text = propose(&[(field, json!(value))]);
            assert_eq!(
                parse(&block(&text)),
                Err(BlockError::InvalidField(field)),
                "{field}: {value:.40?}"
            );
        }

        let exactly_full = "y".repeat(MAX_TASK_OBJECTIVE_BYTES);
        let accepted = [
            ("project", "p-0123abcd"),
            ("objective", exactly_full.as_str()),
            ("objective", "Line one.\n\tLine two.\r\nLine three."),
            ("agent", "claude-code"),
        ];
        for (field, value) in accepted {
            let text = propose(&[(field, json!(value))]);
            assert!(parse(&block(&text)).is_ok(), "{field}: {value:.40?}");
        }
    }

    #[test]
    fn task_propose_refuses_both_workstream_id_and_ref_and_an_objective_with_a_ref() {
        let with_create = |fields: &[(&str, Value)]| format!("{CREATE},{}", propose(fields));
        let both = with_create(&[
            ("workstream_id", json!("ws-1")),
            ("workstream_ref", json!("a")),
        ]);
        assert_eq!(parse(&block(&both)), Err(BlockError::BadReference));

        let objective_with_ref = with_create(&[
            ("workstream_ref", json!("a")),
            ("objective_id", json!("obj-1")),
        ]);
        assert_eq!(
            parse(&block(&objective_with_ref)),
            Err(BlockError::InvalidField("objective_id"))
        );
        let objective_alone = propose(&[("objective_id", json!("obj-1"))]);
        assert_eq!(
            parse(&block(&objective_alone)),
            Err(BlockError::InvalidField("objective_id"))
        );

        let bad_workstream = propose(&[("workstream_id", json!("ws 1"))]);
        assert_eq!(
            parse(&block(&bad_workstream)),
            Err(BlockError::InvalidField("workstream_id"))
        );
        let bad_objective = propose(&[
            ("workstream_id", json!("ws-1")),
            ("objective_id", json!("obj 1")),
        ]);
        assert_eq!(
            parse(&block(&bad_objective)),
            Err(BlockError::InvalidField("objective_id"))
        );
        let linked = propose(&[
            ("workstream_id", json!("ws-1")),
            ("objective_id", json!("obj-1")),
        ]);
        assert!(parse(&block(&linked)).is_ok());
    }

    #[test]
    fn task_propose_may_name_a_workstream_create_ref_and_becomes_its_dependent() {
        let by_ref = propose(&[("workstream_ref", json!("a"))]);
        let parsed = parse(&block(&format!("{CREATE},{by_ref}"))).expect("valid block");
        assert_eq!(parsed.parent(0), None);
        assert_eq!(parsed.parent(1), Some(0));

        let action = &parsed.actions()[1];
        let first = action.canonical(Some("proposal-1"));
        let second = action.canonical(Some("proposal-2"));
        assert!(first.json.contains(r#""workstream_ref":"proposal-1""#));
        assert_ne!(first.digest, second.digest);
        assert_eq!(first.subject, second.subject);

        let unlinked = parse(&block(PROPOSE)).expect("valid block");
        assert_eq!(unlinked.parent(0), None);

        let forward = format!("{by_ref},{CREATE}");
        assert_eq!(parse(&block(&forward)), Err(BlockError::BadReference));
        let unknown = propose(&[("workstream_ref", json!("zz"))]);
        assert_eq!(
            parse(&block(&format!("{CREATE},{unknown}"))),
            Err(BlockError::BadReference)
        );
        assert_eq!(parse(&block(&by_ref)), Err(BlockError::BadReference));
    }

    #[test]
    fn task_cancel_retry_and_link_reject_unknown_fields_and_bad_ids() {
        let parsed = parse(&block(&format!("{CANCEL},{RETRY},{LINK}"))).expect("valid block");
        let [cancel, retry, link] = parsed.actions() else {
            panic!("expected three actions");
        };
        assert_eq!(cancel.target_task(), Some("task-1"));
        assert_eq!(retry.target_task(), Some("task-1"));
        // A linked task belongs to no workstream yet, so there is no target to check.
        assert_eq!(link.target_task(), None);
        assert_eq!(link.linked_workstream(), Some("ws-1"));
        assert_eq!(link.linked_objective(), None);
        assert_eq!(cancel.linked_workstream(), None);
        assert_eq!(
            cancel.canonical(None).json,
            r#"{"kind":"task.cancel","task_id":"task-1"}"#
        );
        assert_eq!(
            retry.canonical(None).json,
            r#"{"kind":"task.retry","task_id":"task-1"}"#
        );
        assert_eq!(
            link.canonical(None).json,
            r#"{"kind":"workstream.link_task","task_id":"task-1","workstream_id":"ws-1"}"#
        );

        for text in [CANCEL, RETRY, LINK, PROPOSE] {
            let extra = text.replace('}', r#","force":true}"#);
            assert_eq!(parse(&block(&extra)), Err(BlockError::Malformed), "{extra}");
        }
        // A proposal never carries policy, limits, dependencies, or a parent.
        for field in [
            "autonomy",
            "verification",
            "permission",
            "max_retries",
            "max_repairs",
            "dependencies",
            "parent_id",
            "model_selection",
        ] {
            let text = propose(&[(field, json!("x"))]);
            assert_eq!(parse(&block(&text)), Err(BlockError::Malformed), "{field}");
        }

        let missing = [
            r#"{"kind":"task.cancel"}"#,
            r#"{"kind":"task.retry"}"#,
            r#"{"kind":"workstream.link_task","task_id":"task-1"}"#,
            r#"{"kind":"workstream.link_task","workstream_id":"ws-1"}"#,
        ];
        for text in missing {
            assert_eq!(parse(&block(text)), Err(BlockError::Malformed), "{text}");
        }

        for bad in ["", "task 1", "t\u{e9}che", &"t".repeat(129)] {
            for kind in ["task.cancel", "task.retry"] {
                let text = json!({"kind": kind, "task_id": bad}).to_string();
                assert_eq!(
                    parse(&block(&text)),
                    Err(BlockError::InvalidField("task_id")),
                    "{kind} {bad:.20?}"
                );
            }
            let text =
                json!({"kind": "workstream.link_task", "workstream_id": "ws-1", "task_id": bad})
                    .to_string();
            assert_eq!(
                parse(&block(&text)),
                Err(BlockError::InvalidField("task_id")),
                "link {bad:.20?}"
            );
        }
        let bad_workstream =
            r#"{"kind":"workstream.link_task","workstream_id":"ws 1","task_id":"task-1"}"#;
        assert_eq!(
            parse(&block(bad_workstream)),
            Err(BlockError::InvalidField("workstream_id"))
        );
        let bad_objective = r#"{"kind":"workstream.link_task","workstream_id":"ws-1","task_id":"task-1","objective_id":"obj 1"}"#;
        assert_eq!(
            parse(&block(bad_objective)),
            Err(BlockError::InvalidField("objective_id"))
        );
        let objective = r#"{"kind":"workstream.link_task","workstream_id":"ws-1","task_id":"task-1","objective_id":"obj-1"}"#;
        let parsed = parse(&block(objective)).expect("valid block");
        assert_eq!(parsed.actions()[0].linked_objective(), Some("obj-1"));
    }

    #[test]
    fn in_a_workstream_conversation_propose_and_link_must_name_that_workstream() {
        let scope = BlockScope::Workstream("ws-1");
        let refused = [
            PROPOSE.to_owned(),
            propose(&[("workstream_id", json!("ws-2"))]),
            format!("{CREATE},{}", propose(&[("workstream_ref", json!("a"))])),
            LINK.replace("ws-1", "ws-2"),
        ];
        for action in refused {
            assert_eq!(
                parse_block(&block(&action), scope),
                Err(BlockError::OutOfScope),
                "{action}"
            );
        }
        let allowed = [
            propose(&[("workstream_id", json!("ws-1"))]),
            propose(&[
                ("workstream_id", json!("ws-1")),
                ("objective_id", json!("obj-1")),
            ]),
            LINK.to_owned(),
            // The owning workstream of a cancelled or retried task is checked against the store.
            CANCEL.to_owned(),
            RETRY.to_owned(),
        ];
        for action in allowed {
            assert!(parse_block(&block(&action), scope).is_ok(), "{action}");
        }
        let global = [
            PROPOSE.to_owned(),
            propose(&[("workstream_id", json!("ws-2"))]),
            CANCEL.to_owned(),
            RETRY.to_owned(),
            LINK.replace("ws-1", "ws-2"),
        ];
        for action in global {
            assert!(parse(&block(&action)).is_ok(), "{action}");
        }
    }

    #[test]
    fn a_task_subject_supersedes_only_the_same_kind_and_target() {
        let canonical_of = |text: &str| {
            let parsed = parse(&block(text)).expect("valid block");
            parsed.actions()[0].canonical(None)
        };
        let base = canonical_of(PROPOSE);
        let objective_hash = hex(&Sha256::digest(
            "add unit tests for the retry module.".as_bytes(),
        ));
        assert_eq!(base.subject, format!("p-3fa9c1d2/{objective_hash}"));
        assert!(base.subject.len() <= 512);

        // Same project and objective text up to case and spacing: same subject. A
        // different delivery changes the digest, so the newer proposal supersedes.
        let respaced = canonical_of(&propose(&[(
            "objective",
            json!("  ADD unit   tests for\nthe retry module. "),
        )]));
        let report = canonical_of(&propose(&[("delivery", json!("ReportOnly"))]));
        assert_eq!(respaced.subject, base.subject);
        assert_eq!(report.subject, base.subject);
        assert_ne!(report.digest, base.digest);

        let other_objective = canonical_of(&propose(&[("objective", json!("Fix the parser."))]));
        let other_project = canonical_of(&propose(&[("project", json!("p-00000000"))]));
        assert_ne!(other_objective.subject, base.subject);
        assert_ne!(other_project.subject, base.subject);

        // Cancel, retry and link subjects are the task id; the store keys on the kind too.
        let cancel = canonical_of(CANCEL);
        assert_eq!(cancel.subject, "task-1");
        assert_eq!(canonical_of(RETRY).subject, "task-1");
        assert_eq!(canonical_of(LINK).subject, "task-1");
        assert_eq!(
            canonical_of(&LINK.replace("ws-1", "ws-2")).subject,
            "task-1"
        );
        assert_ne!(
            canonical_of(&CANCEL.replace("task-1", "task-2")).subject,
            cancel.subject
        );
        assert_ne!(cancel.json, canonical_of(RETRY).json);
    }

    #[test]
    fn a_checked_block_denies_only_a_pull_request_task_propose() {
        let pull_request = propose(&[("delivery", json!("PullRequest"))]);
        let text = block(&format!("{CREATE},{PROPOSE},{pull_request},{CANCEL}"));
        let parsed = parse(&text).expect("valid block");
        assert!((0..4).all(|index| parsed.denial(index).is_none()));

        let checked = parsed.clone().checked(&chat_task_envelope());
        assert_eq!(checked.actions(), parsed.actions());
        assert_eq!(checked.parent(1), parsed.parent(1));
        let denials: Vec<_> = (0..5).map(|index| checked.denial(index)).collect();
        assert_eq!(denials, [None, None, Some(Denial::Delivery), None, None]);
    }

    #[test]
    fn the_objective_rule_refuses_the_same_characters_as_the_contract_check() {
        let Action::TaskPropose(base) =
            parse(&block(PROPOSE)).expect("valid block").actions()[0].clone()
        else {
            panic!("expected a task.propose action");
        };
        let envelope = chat_task_envelope();
        for c in (0..=0xFFFF).filter_map(char::from_u32) {
            let action = ProposeTask {
                objective: format!("a{c}b"),
                ..base.clone()
            };
            let verdict = validate_proposal(&task_proposal(&action, &envelope), &envelope);
            assert_eq!(
                matches!(verdict, Validation::Invalid(_)),
                is_disallowed_objective_char(c),
                "{c:?}"
            );
        }
    }

    #[test]
    fn proposal_id_is_prefixed_and_matches_sha256_of_turn_and_index() {
        assert_eq!(
            proposal_id("turn-1", 0),
            "proposal-ec4a47d6b5c54abe3a972735dd31406d0e40aa58c2c45d5c54d508867d917e92"
        );
        assert_ne!(proposal_id("turn-1", 0), proposal_id("turn-1", 1));
        assert_eq!(
            proposal_id("turn-1", 1),
            "proposal-b671056853c6cb328fe9da892712245819544c3a90428ad28662f8fa2cfd9b93"
        );
    }

    #[test]
    fn text_without_a_fence_passes_through_for_every_split() {
        let text = "Hello\n\nworld ``` not a fence\n";
        for split in 0..=text.len() {
            let (head, tail) = text.split_at(split);
            let (shown, end) = reply(&[head, tail]);
            assert_eq!(format!("{shown}{}", end.tail), text, "split {split}");
            assert_eq!(end.block, None, "split {split}");
        }
    }

    #[test]
    fn a_fence_split_at_every_byte_boundary_is_held() {
        let text = format!("Hi\n{}", block(CREATE));
        for split in 0..=text.len() {
            let (head, tail) = text.split_at(split);
            let (shown, end) = reply(&[head, tail]);
            assert_eq!(format!("{shown}{}", end.tail), "Hi\n", "split {split}");
            assert_eq!(
                end.block,
                Some(HeldBlock::Text(block(CREATE))),
                "split {split}"
            );
        }
    }

    #[test]
    fn a_fence_after_shown_text_on_the_same_line_is_not_a_block() {
        let (shown, end) = reply(&["see ", "```mcao-actions\n{}\n```"]);
        assert_eq!(
            format!("{shown}{}", end.tail),
            "see ```mcao-actions\n{}\n```"
        );
        assert_eq!(end.block, None);
    }

    #[test]
    fn a_diverging_prefix_like_a_rust_fence_is_flushed() {
        let text = "```rust\nfn main() {}\n```\n";
        for split in 0..=text.len() {
            let (head, tail) = text.split_at(split);
            let (shown, end) = reply(&[head, tail]);
            assert_eq!(format!("{shown}{}", end.tail), text, "split {split}");
            assert_eq!(end.block, None, "split {split}");
        }
    }

    #[test]
    fn a_trailing_fence_prefix_is_flushed_at_finish() {
        let (shown, end) = reply(&["Hi\n```mca"]);
        assert_eq!(shown, "Hi\n");
        assert_eq!(end.tail, "```mca");
        assert_eq!(end.block, None);
    }

    #[test]
    fn a_prose_line_starting_with_the_fence_is_shown() {
        let text = "```mcao-actions is the format\nnext line\n";
        for split in 0..=text.len() {
            let (head, tail) = text.split_at(split);
            let (shown, end) = reply(&[head, tail]);
            assert_eq!(format!("{shown}{}", end.tail), text, "split {split}");
            assert_eq!(end.block, None, "split {split}");
        }
    }

    #[test]
    fn an_unclosed_fence_at_the_end_is_shown_as_tail() {
        let text = "Hi\n\n```mcao-actions\n{\"mcao_actions\":1";
        for split in 0..=text.len() {
            let (head, tail) = text.split_at(split);
            let (shown, end) = reply(&[head, tail]);
            assert_eq!(format!("{shown}{}", end.tail), text, "split {split}");
            assert_eq!(end.block, None, "split {split}");
        }
    }

    #[test]
    fn a_fence_line_padded_past_the_cap_is_shown_not_held() {
        let padding = " ".repeat(10 * 1024);
        let mut filter = ReplyFilter::default();
        assert_eq!(filter.push(ACTION_FENCE), "");
        let shown = filter.push(&padding);
        assert_eq!(shown, format!("{ACTION_FENCE}{padding}"));
        let end = filter.finish();
        assert_eq!(end.tail, "");
        assert_eq!(end.block, None);
    }

    #[test]
    fn a_closed_fence_still_holds() {
        let text = "```mcao-actions \n{}\n```";
        let (shown, end) = reply(&[text]);
        assert_eq!(shown, "");
        assert_eq!(end.tail, "");
        assert_eq!(end.block, Some(HeldBlock::Text(text.to_owned())));
    }

    #[test]
    fn an_indented_fence_is_shown() {
        let text = "  ```mcao-actions\n{}\n```\n";
        let (shown, end) = reply(&[text]);
        assert_eq!(format!("{shown}{}", end.tail), text);
        assert_eq!(end.block, None);
    }

    #[test]
    fn blank_lines_before_a_block_are_dropped_and_kept_otherwise() {
        let text = "a\n\n\nb\n\n```mcao-actions\n{}\n```";
        let (shown, end) = reply(&[text]);
        assert_eq!(shown, "a\n\n\nb\n");
        assert_eq!(end.tail, "");
        assert_eq!(
            end.block,
            Some(HeldBlock::Text("```mcao-actions\n{}\n```".to_owned()))
        );
    }

    #[test]
    fn a_codex_message_prefix_before_a_block_is_dropped() {
        let (shown, end) = reply(&["\n\n", "```mcao-actions\n{}\n```"]);
        assert_eq!(shown, "");
        assert_eq!(
            end.block,
            Some(HeldBlock::Text("```mcao-actions\n{}\n```".to_owned()))
        );
    }

    #[test]
    fn held_text_is_capped() {
        let mut filter = ReplyFilter::default();
        assert_eq!(filter.push(&format!("{ACTION_FENCE}\n")), "");
        let chunk = "x".repeat(1024);
        for _ in 0..(HELD_LIMIT / 1024 + 2) {
            assert_eq!(filter.push(&chunk), "");
        }
        assert_eq!(filter.finish().block, Some(HeldBlock::Overflow));
    }
}
