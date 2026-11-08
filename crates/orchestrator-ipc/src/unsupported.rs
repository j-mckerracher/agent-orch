use crate::{Capability, Health, IpcError};
use std::path::PathBuf;
use std::time::Duration;

pub struct Client;

impl Client {
    pub fn connect(_: impl AsRef<std::path::Path>) -> Result<Self, IpcError> {
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
}

impl Server {
    pub fn serve(_: ServerConfig) -> Result<(), IpcError> {
        Err(IpcError::Unsupported)
    }
}
