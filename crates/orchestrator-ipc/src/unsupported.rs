use crate::{Capability, Health, IpcError, IpcEvent, RpcHandler};
use serde_json::Value;
use std::fmt;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(2);

/// Holds no bytes here: `generate` and `read_from` always fail.
pub struct ApproverSecret;

impl fmt::Debug for ApproverSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApproverSecret([REDACTED])")
    }
}

impl ApproverSecret {
    pub fn generate() -> Result<Self, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn to_wire(&self) -> String {
        String::new()
    }
    pub fn from_wire(_: &str) -> Option<Self> {
        None
    }
    pub fn read_from(_: &mut impl Read) -> Result<Self, IpcError> {
        Err(IpcError::Unsupported)
    }
}

pub struct Client;

impl Client {
    pub fn connect_with_timeout(
        _: impl AsRef<std::path::Path>,
        _: &[Capability],
        _: Duration,
    ) -> Result<Self, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn connect_with_approver(
        _: impl AsRef<std::path::Path>,
        _: &[Capability],
        _: Duration,
        _: Option<Arc<ApproverSecret>>,
    ) -> Result<Self, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn is_approver(&self) -> bool {
        false
    }
    pub fn connect(_: impl AsRef<std::path::Path>) -> Result<Self, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn connect_with_capabilities(
        _: impl AsRef<std::path::Path>,
        _: &[Capability],
    ) -> Result<Self, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn health(&mut self) -> Result<Health, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn capabilities(&mut self) -> Result<Vec<Capability>, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn shutdown(&mut self) -> Result<(), IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn projects_list(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn projects_register(&mut self, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn projects_get_defaults(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn projects_set_defaults(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn tasks_submit(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn tasks_list(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn tasks_get(&mut self, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn tasks_cancel(&mut self, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn sessions_list(&mut self, _: Option<&str>) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn sessions_get(&mut self, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn sessions_events(&mut self, _: &str, _: &str, _: Option<i64>) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn approvals_list(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn approvals_resolve(&mut self, _: &str, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn actions_pending(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn actions_resolve(
        &mut self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn workspaces_diff(
        &mut self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn delivery_request(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn coordinator_send_message(
        &mut self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn coordinator_propose(&mut self, _: &str, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn coordinator_get(&mut self, _: &str, _: Option<i64>) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn queue_snapshot(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn review_request_approval(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn host_metrics(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn settings_get(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn settings_set(&mut self, _: Value, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn monitoring_get(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn monitoring_set(&mut self, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn monitoring_ack(&mut self, _: Value, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn monitoring_brief(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn monitoring_brief_ack(&mut self, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn holds_set(&mut self, _: &str, _: &str, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn holds_release(&mut self, _: &str, _: &str) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn holds_list(&mut self) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn grants_create(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn grants_revoke(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn grants_list(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn pulls_list(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn pulls_refresh(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn memory_list(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn memory_set(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn memory_remove(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn memory_restore(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn reports_list(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn reports_get(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn reports_relay(&mut self, _: Value) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn review_evidence(
        &mut self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn review_request_changes(
        &mut self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Value, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn events_subscribe(&mut self, _: Option<i64>) -> Result<String, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn next_event(&mut self) -> Result<IpcEvent, IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn reconnect(&mut self) -> Result<(), IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn last_event_sequence(&self) -> Option<i64> {
        None
    }
}

pub struct Server;
pub struct ServerConfig;

impl ServerConfig {
    pub fn new(_: PathBuf, _: impl Into<String>) -> Self {
        Self
    }
    pub fn with_idle_timeout(self, _: Duration) -> Self {
        self
    }
    pub fn with_stream_timeout(self, _: Duration) -> Self {
        self
    }
    pub fn with_approver(self, _: ApproverSecret) -> Self {
        self
    }
}

impl Server {
    pub fn serve(_: ServerConfig) -> Result<(), IpcError> {
        Err(IpcError::Unsupported)
    }
    pub fn serve_with_handler(_: ServerConfig, _: Arc<dyn RpcHandler>) -> Result<(), IpcError> {
        Err(IpcError::Unsupported)
    }
}
