//! The task record as the desktop reads it (DPES-1126). The stored record keeps its shape. The
//! daemon adds two facts it derives from the record: the permission level a task runs at, named
//! explicitly so Restricted is never left implicit, and a notice when the worker lacked a tool the
//! task's work needs.

use orchestrator_core::brief::ShellAccess;
use orchestrator_core::contract::TaskKind;
use orchestrator_store::TaskRecord;
use serde_json::{json, Value};

use crate::executor::worker_environment::shell_access;

/// The task record as JSON, with `permission_level` and `tools_notice` beside the stored fields.
pub(crate) fn task_view(record: &TaskRecord) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(record)?;
    if let Value::Object(fields) = &mut value {
        fields.insert(
            "permission_level".to_owned(),
            json!(record.contract.permission.as_str()),
        );
        fields.insert(
            "tools_notice".to_owned(),
            tools_notice(record).unwrap_or(Value::Null),
        );
    }
    Ok(value)
}

/// The notice an implementation task gets when its level gave the worker no shell that can run
/// the project's tests. Read-only work needs no such shell and gets no notice. The text is the
/// daemon's own: nothing the agent wrote reaches it.
fn tools_notice(record: &TaskRecord) -> Option<Value> {
    let contract = &record.contract;
    if contract.kind != TaskKind::Implement {
        return None;
    }
    let (tools, reason) = match shell_access(&contract.agent, contract.permission) {
        ShellAccess::None => (
            vec!["shell"],
            "This task ran at restricted permission, which gives the worker no shell. It could \
             not run the project's tests.",
        ),
        ShellAccess::ReadOnlyGit => (
            vec!["shell beyond read-only git"],
            "This task ran at extended permission, where the worker's shell runs read-only git \
             only. It could not run the project's tests.",
        ),
        ShellAccess::Checked | ShellAccess::Unchecked => return None,
    };
    Some(json!({"kind": "lacked_tools", "tools": tools, "reason": reason}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::contract::{AutonomyPolicy, DeliveryPolicy, VerificationPolicy};
    use orchestrator_core::permission::PermissionLevel;
    use orchestrator_core::task::TaskState;

    fn record(kind: TaskKind, agent: &str, permission: PermissionLevel) -> TaskRecord {
        TaskRecord {
            contract: orchestrator_core::contract::TaskContract {
                id: "t-1".into(),
                project_id: "/repo".into(),
                parent_id: None,
                objective: "Add a parser test.".into(),
                kind,
                agent: agent.into(),
                dependencies: Vec::new(),
                verification: VerificationPolicy::None,
                delivery: DeliveryPolicy::LocalReview,
                autonomy: AutonomyPolicy::WorkspaceWrites,
                max_retries: 0,
                max_repairs: 0,
                model_selection: None,
                permission,
                work_item: None,
            },
            state: TaskState::Created,
            attempts: Vec::new(),
            sessions: Vec::new(),
            events: Vec::new(),
            version: 0,
        }
    }

    #[test]
    fn every_task_names_its_level_and_a_restricted_one_is_not_implicit() {
        let view = task_view(&record(
            TaskKind::Investigate,
            "claude-code",
            PermissionLevel::Restricted,
        ))
        .unwrap();
        assert_eq!(view["permission_level"], "restricted");
        assert_eq!(view["tools_notice"], Value::Null);
        // The stored contract still omits a Restricted level, as before.
        assert!(view["contract"].get("permission").is_none());
    }

    #[test]
    fn an_implement_task_without_a_shell_gets_a_lacked_tools_notice() {
        let view = task_view(&record(
            TaskKind::Implement,
            "claude-code",
            PermissionLevel::Restricted,
        ))
        .unwrap();
        assert_eq!(view["tools_notice"]["kind"], "lacked_tools");
        assert_eq!(view["tools_notice"]["tools"], json!(["shell"]));
        assert!(view["tools_notice"]["reason"]
            .as_str()
            .unwrap()
            .contains("restricted permission"));
    }

    #[test]
    fn an_implement_task_at_extended_says_its_shell_reads_git_only() {
        let view = task_view(&record(
            TaskKind::Implement,
            "github-copilot",
            PermissionLevel::Extended,
        ))
        .unwrap();
        assert_eq!(view["permission_level"], "extended");
        assert_eq!(
            view["tools_notice"]["tools"],
            json!(["shell beyond read-only git"])
        );
    }

    #[test]
    fn an_implement_task_with_a_checked_shell_gets_no_notice() {
        let view = task_view(&record(
            TaskKind::Implement,
            "claude-code",
            PermissionLevel::Auto,
        ))
        .unwrap();
        assert_eq!(view["permission_level"], "auto");
        assert_eq!(view["tools_notice"], Value::Null);
    }
}
