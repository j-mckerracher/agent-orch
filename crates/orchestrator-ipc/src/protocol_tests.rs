use super::{read, write, FrameError, RpcId, RpcRequest, HEADER_LEN, MAX_BODY, VERSION};
use serde_json::{json, Value};
use std::io::{self, Cursor, Read};

#[test]
fn frame_round_trip_uses_versioned_big_endian_header() {
    let value = json!({"jsonrpc":"2.0", "id":7});
    let mut bytes = Vec::new();
    write(&mut bytes, &value).unwrap();
    assert_eq!(&bytes[..2], &VERSION.to_be_bytes());
    assert_eq!(
        u32::from_be_bytes(bytes[2..6].try_into().unwrap()) as usize,
        bytes.len() - HEADER_LEN
    );
    let decoded: Value = read(&mut Cursor::new(bytes)).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn oversized_length_is_rejected_before_body_read() {
    struct HeaderOnly(bool);
    impl Read for HeaderOnly {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            assert!(!self.0, "oversized frame must not request body bytes");
            self.0 = true;
            let header = [0, VERSION as u8, 0, 0x10, 0, 1];
            output.copy_from_slice(&header);
            Ok(HEADER_LEN)
        }
    }
    let error = read::<Value>(&mut HeaderOnly(false)).unwrap_err();
    assert!(matches!(error, FrameError::Oversized { maximum, .. } if maximum == MAX_BODY));
}

#[test]
fn truncated_header_and_body_are_typed() {
    assert!(matches!(
        read::<Value>(&mut Cursor::new([])),
        Err(FrameError::Truncated { part: "header" })
    ));
    let mut bytes = vec![0, VERSION as u8, 0, 0, 0, 2, b'{'];
    assert!(matches!(
        read::<Value>(&mut Cursor::new(&mut bytes)),
        Err(FrameError::Truncated { part: "body" })
    ));
}

#[test]
fn wrong_version_and_malformed_json_are_typed_without_payload() {
    let error = read::<Value>(&mut Cursor::new([0, 2, 0, 0, 0, 0])).unwrap_err();
    assert!(matches!(
        error,
        FrameError::WrongVersion {
            received: 2,
            supported: 1
        }
    ));
    let error = read::<Value>(&mut Cursor::new([0, 1, 0, 0, 0, 1, b'x'])).unwrap_err();
    assert!(matches!(&error, FrameError::MalformedJson));
    assert_eq!(error.to_string(), "malformed JSON frame");
}
#[test]
fn non_eof_read_errors_are_not_reported_as_truncated() {
    struct TimedOut;
    impl Read for TimedOut {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::TimedOut))
        }
    }
    assert!(matches!(
        read::<Value>(&mut TimedOut),
        Err(FrameError::Io(io::ErrorKind::TimedOut))
    ));
}
#[test]
fn json_rpc_string_request_ids_are_supported() {
    let request: RpcRequest = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": "health-request",
        "method": "health"
    }))
    .unwrap();
    assert_eq!(request.id, RpcId::String("health-request".into()));
}
