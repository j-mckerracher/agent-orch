use async_trait::async_trait;
use orchestrator_agents::{
    AgentError, AgentEvent, AgentExit, AgentHandle, ApprovalDecision, InteractiveAgentHandle,
    Result,
};
use std::collections::HashSet;
use uuid::Uuid;

struct FakeInteractiveHandle {
    pending_approvals: HashSet<Uuid>,
    exited: bool,
    turn_running: bool,
    decisions: Vec<(Uuid, ApprovalDecision)>,
    prompts: Vec<String>,
}

impl FakeInteractiveHandle {
    fn new(pending_approvals: impl IntoIterator<Item = Uuid>) -> Self {
        Self {
            pending_approvals: pending_approvals.into_iter().collect(),
            exited: false,
            turn_running: false,
            decisions: Vec::new(),
            prompts: Vec::new(),
        }
    }
}

#[async_trait]
impl AgentHandle for FakeInteractiveHandle {
    fn pid(&self) -> u32 {
        0
    }

    fn try_exited(&self) -> std::io::Result<bool> {
        Ok(self.exited)
    }

    async fn next_event(&mut self) -> Result<Option<AgentEvent>> {
        Ok(None)
    }

    async fn cancel(&mut self) -> Result<()> {
        self.exited = true;
        Ok(())
    }

    async fn wait(&mut self) -> Result<AgentExit> {
        self.exited = true;
        Ok(AgentExit::Completed {
            code: Some(0),
            last_message: None,
        })
    }
}

#[async_trait]
impl InteractiveAgentHandle for FakeInteractiveHandle {
    async fn send_turn(&mut self, prompt: &str) -> Result<()> {
        if self.exited {
            return Err(AgentError::Protocol("session already exited".into()));
        }
        if self.turn_running {
            return Err(AgentError::Protocol("turn already running".into()));
        }
        self.prompts.push(prompt.to_owned());
        Ok(())
    }

    async fn respond_approval(
        &mut self,
        approval_id: Uuid,
        decision: ApprovalDecision,
    ) -> Result<()> {
        if !self.pending_approvals.remove(&approval_id) {
            return Err(AgentError::Protocol(format!(
                "approval id {approval_id} not pending"
            )));
        }
        self.decisions.push((approval_id, decision));
        Ok(())
    }
}

#[test]
fn plan_proposed_clones_and_compares_by_fields() {
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    let plan = AgentEvent::PlanProposed {
        approval_id: id_a,
        plan: "1. Edit contract\n2. Run tests".into(),
    };

    assert_eq!(plan.clone(), plan);
    assert_ne!(
        plan,
        AgentEvent::PlanProposed {
            approval_id: id_a,
            plan: "different plan".into(),
        }
    );
    assert_ne!(
        plan,
        AgentEvent::PlanProposed {
            approval_id: id_b,
            plan: "1. Edit contract\n2. Run tests".into(),
        }
    );

    assert_eq!(ApprovalDecision::Approve.clone(), ApprovalDecision::Approve);
    let deny = ApprovalDecision::Deny {
        reason: "needs review".into(),
    };
    assert_eq!(deny.clone(), deny);
    assert_ne!(deny, ApprovalDecision::Approve);
    assert_ne!(
        deny,
        ApprovalDecision::Deny {
            reason: "other".into(),
        }
    );
}

#[tokio::test]
async fn respond_approval_answers_each_id_at_most_once() {
    let id = Uuid::new_v4();
    let unknown = Uuid::new_v4();
    let mut handle = FakeInteractiveHandle::new([id]);

    assert!(handle
        .respond_approval(id, ApprovalDecision::Approve)
        .await
        .is_ok());
    assert!(matches!(
        handle.respond_approval(id, ApprovalDecision::Approve).await,
        Err(AgentError::Protocol(_))
    ));
    assert!(matches!(
        handle
            .respond_approval(
                unknown,
                ApprovalDecision::Deny {
                    reason: "nope".into(),
                },
            )
            .await,
        Err(AgentError::Protocol(_))
    ));
    assert_eq!(handle.decisions, vec![(id, ApprovalDecision::Approve)]);

    assert!(handle.send_turn("follow up").await.is_ok());
    assert_eq!(handle.prompts, vec!["follow up".to_owned()]);

    handle.turn_running = true;
    assert!(matches!(
        handle.send_turn("overlap").await,
        Err(AgentError::Protocol(_))
    ));

    handle.turn_running = false;
    handle.exited = true;
    assert!(matches!(
        handle.send_turn("after exit").await,
        Err(AgentError::Protocol(_))
    ));
}
