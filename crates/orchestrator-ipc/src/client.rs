//! Synchronous JSON-RPC client with cursor tracking. A `call()` keeps any events
//! that arrive before its response (for example a replay in flight), so the
//! connection stays in sync; the daemon pages replay and bounds live lag.
//! Updated: 2026-11-08T18:09:45+00:00.
use crate::approver::ApproverSecret;
use crate::protocol::{
    self, Capability, Handshake, IncomingMessage, RpcId, RpcRequest, RpcResponse, Welcome,
};
use crate::transport::{connect as connect_local, IpcStream};
use crate::{Health, IpcError, IpcEvent};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// I/O timeout `Client::connect` uses for the handshake and each call.
pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Client {
    data_dir: PathBuf,
    requested: Vec<Capability>,
    stream: Box<dyn IpcStream>,
    negotiated: Vec<Capability>,
    next_id: u64,
    subscription: Option<String>,
    last_event_sequence: Option<i64>,
    journal_epoch: Option<String>,
    cursor_epoch: Option<String>,
    events: VecDeque<Result<IpcEvent, IpcError>>,
    io_timeout: Duration,
    /// Presented in every handshake, `reconnect()` included, so a reconnect
    /// keeps the role. Never formatted or logged.
    approver: Option<Arc<ApproverSecret>>,
    /// What the welcome said: true only when the daemon accepted the proof.
    approver_granted: bool,
}

impl Client {
    pub fn connect(data_dir: impl AsRef<Path>) -> Result<Self, IpcError> {
        Self::connect_with_capabilities(data_dir, &Capability::ALL)
    }

    pub fn connect_with_capabilities(
        data_dir: impl AsRef<Path>,
        requested: &[Capability],
    ) -> Result<Self, IpcError> {
        Self::connect_with_timeout(data_dir, requested, DEFAULT_IO_TIMEOUT)
    }

    /// Like `connect_with_capabilities`, with a per-read and per-write I/O
    /// timeout for the handshake and every later call. A call that spans a slow
    /// daemon operation (for example Git work on a slow host) needs a larger
    /// bound than `DEFAULT_IO_TIMEOUT`; `reconnect()` keeps the chosen value.
    pub fn connect_with_timeout(
        data_dir: impl AsRef<Path>,
        requested: &[Capability],
        io_timeout: Duration,
    ) -> Result<Self, IpcError> {
        Self::connect_with_approver(data_dir, requested, io_timeout, None)
    }

    /// Like `connect_with_timeout`, presenting the approver secret in the
    /// handshake. A daemon that does not accept it still connects; the session
    /// is then not an approver (see `is_approver`). `reconnect()` presents it
    /// again.
    pub fn connect_with_approver(
        data_dir: impl AsRef<Path>,
        requested: &[Capability],
        io_timeout: Duration,
        approver: Option<Arc<ApproverSecret>>,
    ) -> Result<Self, IpcError> {
        let data_dir = data_dir.as_ref();
        let token = read_token(data_dir)?;
        let mut stream = connect_local(data_dir).map_err(|_| IpcError::Operation)?;
        stream
            .set_timeouts(io_timeout)
            .map_err(|_| IpcError::Operation)?;
        let handshake = RpcRequest {
            jsonrpc: "2.0".into(),
            id: RpcId::Number(1u64.into()),
            method: "handshake".into(),
            params: serde_json::to_value(Handshake {
                token: Some(token),
                versions: vec![1],
                capabilities: requested
                    .iter()
                    .map(|capability| capability.as_str().to_owned())
                    .collect(),
                approver: approver.as_deref().map(ApproverSecret::to_wire),
            })
            .map_err(|_| IpcError::Operation)?,
        };
        protocol::write(&mut stream, &handshake)?;
        let response = protocol::read::<RpcResponse>(&mut stream)?;
        if response.jsonrpc != "2.0" || response.id != RpcId::Number(1u64.into()) {
            return Err(IpcError::Operation);
        }
        if let Some(error) = response.error {
            return Err(map_remote(error.code));
        }
        let welcome: Welcome = serde_json::from_value(response.result.ok_or(IpcError::Operation)?)
            .map_err(|_| IpcError::Operation)?;
        if welcome.version != 1
            || welcome
                .capabilities
                .iter()
                .any(|cap| !requested.contains(cap))
        {
            return Err(IpcError::UnsupportedVersion);
        }
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            requested: requested.to_vec(),
            stream,
            negotiated: welcome.capabilities,
            next_id: 2,
            subscription: None,
            last_event_sequence: None,
            journal_epoch: welcome.journal_epoch,
            cursor_epoch: None,
            events: VecDeque::new(),
            io_timeout,
            approver,
            approver_granted: welcome.approver,
        })
    }

    /// True when the daemon accepted this session's approver secret, so it may
    /// call the user-only methods (`actions.resolve`, `approvals.resolve`).
    pub fn is_approver(&self) -> bool {
        self.approver_granted
    }

    pub fn health(&mut self) -> Result<Health, IpcError> {
        let result = self.call_feature(Capability::Health, "health", Value::Null)?;
        serde_json::from_value(result).map_err(|_| IpcError::Operation)
    }

    pub fn capabilities(&mut self) -> Result<Vec<Capability>, IpcError> {
        let result = self.call("capabilities", Value::Null)?;
        let value = result
            .get("capabilities")
            .cloned()
            .ok_or(IpcError::Operation)?;
        let reported: Vec<Capability> =
            serde_json::from_value(value).map_err(|_| IpcError::Operation)?;
        if reported != self.negotiated {
            return Err(IpcError::Operation);
        }
        Ok(reported)
    }

    pub fn shutdown(&mut self) -> Result<(), IpcError> {
        self.call("shutdown", Value::Null).map(|_| ())
    }

    pub fn projects_list(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Projects, "projects.list", Value::Null)
    }

    /// Registers the Git repository at an absolute path as a project.
    pub fn projects_register(&mut self, path: &str) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Projects,
            "projects.register",
            json!({"path": path}),
        )
    }

    /// `projects.get_defaults`: the defaults of one project (open to every session).
    pub fn projects_get_defaults(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Projects, "projects.get_defaults", params)
    }

    /// `projects.set_defaults`: changes one project's defaults. The daemon refuses a
    /// session that did not prove the approver secret.
    pub fn projects_set_defaults(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Projects, "projects.set_defaults", params)
    }

    pub fn tasks_submit(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Tasks, "tasks.submit", params)
    }

    pub fn tasks_list(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Tasks, "tasks.list", Value::Null)
    }

    pub fn tasks_get(&mut self, task_id: &str) -> Result<Value, IpcError> {
        self.call_feature(Capability::Tasks, "tasks.get", json!({"task_id": task_id}))
    }

    pub fn tasks_cancel(
        &mut self,
        task_id: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Tasks,
            "tasks.cancel",
            json!({"task_id": task_id, "idempotency_key": idempotency_key}),
        )
    }

    pub fn sessions_list(&mut self, task_id: Option<&str>) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Sessions,
            "sessions.list",
            json!({"task_id": task_id}),
        )
    }

    pub fn sessions_get(&mut self, task_id: &str, session_id: &str) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Sessions,
            "sessions.get",
            json!({"task_id": task_id, "session_id": session_id}),
        )
    }

    pub fn sessions_events(
        &mut self,
        task_id: &str,
        session_id: &str,
        after_sequence: Option<i64>,
    ) -> Result<Value, IpcError> {
        self.sessions_events_in_epoch(task_id, session_id, after_sequence, None)
    }

    /// Like `sessions_events`, but fails with `IpcError::EpochMismatch` when
    /// the cursor was issued under another journal than the daemon's. Pass the
    /// `epoch` field of an earlier result to resume a page safely.
    pub fn sessions_events_in_epoch(
        &mut self,
        task_id: &str,
        session_id: &str,
        after_sequence: Option<i64>,
        epoch: Option<&str>,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Sessions,
            "sessions.events",
            json!({
                "task_id": task_id,
                "session_id": session_id,
                "after_sequence": after_sequence,
                "epoch": epoch
            }),
        )
    }

    pub fn approvals_list(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Approvals, "approvals.list", Value::Null)
    }

    pub fn approvals_resolve(
        &mut self,
        approval_id: &str,
        decision: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Approvals,
            "approvals.resolve",
            json!({
                "approval_id": approval_id,
                "decision": decision,
                "idempotency_key": idempotency_key
            }),
        )
    }

    pub fn tasks_retry(&mut self, task_id: &str, idempotency_key: &str) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Tasks,
            "tasks.retry",
            json!({"task_id": task_id, "idempotency_key": idempotency_key}),
        )
    }

    pub fn agents_capabilities(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Agents, "agents.capabilities", Value::Null)
    }

    /// Read-only scheduler capacity and queue view; see `docs/queue-snapshot.md`.
    pub fn queue_snapshot(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Queue, "queue.snapshot", Value::Null)
    }

    pub fn agents_catalog(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Agents, "agents.catalog", Value::Null)
    }

    /// Explicit GitHub Copilot sign-in check; see `agents.check_copilot_sign_in`.
    pub fn agents_check_copilot_sign_in(&mut self, force: bool) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Agents,
            "agents.check_copilot_sign_in",
            json!({"force": force}),
        )
    }

    pub fn agents_validate_selection(
        &mut self,
        agent: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Agents,
            "agents.validate_selection",
            json!({"agent": agent, "model": model, "effort": effort}),
        )
    }

    /// Lists the pending proposals of every conversation; see `actions.pending`.
    pub fn actions_pending(&mut self) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::OrchestratorActions,
            "actions.pending",
            Value::Null,
        )
    }

    /// Approves or rejects one proposed action; see `actions.resolve`.
    pub fn actions_resolve(
        &mut self,
        proposal_id: &str,
        decision: &str,
        digest: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::OrchestratorActions,
            "actions.resolve",
            json!({
                "proposal_id": proposal_id,
                "decision": decision,
                "digest": digest,
                "idempotency_key": idempotency_key
            }),
        )
    }

    pub fn workspaces_diff(
        &mut self,
        task_id: &str,
        workspace_id: Option<&str>,
        path: Option<&str>,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Workspaces,
            "workspaces.diff",
            json!({"task_id": task_id, "workspace_id": workspace_id, "path": path}),
        )
    }

    pub fn delivery_request(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Delivery, "delivery.request", params)
    }

    pub fn coordinator_send_message(
        &mut self,
        project_id: &str,
        text: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Coordinator,
            "coordinator.send_message",
            json!({"project_id": project_id, "text": text, "idempotency_key": idempotency_key}),
        )
    }

    pub fn coordinator_propose(
        &mut self,
        project_id: &str,
        proposal: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Coordinator,
            "coordinator.propose",
            json!({
                "project_id": project_id,
                "proposal": proposal,
                "idempotency_key": idempotency_key
            }),
        )
    }

    pub fn coordinator_get(
        &mut self,
        project_id: &str,
        after_sequence: Option<i64>,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Coordinator,
            "coordinator.get",
            json!({"project_id": project_id, "after_sequence": after_sequence}),
        )
    }

    /// `reports.list`: the newest scout reports, without their text.
    pub fn reports_list(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Coordinator, "reports.list", params)
    }

    /// `reports.get`: one scout report with its text.
    pub fn reports_get(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Coordinator, "reports.get", params)
    }

    /// `reports.relay`: asks the orchestrator to read one scout report in a new Restricted
    /// turn of the global conversation.
    pub fn reports_relay(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Coordinator, "reports.relay", params)
    }

    pub fn review_request_approval(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::Review, "review.request_approval", params)
    }

    /// `host.metrics`: cached CPU, memory, disk and GPU meters. See
    /// `docs/host-metrics.md`.
    pub fn host_metrics(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Host, "host.metrics", Value::Null)
    }

    /// `settings.get`: the machine load limits and the live state of the load gate.
    pub fn settings_get(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Settings, "settings.get", json!({}))
    }

    /// `settings.set`: changes some load limits (`load` holds `cpu_percent`,
    /// `memory_percent` and `disk_percent` as wanted). The daemon refuses a session
    /// that did not prove the approver secret.
    pub fn settings_set(&mut self, load: Value, idempotency_key: &str) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Settings,
            "settings.set",
            json!({"idempotency_key": idempotency_key, "load": load}),
        )
    }

    /// `monitoring.get`: the wake-up mode, the paused scopes and the unseen counts.
    pub fn monitoring_get(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Monitoring, "monitoring.get", json!({}))
    }

    /// `monitoring.set`: changes the wake-up mode only (`on`, `quiet` or `off`). The
    /// daemon refuses a session that did not prove the approver secret.
    pub fn monitoring_set(&mut self, mode: &str, idempotency_key: &str) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Monitoring,
            "monitoring.set",
            json!({"idempotency_key": idempotency_key, "mode": mode}),
        )
    }

    /// `monitoring.ack`: marks the records of one scope seen. `target` is the
    /// `conversations.get` target shape.
    pub fn monitoring_ack(
        &mut self,
        target: Value,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Monitoring,
            "monitoring.ack",
            json!({"idempotency_key": idempotency_key, "target": target}),
        )
    }

    /// `holds.set`: places a hold on one subject until the given day. The daemon
    /// refuses a session that did not prove the approver secret.
    pub fn holds_set(
        &mut self,
        subject_kind: &str,
        subject_id: &str,
        until: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Holds,
            "holds.set",
            json!({
                "subject_kind": subject_kind,
                "subject_id": subject_id,
                "until": until,
                "idempotency_key": idempotency_key
            }),
        )
    }

    /// `holds.release`: removes one hold. The daemon refuses a session that did not
    /// prove the approver secret.
    pub fn holds_release(
        &mut self,
        hold_id: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Holds,
            "holds.release",
            json!({"hold_id": hold_id, "idempotency_key": idempotency_key}),
        )
    }

    /// `monitoring.brief`: the return brief for the away window, or `null` when none is due.
    pub fn monitoring_brief(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::StandingApprovals, "monitoring.brief", json!({}))
    }

    /// `monitoring.brief_ack`: marks the closed brief seen. Idempotent.
    pub fn monitoring_brief_ack(&mut self, idempotency_key: &str) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::StandingApprovals,
            "monitoring.brief_ack",
            json!({"idempotency_key": idempotency_key}),
        )
    }

    /// `grants.create`: creates one standing grant from `params` (scope, kinds, expiry, use
    /// limit, token budget and an idempotency key). The daemon refuses a session that did
    /// not prove the approver secret.
    pub fn grants_create(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::StandingApprovals, "grants.create", params)
    }

    /// `grants.revoke`: revokes one standing grant, taking `grant_id` and an idempotency key
    /// from `params`. The daemon refuses a session that did not prove the approver secret.
    pub fn grants_revoke(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::StandingApprovals, "grants.revoke", params)
    }

    /// `grants.list`: the standing grants and the policy limits.
    pub fn grants_list(&mut self, params: Value) -> Result<Value, IpcError> {
        self.call_feature(Capability::StandingApprovals, "grants.list", params)
    }

    /// `holds.list`: the holds in place.
    pub fn holds_list(&mut self) -> Result<Value, IpcError> {
        self.call_feature(Capability::Holds, "holds.list", json!({}))
    }

    pub fn review_evidence(
        &mut self,
        task_id: &str,
        approval_id: Option<&str>,
        path: Option<&str>,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Review,
            "review.evidence",
            json!({"task_id": task_id, "approval_id": approval_id, "path": path}),
        )
    }

    pub fn review_request_changes(
        &mut self,
        approval_id: &str,
        candidate_commit: &str,
        feedback: &str,
        idempotency_key: &str,
    ) -> Result<Value, IpcError> {
        self.call_feature(
            Capability::Review,
            "review.request_changes",
            json!({
                "approval_id": approval_id,
                "candidate_commit": candidate_commit,
                "feedback": feedback,
                "idempotency_key": idempotency_key
            }),
        )
    }

    /// Calls one `workstreams.*` method. The daemon authorizes by method name,
    /// so a non-workstream method is refused here rather than gated wrongly.
    pub fn workstreams_call(&mut self, method: &str, params: Value) -> Result<Value, IpcError> {
        if !method.starts_with("workstreams.") {
            return Err(IpcError::Operation);
        }
        self.call_feature(Capability::Workstreams, method, params)
    }

    pub fn events_subscribe(&mut self, after_sequence: Option<i64>) -> Result<String, IpcError> {
        self.events_subscribe_in_epoch(after_sequence, None)
    }

    /// Subscribes after a cursor that was issued under `epoch` (see
    /// `cursor_epoch`). A daemon whose journal has another epoch refuses with
    /// `IpcError::EpochMismatch` rather than replaying unrelated events.
    pub fn events_subscribe_in_epoch(
        &mut self,
        after_sequence: Option<i64>,
        epoch: Option<&str>,
    ) -> Result<String, IpcError> {
        if self.subscription.is_some() {
            return Err(IpcError::Conflict);
        }
        if after_sequence.is_some_and(|sequence| sequence < 0) {
            return Err(IpcError::InvalidParams);
        }
        let mut params = json!({});
        if let Some(sequence) = after_sequence {
            params["after_sequence"] = json!(sequence);
        }
        if let Some(epoch) = epoch {
            params["epoch"] = json!(epoch);
        }
        let result = self.call_feature(Capability::Events, "events.subscribe", params)?;
        let subscription = result
            .get("subscription_id")
            .and_then(Value::as_str)
            .ok_or(IpcError::Operation)?
            .to_owned();
        self.subscription = Some(subscription.clone());
        self.last_event_sequence = Some(after_sequence.unwrap_or(0));
        self.cursor_epoch = result
            .get("epoch")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(subscription)
    }

    pub fn next_event(&mut self) -> Result<IpcEvent, IpcError> {
        if let Some(event) = self.events.pop_front() {
            return self.accept_event(event?);
        }
        if self.subscription.is_none() {
            return Err(IpcError::NoSubscription);
        }
        self.stream
            .set_timeouts(Duration::from_secs(24 * 60 * 60))
            .map_err(|_| IpcError::Operation)?;
        let result = self.read_event();
        let reset = self.stream.set_timeouts(self.io_timeout);
        if reset.is_err() {
            return Err(IpcError::Operation);
        }
        self.accept_event(result?)
    }

    /// Replaces the connection with a freshly authenticated one and resumes the
    /// event stream after the last sequence this client accepted. Use it after
    /// a transport error, a typed stream error, or a daemon restart; unread
    /// buffered events are dropped and replayed by the daemon.
    ///
    /// The cursor is kept until a resume succeeds, so calling `reconnect()`
    /// again after any failure retries the same resume and never returns `Ok`
    /// without a subscription. `IpcError::ReplayGap` is terminal for that
    /// cursor: the daemon journal is behind it. `IpcError::EpochMismatch` is
    /// terminal for it too: the journal was replaced, so the cursor names
    /// events of another journal. The cursor and its epoch stay recorded, so a
    /// repeated `reconnect()` fails the same way instead of resuming silently.
    /// For either error, refresh state with the list methods, then call
    /// `events_subscribe(None)` to start a new stream on the current journal.
    pub fn reconnect(&mut self) -> Result<(), IpcError> {
        let cursor = self.last_event_sequence;
        let cursor_epoch = self.cursor_epoch.clone();
        let mut fresh = Self::connect_with_approver(
            &self.data_dir,
            &self.requested,
            self.io_timeout,
            self.approver.clone(),
        )?;
        let resumed = match cursor {
            Some(cursor) => fresh
                .events_subscribe_in_epoch(Some(cursor), cursor_epoch.as_deref())
                .map(|_| ()),
            None => Ok(()),
        };
        fresh.last_event_sequence = cursor;
        fresh.cursor_epoch = cursor_epoch;
        *self = fresh;
        resumed
    }

    /// Journal epoch announced by the daemon this connection reached, when the
    /// daemon provides one.
    pub fn journal_epoch(&self) -> Option<&str> {
        self.journal_epoch.as_deref()
    }

    /// Journal epoch the current event cursor was issued under.
    pub fn cursor_epoch(&self) -> Option<&str> {
        self.cursor_epoch.as_deref()
    }

    pub fn last_event_sequence(&self) -> Option<i64> {
        self.last_event_sequence
    }

    fn call_feature(
        &mut self,
        capability: Capability,
        method: &str,
        params: Value,
    ) -> Result<Value, IpcError> {
        if !self.negotiated.contains(&capability) {
            return Err(IpcError::UnsupportedCapability);
        }
        self.call(method, params)
    }

    pub(crate) fn call(&mut self, method: &str, params: Value) -> Result<Value, IpcError> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(IpcError::Operation)?;
        let rpc_id = RpcId::Number(id.into());
        protocol::write(
            &mut self.stream,
            &RpcRequest {
                jsonrpc: "2.0".into(),
                id: rpc_id.clone(),
                method: method.into(),
                params,
            },
        )?;
        loop {
            match protocol::read::<IncomingMessage>(&mut self.stream)? {
                IncomingMessage::Notification(notification) => {
                    self.queue_notification(notification)?;
                }
                IncomingMessage::Response(response) => {
                    if response.jsonrpc != "2.0" || response.id != rpc_id {
                        return Err(IpcError::Operation);
                    }
                    if let Some(error) = response.error {
                        return Err(map_remote(error.code));
                    }
                    return response.result.ok_or(IpcError::Operation);
                }
            }
        }
    }

    fn read_event(&mut self) -> Result<IpcEvent, IpcError> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return event;
            }
            match protocol::read::<IncomingMessage>(&mut self.stream)? {
                IncomingMessage::Notification(notification) => {
                    self.queue_notification(notification)?;
                }
                IncomingMessage::Response(_) => return Err(IpcError::Operation),
            }
        }
    }

    fn queue_notification(
        &mut self,
        notification: crate::protocol::RpcNotification,
    ) -> Result<(), IpcError> {
        if notification.jsonrpc != "2.0" {
            return Err(IpcError::Operation);
        }
        let subscription = self
            .subscription
            .as_deref()
            .ok_or(IpcError::NoSubscription)?;
        match notification.method.as_str() {
            "events.event" => {
                let params: EventParams =
                    serde_json::from_value(notification.params).map_err(|_| IpcError::Operation)?;
                if params.subscription_id != subscription {
                    return Err(IpcError::Operation);
                }
                self.events.push_back(Ok(params.event));
            }
            "events.error" => {
                let params: EventErrorParams =
                    serde_json::from_value(notification.params).map_err(|_| IpcError::Operation)?;
                if params.subscription_id != subscription {
                    return Err(IpcError::Operation);
                }
                self.subscription = None;
                self.events.push_back(Err(map_remote(params.error.code)));
            }
            _ => return Err(IpcError::Operation),
        }
        Ok(())
    }

    fn accept_event(&mut self, event: IpcEvent) -> Result<IpcEvent, IpcError> {
        let expected = self
            .last_event_sequence
            .and_then(|sequence| sequence.checked_add(1))
            .ok_or(IpcError::Operation)?;
        if event.sequence != expected {
            return Err(IpcError::Operation);
        }
        self.last_event_sequence = Some(event.sequence);
        Ok(event)
    }
}

#[derive(serde::Deserialize)]
struct EventParams {
    subscription_id: String,
    event: IpcEvent,
}

#[derive(serde::Deserialize)]
struct EventErrorParams {
    subscription_id: String,
    error: RemoteEventError,
}

#[derive(serde::Deserialize)]
struct RemoteEventError {
    code: i32,
    #[serde(rename = "message")]
    _message: String,
}

#[cfg(any(unix, windows))]
pub(crate) fn read_token(data_dir: &Path) -> Result<String, IpcError> {
    let path = data_dir.join("daemon.token");
    let before = fs::symlink_metadata(&path).map_err(|_| IpcError::Authentication)?;
    #[cfg(unix)]
    {
        let owner = fs::metadata(data_dir)
            .map_err(|_| IpcError::Authentication)?
            .uid();
        if !before.is_file() || before.uid() != owner || before.mode() & 0o777 != 0o600 {
            return Err(IpcError::Authentication);
        }
    }
    #[cfg(windows)]
    if !crate::auth::is_regular_file(&before) {
        return Err(IpcError::Authentication);
    }
    let mut file = File::open(&path).map_err(|_| IpcError::Authentication)?;
    #[cfg(unix)]
    {
        let opened = file.metadata().map_err(|_| IpcError::Authentication)?;
        if (before.dev(), before.ino(), before.uid()) != (opened.dev(), opened.ino(), opened.uid())
        {
            return Err(IpcError::Authentication);
        }
    }
    #[cfg(windows)]
    {
        use crate::auth::{file_id_from_file, file_id_from_path};
        let opened = file.metadata().map_err(|_| IpcError::Authentication)?;
        let expected = file_id_from_path(&path, &before);
        if expected.is_none() || file_id_from_file(&file, &opened) != expected {
            return Err(IpcError::Authentication);
        }
    }
    read_token_contents(&mut file)
}

fn read_token_contents(file: &mut File) -> Result<String, IpcError> {
    let mut token = String::new();
    file.read_to_string(&mut token)
        .map_err(|_| IpcError::Authentication)?;
    if token.len() != 64
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(IpcError::Authentication);
    }
    Ok(token)
}

pub(crate) fn map_remote(code: i32) -> IpcError {
    match code {
        -32001 => IpcError::Authentication,
        -32002 | -32004 => IpcError::UnsupportedCapability,
        -32003 => IpcError::UnsupportedVersion,
        -32602 => IpcError::InvalidParams,
        -32010 => IpcError::NotFound,
        -32020 => IpcError::ParentNotFound,
        -32011 => IpcError::Conflict,
        -32012 => IpcError::Forbidden,
        -32013 => IpcError::IdempotencyConflict,
        -32014 => IpcError::IdempotencyInProgress,
        -32015 => IpcError::SlowConsumer,
        -32016 => IpcError::ResultTooLarge,
        -32017 => IpcError::ReplayGap,
        -32018 => IpcError::EpochMismatch,
        -32019 => IpcError::ConversationBusy,
        -32021 => IpcError::ProposalNotFound,
        -32022 => IpcError::ProposalNotPending,
        -32023 => IpcError::ProposalInvalid,
        -32024 => IpcError::GrantNotFound,
        -32025 => IpcError::GrantInvalid,
        _ => IpcError::Operation,
    }
}
