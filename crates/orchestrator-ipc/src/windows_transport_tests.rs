//! Native Windows tests for the named-pipe transport. They compile on other
//! hosts only as a `--target` check; they run only on Windows.

use super::*;
use crate::auth::Secret;
use crate::{protocol, Client, Server, ServerConfig};
use serde_json::{json, Value};
use tempfile::TempDir;
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, GetSecurityInfo,
    SE_FILE_OBJECT, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};

fn wide_to_string(text: *const u16) -> String {
    // SAFETY: callers pass a live NUL-terminated UTF-16 string returned by a
    // Windows API; the slice is copied before the owner frees it.
    unsafe {
        let mut length = 0usize;
        while *text.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    }
}

fn dacl_sddl(descriptor: PSECURITY_DESCRIPTOR) -> String {
    let mut text = std::ptr::null_mut();
    // SAFETY: `descriptor` is a live descriptor returned by the OS and `text`
    // is a writable output pointer for the LocalAlloc-backed string.
    let converted = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(converted, 0, "SDDL conversion failed: {}", last_error());
    let _free = LocalAllocation(text.cast());
    wide_to_string(text)
}

fn pipe_dacl(pipe: &File) -> String {
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: `pipe` owns a live handle for the call; the DACL output pointers
    // are null because only the self-contained descriptor is requested.
    let status = unsafe {
        GetSecurityInfo(
            pipe.as_raw_handle(),
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, 0, "GetSecurityInfo failed with {status}");
    let _free = LocalAllocation(descriptor);
    dacl_sddl(descriptor)
}

fn file_dacl(path: &Path) -> String {
    let path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated UTF-16 that outlives the call; the
    // other output pointers are null because only the descriptor is requested.
    let status = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, 0, "GetNamedSecurityInfoW failed with {status}");
    let _free = LocalAllocation(descriptor);
    dacl_sddl(descriptor)
}

/// Asserts a protected DACL holding exactly one allow ACE: the current user
/// with one of `masks` (SDDL text; the OS may render generic-all as `FA`).
fn assert_current_user_only(sddl: &str, masks: &[&str]) {
    let sid = current_user_sid().unwrap();
    assert!(
        masks
            .iter()
            .any(|mask| sddl == format!("D:P(A;;{mask};;;{sid})")),
        "DACL is not a protected single {masks:?} ACE for {sid}: {sddl}"
    );
}

fn start(dir: &Path) -> thread::JoinHandle<()> {
    let dir = dir.to_path_buf();
    thread::spawn(move || {
        Server::serve(
            ServerConfig::new(dir, "windows-test")
                .with_idle_timeout(Duration::from_secs(20))
                .with_stream_timeout(Duration::from_millis(500)),
        )
        .unwrap();
    })
}

fn wait_for_token(dir: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(token) = fs::read_to_string(dir.join("daemon.token")) {
            return token;
        }
        assert!(
            Instant::now() < deadline,
            "daemon did not publish its token"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn handshake(stream: &mut Box<dyn IpcStream>, token: &str) -> Value {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "handshake",
        "params": {"token": token, "versions": [1], "capabilities": ["health"]}});
    protocol::write(stream, &request).unwrap();
    protocol::read::<Value>(stream).unwrap()
}

/// Connects a client to a fresh listener and returns (client, accepted server).
fn connected_pair(listener: &mut Listener, dir: &Path) -> (Box<dyn IpcStream>, Box<dyn IpcStream>) {
    let client = connect(dir).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok(server) => return (client, server),
            Err(TransportError::Io(io::ErrorKind::WouldBlock)) => {
                assert!(Instant::now() < deadline, "accept never saw the client");
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    }
}

#[test]
fn pipe_and_token_dacls_grant_only_the_current_user() {
    let dir = TempDir::new().unwrap();
    let mut listener = Listener::bind(dir.path()).unwrap();
    assert_current_user_only(
        &pipe_dacl(listener.pending.as_ref().unwrap()),
        &["GA", "FA"],
    );

    // Instances created after the first must carry the same descriptor.
    let _pair = connected_pair(&mut listener, dir.path());
    assert_current_user_only(
        &pipe_dacl(listener.pending.as_ref().unwrap()),
        &["GA", "FA"],
    );

    let _secret = Secret::create(dir.path()).unwrap();
    assert_current_user_only(&file_dacl(&dir.path().join("daemon.token")), &["FA"]);
}

#[test]
fn pipe_limits_and_remote_rejection_are_configured() {
    assert_eq!(PIPE_MAX_INSTANCES, 16);
    assert_ne!(PIPE_MODE & PIPE_REJECT_REMOTE_CLIENTS, 0);
}

#[test]
fn second_listener_for_the_same_data_directory_is_refused() {
    let dir = TempDir::new().unwrap();
    let _first = Listener::bind(dir.path()).unwrap();
    assert!(matches!(
        Listener::prepare(dir.path()),
        Err(TransportError::AlreadyRunning)
    ));
    assert!(matches!(
        Listener::bind(dir.path()),
        Err(TransportError::AlreadyRunning)
    ));
}

#[test]
fn instance_limit_is_enforced_and_released() {
    let dir = TempDir::new().unwrap();
    let mut listener = Listener::bind(dir.path()).unwrap();
    let mut held = Vec::new();
    for _ in 0..16 {
        held.push(connected_pair(&mut listener, dir.path()));
    }
    assert!(matches!(
        listener.accept(),
        Err(TransportError::Io(io::ErrorKind::WouldBlock))
    ));
    assert!(listener.pending.is_none(), "instance exceeded the limit");
    assert!(connect(dir.path()).is_err(), "client joined a full pipe");

    drop(held.pop());
    assert!(matches!(
        listener.accept(),
        Err(TransportError::Io(io::ErrorKind::WouldBlock))
    ));
    assert!(
        listener.pending.is_some(),
        "released instance was not reusable"
    );
}

#[test]
fn pipe_name_stays_bound_when_a_client_connects_and_leaves() {
    let dir = TempDir::new().unwrap();
    let mut listener = Listener::bind(dir.path()).unwrap();
    for _ in 0..5 {
        drop(connect(dir.path()).unwrap());
        for _ in 0..3 {
            // Any accept result is fine; the name must never lose its last instance.
            let _ = listener.accept();
            assert!(matches!(
                Listener::prepare(dir.path()),
                Err(TransportError::AlreadyRunning)
            ));
            assert!(matches!(
                Listener::bind(dir.path()),
                Err(TransportError::AlreadyRunning)
            ));
        }
        assert!(listener.pending.is_some());
    }
}

#[test]
fn write_to_a_closed_peer_fails_promptly() {
    let dir = TempDir::new().unwrap();
    let mut listener = Listener::bind(dir.path()).unwrap();
    let (client, mut server) = connected_pair(&mut listener, dir.path());
    drop(client);
    let started = Instant::now();
    assert!(server.write_all(b"x").is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn every_connection_must_complete_the_authenticated_handshake() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path());
    let token = wait_for_token(dir.path());

    let mut skipped = connect(dir.path()).unwrap();
    protocol::write(
        &mut skipped,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "health", "params": {}}),
    )
    .unwrap();
    assert!(protocol::read::<Value>(&mut skipped).is_err());

    let mut wrong_token = token.clone();
    wrong_token.replace_range(..1, if token.starts_with('0') { "1" } else { "0" });
    let mut wrong = connect(dir.path()).unwrap();
    let response = handshake(&mut wrong, &wrong_token).to_string();
    assert!(response.contains("authentication failed"));
    assert!(!response.contains(&token) && !response.contains(&wrong_token));

    let mut accepted = connect(dir.path()).unwrap();
    assert!(handshake(&mut accepted, &token).get("result").is_some());
    drop(accepted);

    // A later connection gets no credit for an earlier accepted handshake.
    let mut later = connect(dir.path()).unwrap();
    protocol::write(
        &mut later,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "health", "params": {}}),
    )
    .unwrap();
    assert!(protocol::read::<Value>(&mut later).is_err());

    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(client.health().unwrap().package_version, "windows-test");
    client.shutdown().unwrap();
    daemon.join().unwrap();
    assert!(!dir.path().join("daemon.token").exists());
}

#[test]
fn stalled_connection_does_not_block_other_clients() {
    let dir = TempDir::new().unwrap();
    let daemon = start(dir.path());
    wait_for_token(dir.path());
    let mut stalled = connect(dir.path()).unwrap();
    stalled.write_all(&[0]).unwrap();
    let mut client = Client::connect(dir.path()).unwrap();
    assert_eq!(client.health().unwrap().package_version, "windows-test");
    client.shutdown().unwrap();
    daemon.join().unwrap();
}
