use serde_json::Value;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

fn worker() -> Command {
    Command::new(env!("CARGO_BIN_EXE_macsigner-token-worker"))
}

#[test]
fn version_reports_protocol_and_capabilities() {
    let output = worker().arg("version").output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["protocol"], 1);
    assert_eq!(response["architecture"], std::env::consts::ARCH);
    assert_eq!(
        response["capabilities"],
        serde_json::json!(["probe-token", "probe-cert", "sign-digest"])
    );
}

#[test]
fn invalid_operations_and_arguments_never_emit_success_json() {
    for arguments in [
        vec![],
        vec!["version", "unexpected"],
        vec!["sign"],
        vec!["probe-token", "relative.dll"],
        vec!["probe-cert"],
    ] {
        let output = worker().args(arguments).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn signing_rejects_bad_frames_before_loading_a_module() {
    for body in [
        vec![],
        0u32.to_le_bytes().to_vec(),
        (64u32 * 1024 + 1).to_le_bytes().to_vec(),
        [18u32.to_le_bytes().as_slice(), b"{\"pin\":\"test-only\""].concat(),
    ] {
        let mut child = worker()
            .args(["sign-digest", env!("CARGO_BIN_EXE_macsigner-token-worker")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&body).unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("test-only"));
    }
}
