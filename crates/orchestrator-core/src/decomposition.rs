//! Structured coordinator proposal validation. Updated: 2026-11-08 09:04:35 +00:00 UTC.
//!
//! [`validate_proposal`] parses a JSON object with a top-level `objective` and
//! non-empty `tasks` array. Each task has a `title` and a `contract` containing
//! the fields of [`TaskContract`]. The contract's `id` is a proposal-local key.
//! Its `dependencies` list names other local keys. The validator replaces the
//! ID and dependency references before returning the plan.
//! Unknown fields are rejected in proposal, task, contract, and verification objects.
//!
//! Canonical IDs use a stable FNV-1a fingerprint of the top-level objective
//! and each task's one-based position. JSON object key order does not affect
//! the result. Task array order does.
//!
//! Validation is pure and performs no I/O. It does not start tasks, persist
//! plans, change policy, or grant approvals. The engine retains lifecycle and
//! authorization authority.

use crate::{
    contract::{self, ContractError, TaskContract},
    scheduler::{Scheduler, SchedulerError, SchedulerLimits, SchedulerSnapshot, TaskSnapshot},
    task::TaskState,
};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{collections::BTreeMap, fmt};

// TaskContract's derived deserializer ignores unknown fields. This allow-list
// keeps the proposal boundary strict while leaving field types to TaskContract.
const TASK_CONTRACT_FIELDS: &[&str] = &[
    "id",
    "project_id",
    "parent_id",
    "objective",
    "kind",
    "agent",
    "dependencies",
    "verification",
    "delivery",
    "autonomy",
    "max_retries",
    "max_repairs",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    objective: String,
    tasks: Vec<ProposedTask>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposedTask {
    title: String,
    #[serde(deserialize_with = "deserialize_contract")]
    contract: TaskContract,
}

fn reject_unknown_verification_fields<E>(value: &serde_json::Value) -> Result<(), E>
where
    E: de::Error,
{
    let Some(commands) = value
        .as_object()
        .and_then(|variants| variants.get("Commands"))
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(());
    };
    for key in commands.keys() {
        if key != "commands" {
            return Err(E::unknown_field(key, &["commands"]));
        }
    }
    Ok(())
}

fn deserialize_contract<'de, D>(deserializer: D) -> Result<TaskContract, D::Error>
where
    D: Deserializer<'de>,
{
    struct ContractVisitor;

    impl<'de> Visitor<'de> for ContractVisitor {
        type Value = TaskContract;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a task contract object")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut fields = serde_json::Map::new();
            while let Some(key) = map.next_key::<String>()? {
                if !TASK_CONTRACT_FIELDS.contains(&key.as_str()) {
                    return Err(de::Error::unknown_field(&key, TASK_CONTRACT_FIELDS));
                }
                if fields.contains_key(&key) {
                    return Err(de::Error::custom(format!("duplicate field `{key}`")));
                }
                let value = map.next_value::<serde_json::Value>()?;
                if key == "verification" {
                    reject_unknown_verification_fields::<M::Error>(&value)?;
                }
                fields.insert(key, value);
            }
            TaskContract::deserialize(serde_json::Value::Object(fields)).map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_map(ContractVisitor)
}

/// A validated decomposition whose contracts contain canonical IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPlan {
    objective: String,
    tasks: Vec<PlannedTask>,
}

impl ValidatedPlan {
    /// Return the objective that identifies this plan.
    pub fn objective(&self) -> &str {
        &self.objective
    }

    /// Return tasks in their proposal order.
    pub fn tasks(&self) -> &[PlannedTask] {
        &self.tasks
    }
}

/// One task in a validated plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTask {
    title: String,
    contract: TaskContract,
}

impl PlannedTask {
    /// Return the coordinator-proposed task title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Return the validated contract with canonical ID and dependencies.
    pub fn contract(&self) -> &TaskContract {
        &self.contract
    }
}

/// One task whose contract failed the core contract validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractFailure {
    /// Proposal-local task key from the incoming contract ID.
    pub key: String,
    /// First invariant that failed for this contract.
    pub error: ContractError,
}

/// A schema, dependency, or contract error in a coordinator proposal.
#[derive(Debug)]
pub enum DecompositionError {
    /// JSON did not match the strict proposal schema.
    Json(serde_json::Error),
    /// A proposal must contain at least one task.
    EmptyTasks,
    /// Two tasks use the same proposal-local key.
    DuplicateKey(String),
    /// A task refers to a key absent from the proposal.
    UnknownDependency {
        task_key: String,
        dependency_key: String,
    },
    /// Every listed task contract is returned, not only the first failure.
    InvalidContracts { failures: Vec<ContractFailure> },
    /// The dependency graph contains a closed cycle path of local keys.
    DependencyCycle { path: Vec<String> },
    /// An unexpected scheduler validation error.
    Scheduler(SchedulerError),
}

impl fmt::Display for DecompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid proposal JSON: {error}"),
            Self::EmptyTasks => formatter.write_str("proposal must contain at least one task"),
            Self::DuplicateKey(key) => write!(formatter, "duplicate proposal key {key:?}"),
            Self::UnknownDependency {
                task_key,
                dependency_key,
            } => write!(
                formatter,
                "task {task_key:?} references unknown dependency key {dependency_key:?}"
            ),
            Self::InvalidContracts { failures } => {
                formatter.write_str("invalid task contracts: ")?;
                for (index, failure) in failures.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str("; ")?;
                    }
                    match &failure.error {
                        ContractError::SelfDependency => write!(
                            formatter,
                            "task {:?}: dependency cycle: {} -> {}",
                            failure.key, failure.key, failure.key
                        )?,
                        error => write!(formatter, "task {:?}: {error}", failure.key)?,
                    }
                }
                Ok(())
            }
            Self::DependencyCycle { path } => {
                write!(formatter, "dependency cycle: {}", path.join(" -> "))
            }
            Self::Scheduler(error) => write!(formatter, "scheduler rejected plan: {error}"),
        }
    }
}

impl std::error::Error for DecompositionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Scheduler(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for DecompositionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Parse, validate, and canonicalize one coordinator proposal.
///
/// The proposal-local contract IDs identify dependency references only. The
/// returned contracts use deterministic IDs derived from the proposal
/// objective and task position.
pub fn validate_proposal(json: &str) -> Result<ValidatedPlan, DecompositionError> {
    let mut proposal: Proposal = serde_json::from_str(json)?;
    if proposal.tasks.is_empty() {
        return Err(DecompositionError::EmptyTasks);
    }

    let local_keys: Vec<_> = proposal
        .tasks
        .iter_mut()
        .map(|task| std::mem::take(&mut task.contract.id))
        .collect();
    let mut key_indices = BTreeMap::new();
    for (index, key) in local_keys.iter().enumerate() {
        if key_indices.insert(key.as_str(), index).is_some() {
            return Err(DecompositionError::DuplicateKey(key.clone()));
        }
    }

    let objective_hash = objective_fingerprint(&proposal.objective);
    let canonical_ids: Vec<_> = (0..proposal.tasks.len())
        .map(|position| format!("decomp-{objective_hash:016x}-{}", position + 1))
        .collect();
    for (index, task) in proposal.tasks.iter_mut().enumerate() {
        let dependencies = std::mem::take(&mut task.contract.dependencies);
        let mut canonical_dependencies = Vec::with_capacity(dependencies.len());
        for dependency_key in dependencies {
            let Some(&dependency_index) = key_indices.get(dependency_key.as_str()) else {
                return Err(DecompositionError::UnknownDependency {
                    task_key: local_keys[index].clone(),
                    dependency_key,
                });
            };
            canonical_dependencies.push(canonical_ids[dependency_index].clone());
        }
        task.contract.id = canonical_ids[index].clone();
        task.contract.dependencies = canonical_dependencies;
    }

    let failures: Vec<_> = proposal
        .tasks
        .iter()
        .zip(&local_keys)
        .filter_map(|(task, key)| {
            contract::validate(&task.contract)
                .err()
                .map(|error| ContractFailure {
                    key: key.clone(),
                    error,
                })
        })
        .collect();
    if !failures.is_empty() {
        return Err(DecompositionError::InvalidContracts { failures });
    }

    let snapshots: Vec<_> = proposal
        .tasks
        .iter()
        .map(|task| TaskSnapshot {
            contract: &task.contract,
            state: TaskState::Queued,
            workspace: None,
        })
        .collect();
    let canonical_to_index: BTreeMap<_, _> = canonical_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    if let Err(error) = Scheduler::new(
        SchedulerSnapshot {
            tasks: &snapshots,
            last_started_project: None,
        },
        SchedulerLimits::default(),
    ) {
        match error {
            SchedulerError::DependencyCycle { cycle } => {
                let path = cycle
                    .iter()
                    .map(|id| {
                        canonical_to_index
                            .get(id.as_str())
                            .map(|index| local_keys[*index].clone())
                    })
                    .collect::<Option<Vec<_>>>();
                return match path {
                    Some(path) => Err(DecompositionError::DependencyCycle { path }),
                    None => Err(DecompositionError::Scheduler(
                        SchedulerError::DependencyCycle { cycle },
                    )),
                };
            }
            error => return Err(DecompositionError::Scheduler(error)),
        }
    }

    Ok(ValidatedPlan {
        objective: proposal.objective,
        tasks: proposal
            .tasks
            .into_iter()
            .map(|task| PlannedTask {
                title: task.title,
                contract: task.contract,
            })
            .collect(),
    })
}

fn objective_fingerprint(objective: &str) -> u64 {
    objective.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
