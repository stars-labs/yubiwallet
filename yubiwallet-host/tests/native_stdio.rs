//! No card I/O: verify the real executable's framed info/error responses.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn stdout_contains_only_native_messaging_frames() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_yubiwallet-host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in [
        r#"{"id":7,"method":"get_info"}"#,
        r#"{"id":8,"method":"not_a_method"}"#,
    ] {
        stdin
            .write_all(&(message.len() as u32).to_le_bytes())
            .unwrap();
        stdin.write_all(message.as_bytes()).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let mut bytes = output.stdout.as_slice();
    for id in [7, 8] {
        let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let response: serde_json::Value = serde_json::from_slice(&bytes[4..4 + n]).unwrap();
        assert_eq!(response["id"], id);
        if id == 7 {
            assert_eq!(response["result"]["protocol"], 1);
        } else {
            assert_eq!(response["error"]["code"], "UNSUPPORTED_METHOD");
        }
        bytes = &bytes[4 + n..];
    }
    assert!(bytes.is_empty(), "non-protocol stdout bytes");
    assert!(output.stderr.is_empty());
}
