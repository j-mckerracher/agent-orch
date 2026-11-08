use crate::auth::Secret;
use crate::session::{self, State};
use crate::transport::{Listener, TransportError};
use crate::{IpcError, RpcHandler};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct Server;

pub struct ServerConfig {
    data_dir: PathBuf,
    package_version: String,
    idle_timeout: Duration,
    stream_timeout: Duration,
}

impl ServerConfig {
    pub fn new(data_dir: PathBuf, package_version: impl Into<String>) -> Self {
        Self {
            data_dir,
            package_version: package_version.into(),
            idle_timeout: Duration::from_secs(60),
            stream_timeout: Duration::from_secs(2),
        }
    }

    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    pub fn with_stream_timeout(mut self, timeout: Duration) -> Self {
        self.stream_timeout = timeout;
        self
    }
}

impl Server {
    pub fn serve(config: ServerConfig) -> Result<(), IpcError> {
        Self::serve_inner(config, None)
    }

    /// Serves negotiated API methods through an authenticated local socket.
    /// Updated: 2026-11-08T17:06:42+00:00.
    pub fn serve_with_handler(
        config: ServerConfig,
        handler: Arc<dyn RpcHandler>,
    ) -> Result<(), IpcError> {
        Self::serve_inner(config, Some(handler))
    }

    fn serve_inner(
        config: ServerConfig,
        handler: Option<Arc<dyn RpcHandler>>,
    ) -> Result<(), IpcError> {
        Listener::prepare(&config.data_dir).map_err(map_transport)?;
        let secret = Arc::new(Secret::create(&config.data_dir).map_err(|_| IpcError::Operation)?);
        let mut listener = Listener::bind(&config.data_dir).map_err(map_transport)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| IpcError::Operation)?;
        let state = Arc::new(State::new(
            secret.clone(),
            config.data_dir,
            config.package_version,
            config.idle_timeout,
            config.stream_timeout,
            handler,
        ));
        let mut workers: Vec<JoinHandle<()>> = Vec::new();
        let mut failure = None;
        while !state.stopping.load(Ordering::Acquire) {
            reap_finished(&mut workers);
            let idle = state
                .last_request
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .elapsed()
                >= state.idle_timeout;
            if idle {
                if state
                    .handler
                    .as_ref()
                    .is_some_and(|handler| handler.keep_alive())
                {
                    *state
                        .last_request
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Instant::now();
                } else {
                    break;
                }
            }
            match listener.accept() {
                Ok(stream) => {
                    let state = state.clone();
                    if let Ok(worker) =
                        thread::Builder::new().spawn(move || session::serve(stream, state))
                    {
                        workers.push(worker);
                    }
                }
                Err(TransportError::Io(std::io::ErrorKind::WouldBlock)) => {
                    thread::sleep(Duration::from_millis(25))
                }
                Err(error) => {
                    failure = Some(map_transport(error));
                    break;
                }
            }
        }

        state.stopping.store(true, Ordering::Release);
        secret.remove_owned();
        drop(listener);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !workers.is_empty() && Instant::now() < deadline {
            reap_finished(&mut workers);
            if !workers.is_empty() {
                thread::sleep(Duration::from_millis(10));
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn reap_finished(workers: &mut Vec<JoinHandle<()>>) {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            let worker = workers.swap_remove(index);
            let _ = worker.join();
        } else {
            index += 1;
        }
    }
}

fn map_transport(error: TransportError) -> IpcError {
    match error {
        TransportError::AlreadyRunning => IpcError::AlreadyRunning,
        TransportError::UnsafePath => IpcError::UnsafePath,
        TransportError::Io(_) => IpcError::Operation,
    }
}
