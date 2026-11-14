use super::set_timeouts;
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[test]
fn timeout_applies_to_both_stream_directions() {
    let (stream, _peer) = UnixStream::pair().unwrap();
    let timeout = Duration::from_millis(125);
    set_timeouts(&stream, timeout).unwrap();
    assert_eq!(stream.read_timeout().unwrap(), Some(timeout));
    assert_eq!(stream.write_timeout().unwrap(), Some(timeout));
}
