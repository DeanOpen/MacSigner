use macsigner::esign::native;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn executable_native_mode_keeps_stdout_protocol_clean() {
    let executable = env!("CARGO_BIN_EXE_macsigner");
    let mut child = Command::new(executable)
        .args(["esign", "native"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for code in ["CER_002", "unknown"] {
            native::write_frame(
                &mut stdin,
                &serde_json::to_vec(&json!({"CODE":code})).unwrap(),
            )
            .unwrap();
        }
        stdin.flush().unwrap();
    }
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let mut stdout = Cursor::new(result.stdout);
    for code in ["CER_002", "unknown"] {
        let frame = native::read_frame(&mut stdout).unwrap().unwrap();
        let response: Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(response, json!({"CODE":code,"SUCCESS":"FALSE"}));
    }
    assert!(native::read_frame(&mut stdout).unwrap().is_none());
}

#[test]
fn sign_request_reserves_output_and_removes_it_on_failure() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output =
        std::env::temp_dir().join(format!("esign-rs-cli-{}-{unique}.json", std::process::id()));
    let input =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/prepared-sign-request.json");
    let missing_module = std::env::temp_dir().join(format!("esign-rs-missing-{unique}.so"));
    let command = || {
        Command::new(env!("CARGO_BIN_EXE_macsigner"))
            .arg("sign-request")
            .arg(&missing_module)
            .arg("0")
            .arg("01")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap()
    };

    fs::write(&output, b"preserve existing file").unwrap();
    assert!(!command().status.success());
    assert_eq!(fs::read(&output).unwrap(), b"preserve existing file");
    fs::remove_file(&output).unwrap();

    assert!(!command().status.success());
    assert!(!output.exists());
}
