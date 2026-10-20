use crate::esign::bridge;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct Catalog {
    pub commands: Vec<Command>,
    pub declared_but_not_dispatched: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Command {
    pub code: String,
    pub required_strings: Vec<String>,
    #[serde(default)]
    pub conditional_strings: Vec<Condition>,
    #[serde(default)]
    pub subcommands: Vec<Command>,
}

#[derive(Debug, Deserialize)]
pub struct Condition {
    pub field: String,
    pub equals_ignore_case: String,
    pub required_strings: Vec<String>,
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("data/commands.json")).expect("embedded catalog")
    })
}

// The plugin protocol drops the payload on failure and uses STRING booleans.
pub fn response(code: &str, success: bool, payload: Map<String, Value>) -> Value {
    let mut result = if success { payload } else { Map::new() };
    result.insert("CODE".into(), Value::String(code.into()));
    result.insert(
        "SUCCESS".into(),
        Value::String(if success { "TRUE" } else { "FALSE" }.into()),
    );
    Value::Object(result)
}

fn check_strings(input: &Value, names: &[String]) -> Result<(), String> {
    for name in names {
        if !input.get(name).is_some_and(Value::is_string) {
            return Err(format!("missing or non-string field: {name}"));
        }
    }
    Ok(())
}

pub fn validate(input: &Value) -> Result<&'static Command, String> {
    let code = input
        .get("CODE")
        .and_then(Value::as_str)
        .ok_or("missing or non-string CODE")?;
    let command = catalog()
        .commands
        .iter()
        .find(|c| c.code.eq_ignore_ascii_case(code))
        .ok_or("unknown CODE")?;
    check_strings(input, &command.required_strings)?;
    for condition in &command.conditional_strings {
        if input[&condition.field]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(&condition.equals_ignore_case))
        {
            check_strings(input, &condition.required_strings)?;
        }
    }
    if !command.subcommands.is_empty() {
        let sub = input["SUB_CODE"].as_str().ok_or("missing SUB_CODE")?;
        let branch = command
            .subcommands
            .iter()
            .find(|c| c.code.eq_ignore_ascii_case(sub))
            .ok_or("unknown SUB_CODE")?;
        check_strings(input, &branch.required_strings)?;
    }
    Ok(command)
}

/// Answers a request without a signing backend. Version checks work without
/// a backend; signing commands fail explicitly.
pub fn handle(bytes: &[u8]) -> Value {
    handle_with(bytes, "", None).reply
}

/// One handled request: the reply and what the activity log records about it.
pub struct Handled {
    pub reply: Value,
    /// CODE (with SUB_CODE).
    pub action: String,
    /// Empty on success, otherwise why the request failed. Never payloads or PINs.
    pub message: String,
    /// True when a signature was attempted.
    pub signing: bool,
    /// Hex serial and subject of the certificate that signed.
    pub certificate: String,
    pub signer: String,
}

/// Routes CER_001, ONLINE_XML_CER_001 and ONLINE_SUBMIT_SIGNING_001 to
/// `backend` (see `bridge`). CHECK_VERSION is handled locally; unsupported
/// commands and signing requests without a backend answer FALSE.
pub fn handle_with(bytes: &[u8], origin: &str, backend: Option<&dyn bridge::Backend>) -> Handled {
    let (action, reason) = summary(bytes, backend.is_some());
    let failed = |code: &str, message: String, signing: bool| Handled {
        reply: response(code, false, Map::new()),
        action: action.clone(),
        message,
        signing,
        certificate: String::new(),
        signer: String::new(),
    };
    let Ok(input) = serde_json::from_slice::<Value>(bytes) else {
        eprintln!("invalid request JSON");
        return failed("", reason, false);
    };
    let code = input.get("CODE").and_then(Value::as_str).unwrap_or("");
    if let Err(e) = validate(&input) {
        eprintln!("request rejected: {e}");
        return failed(code, reason, false);
    }
    if code.eq_ignore_ascii_case("CHECK_VERSION") {
        // Match the original plugin's no-update reply. MacSigner has its own
        // updater; ROOT_URL must never trigger the Java plugin's installer.
        return Handled {
            reply: response(
                code,
                true,
                json!({"RESULT": "SUCC", "IS_UPDATE": "N"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            action,
            message: String::new(),
            signing: false,
            certificate: String::new(),
            signer: String::new(),
        };
    }
    let Some(backend) = backend.filter(|_| bridge::is_signing_command(code)) else {
        eprintln!("legacy operation is not available: {code}");
        return failed(code, reason, false);
    };
    // No operation may claim success until its real backend has completed:
    // `bridge::sign` returns only replies built from a verified signature.
    match bridge::sign(&input, origin, backend) {
        Ok(signed) => {
            let (certificate, signer) = bridge::describe(&signed.certificate_der);
            Handled {
                reply: signed.reply,
                action,
                message: String::new(),
                signing: true,
                certificate,
                signer,
            }
        }
        Err(e) => {
            eprintln!("signing failed: {e}");
            failed(code, format!("Ký không thành công: {e}"), true)
        }
    }
}

/// CODE (with SUB_CODE) of a request and any failure before reaching a
/// signing backend, for the activity log. Only these names are kept; the
/// payload never is. `with_backend` says whether a signing backend is attached.
pub fn summary(bytes: &[u8], with_backend: bool) -> (String, String) {
    let Ok(input) = serde_json::from_slice::<Value>(bytes) else {
        return ("?".into(), "JSON không hợp lệ".into());
    };
    let code = input.get("CODE").and_then(Value::as_str).unwrap_or("?");
    let action = match input.get("SUB_CODE").and_then(Value::as_str) {
        Some(sub) => format!("{code}/{sub}"),
        None => code.to_string(),
    };
    let reason = match validate(&input) {
        Err(reason) => format!("Yêu cầu bị từ chối: {reason}"),
        Ok(_) if bridge::is_signing_command(code) && !with_backend => {
            "Lệnh ký chỉ chạy khi dịch vụ ESIGN do ứng dụng MacSigner chính (app/) mở, để người dùng chọn chứng thư và xác nhận".into()
        }
        Ok(_) if bridge::is_signing_command(code) || code.eq_ignore_ascii_case("CHECK_VERSION") => {
            String::new()
        }
        Ok(_) => "Lệnh chưa được hỗ trợ trong bản này".into(),
    };
    (action, reason)
}

pub fn capabilities() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "experimental": true,
        "transport": ["native-messaging", "websocket"],
        "pkcs11_cli": ["token-list", "token-mechanisms", "cert-list", "sign-file-rsa-sha256", "sign-base64", "sign-base64-list", "sign-request"],
        "rsa_pkcs1_v1_5_hashes": ["sha1-explicit", "sha256", "sha384", "sha512"],
        "rsa_pkcs_fallback": "host hashes and encodes DER DigestInfo; token performs private-key operation",
        "legacy_cli_prepared_byte_commands": bridge::SIGNING_COMMANDS,
        "legacy_commands_implemented": std::iter::once("CHECK_VERSION")
            .chain(bridge::SIGNING_COMMANDS.iter().copied()).collect::<Vec<_>>(),
        // Signing over the bridge needs a backend that asks the user: the desktop
        // app's WebSocket. Native messaging and headless `serve` answer FALSE.
        "legacy_commands_signing_transport": ["websocket-with-desktop-approval"],
        "legacy_sha1_policy": "CER_001 only (SHA1withRSA as in the original plugin); ONLINE_* refuse SHA-1",
        "legacy_commands_catalogued": catalog().commands.iter().map(|c| &c.code).collect::<Vec<_>>(),
        "declared_but_not_dispatched": catalog().declared_but_not_dispatched,
        "real_portal_verified": false,
        "macos_linux_runtime_verified": false
    })
}
