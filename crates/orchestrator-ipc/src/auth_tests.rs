use super::{authenticate, decode, AuthError, Capability, Handshake, Secret};
use std::fs;
use tempfile::TempDir;

#[test]
fn chooses_highest_common_version_and_rejects_unknown_capability_safely() {
    let dir = TempDir::new().unwrap();
    let secret = Secret::create(dir.path()).unwrap();
    let token = fs::read_to_string(&secret.path).unwrap();
    let accepted = Handshake {
        token: Some(token.clone()),
        versions: vec![3, 1, 2],
        capabilities: vec!["health".into()],
    };
    assert_eq!(
        authenticate(&secret, &accepted, &Capability::ALL)
            .unwrap()
            .0,
        1
    );
    let rejected = Handshake {
        token: Some(token.clone()),
        versions: vec![1],
        capabilities: vec!["attacker-controlled-capability".into()],
    };
    let error = authenticate(&secret, &rejected, &Capability::ALL).unwrap_err();
    assert!(matches!(&error, AuthError::UnsupportedCapability));
    assert!(!error.to_string().contains("attacker-controlled-capability"));
    assert!(!format!("{secret:?}").contains(&token));
    assert!(!format!("{error:?}").contains("attacker-controlled-capability"));
}

#[test]
fn absent_token_is_authentication_failure() {
    let dir = TempDir::new().unwrap();
    let secret = Secret::create(dir.path()).unwrap();
    let handshake = Handshake {
        token: None,
        versions: vec![1],
        capabilities: vec![],
    };
    assert!(matches!(
        authenticate(&secret, &handshake, &Capability::ALL),
        Err(AuthError::Failed)
    ));
}

#[test]
fn startup_replaces_only_regular_stale_token() {
    let dir = TempDir::new().unwrap();
    let old = Secret::create(dir.path()).unwrap();
    let old_token = fs::read_to_string(&old.path).unwrap();
    std::mem::forget(old);
    let fresh = Secret::create(dir.path()).unwrap();
    assert_ne!(fs::read_to_string(&fresh.path).unwrap(), old_token);
    drop(fresh);
    assert!(!dir.path().join("daemon.token").exists());

    fs::create_dir(dir.path().join("daemon.token")).unwrap();
    assert!(matches!(
        Secret::create(dir.path()),
        Err(AuthError::UnsafeTokenFile)
    ));
    assert!(dir.path().join("daemon.token").is_dir());
}

#[test]
fn reports_no_common_version() {
    let dir = TempDir::new().unwrap();
    let secret = Secret::create(dir.path()).unwrap();
    let token = fs::read_to_string(&secret.path).unwrap();
    let handshake = Handshake {
        token: Some(token),
        versions: vec![2, 3],
        capabilities: vec![],
    };
    assert!(matches!(
        authenticate(&secret, &handshake, &Capability::ALL),
        Err(AuthError::UnsupportedVersion)
    ));
}
#[test]
fn token_encoding_requires_lowercase_hex() {
    let lowercase = "ab".repeat(32);
    assert!(decode(&lowercase).is_some());
    assert!(decode(&lowercase.to_uppercase()).is_none());
}
