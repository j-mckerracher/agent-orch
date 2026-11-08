use crate::protocol::{self, Capability, Handshake, RpcId, RpcRequest, RpcResponse, Welcome};
use crate::unix_transport::set_timeouts;
use crate::{Health, IpcError};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub struct Client {
    stream: UnixStream,
    negotiated: Vec<Capability>,
    next_id: u64,
}

impl Client {
    pub fn connect(data_dir: impl AsRef<Path>) -> Result<Self, IpcError> {
        let data_dir = data_dir.as_ref();
        let token = read_token(data_dir)?;
        let mut stream = UnixStream::connect(data_dir.join("ipc/daemon.sock"))
            .map_err(|_| IpcError::Operation)?;
        set_timeouts(&stream, Duration::from_secs(2)).map_err(|_| IpcError::Operation)?;
        let handshake = RpcRequest {
            jsonrpc: "2.0".into(),
            id: RpcId::Number(1u64.into()),
            method: "handshake".into(),
            params: serde_json::to_value(Handshake {
                token: Some(token),
                versions: vec![1],
                capabilities: vec!["health".into()],
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
        if welcome.version != 1 {
            return Err(IpcError::UnsupportedVersion);
        }
        Ok(Self {
            stream,
            negotiated: welcome.capabilities,
            next_id: 2,
        })
    }

    pub fn health(&mut self) -> Result<Health, IpcError> {
        let result = self.call("health", Value::Null)?;
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

    fn call(&mut self, method: &str, params: Value) -> Result<Value, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
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
        let response = protocol::read::<RpcResponse>(&mut self.stream)?;
        if response.jsonrpc != "2.0" || response.id != rpc_id {
            return Err(IpcError::Operation);
        }
        if let Some(error) = response.error {
            return Err(map_remote(error.code));
        }
        response.result.ok_or(IpcError::Operation)
    }
}

fn read_token(data_dir: &Path) -> Result<String, IpcError> {
    let path = data_dir.join("daemon.token");
    let before = fs::symlink_metadata(&path).map_err(|_| IpcError::Authentication)?;
    let owner = fs::metadata(data_dir)
        .map_err(|_| IpcError::Authentication)?
        .uid();
    if !before.is_file() || before.uid() != owner || before.mode() & 0o777 != 0o600 {
        return Err(IpcError::Authentication);
    }
    let mut file = File::open(&path).map_err(|_| IpcError::Authentication)?;
    let opened = file.metadata().map_err(|_| IpcError::Authentication)?;
    if (before.dev(), before.ino(), before.uid()) != (opened.dev(), opened.ino(), opened.uid()) {
        return Err(IpcError::Authentication);
    }
    let mut token = String::new();
    file.read_to_string(&mut token)
        .map_err(|_| IpcError::Authentication)?;
    if token.len() != 64 {
        return Err(IpcError::Authentication);
    }
    Ok(token)
}

fn map_remote(code: i32) -> IpcError {
    match code {
        -32001 => IpcError::Authentication,
        -32002 => IpcError::UnsupportedCapability,
        -32003 => IpcError::UnsupportedVersion,
        -32004 => IpcError::UnsupportedCapability,
        _ => IpcError::Operation,
    }
}
