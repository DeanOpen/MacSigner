// Self-test checks runnable from the desktop, the `selftest` command and tests.
// They exercise the loopback services with real clients and replay the synthetic
// TEST-ONLY vectors recorded from the original Java and .NET implementations.
use crate::{
    config::{CtsignSettings, EsignSettings},
    ctsign::{
        model::SignHashRequest,
        signing::{Identity, SigningState},
    },
    esign::{
        bridge::{self, Backend, Key, KeyRequest},
        legacy,
        online::Algorithm,
        protocol,
    },
    services::{CtsignService, EsignService, Status},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub module: &'static str,
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
}

impl Check {
    fn new(module: &'static str, name: &str, result: Result<String, String>) -> Self {
        let (outcome, detail) = match result {
            Ok(detail) => (Outcome::Pass, detail),
            Err(detail) => (Outcome::Fail, detail),
        };
        Self {
            module,
            name: name.into(),
            outcome,
            detail,
        }
    }
    fn skip(module: &'static str, name: &str, detail: &str) -> Self {
        Self {
            module,
            name: name.into(),
            outcome: Outcome::Skip,
            detail: detail.into(),
        }
    }
}

pub const ESIGN_FIXTURES: &str = include_str!("esign/data/legacy-writer-fixtures.json");
pub const CTSIGN_VECTOR: &str = include_str!("../tests/fixtures/ctsign/dotnet-vector.json");
pub const CTSIGN_CERT: &[u8] = include_bytes!("../tests/fixtures/ctsign/certificate.der");
pub const CTSIGN_KEY: &str = include_str!("../tests/fixtures/ctsign/private-key.fixture");

/// Minimal HTTP/1.1 client for loopback probes (Connection: close, no chunking).
pub fn http(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<(u16, String), String> {
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let body = body.map(Value::to_string).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut response = Vec::new();
    stream
        .take(4 * 1024 * 1024)
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&response);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or("malformed HTTP response")?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("malformed HTTP status")?;
    Ok((status, body.to_owned()))
}

/// Sends one text message to the ESIGN WebSocket with the given Origin.
pub fn websocket(address: SocketAddr, origin: &str, message: &str) -> Result<String, String> {
    use tungstenite::{Message, client::IntoClientRequest, http::HeaderValue};
    let mut request = format!("ws://{address}/plugin/sign")
        .into_client_request()
        .map_err(|e| e.to_string())?;
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_str(origin).map_err(|e| e.to_string())?,
    );
    let stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let (mut ws, _) = tungstenite::client(request, stream).map_err(|e| e.to_string())?;
    ws.send(Message::Text(message.into()))
        .map_err(|e| e.to_string())?;
    loop {
        match ws.read().map_err(|e| e.to_string())? {
            Message::Text(text) => {
                let _ = ws.close(None);
                return Ok(text.to_string());
            }
            Message::Close(_) => return Err("server closed the connection".into()),
            _ => {}
        }
    }
}

fn software_sign(
    key: &rsa::RsaPrivateKey,
    inputs: &[Vec<u8>],
    algorithm: Algorithm,
) -> Vec<Vec<u8>> {
    use rsa::{
        pkcs1v15::SigningKey,
        signature::{SignatureEncoding, Signer},
    };
    inputs
        .iter()
        .map(|input| match algorithm {
            Algorithm::Sha1Rsa => SigningKey::<sha1::Sha1>::new(key.clone())
                .sign(input)
                .to_vec(),
            Algorithm::Sha256Rsa => SigningKey::<sha2::Sha256>::new(key.clone())
                .sign(input)
                .to_vec(),
            Algorithm::Sha384Rsa => SigningKey::<sha2::Sha384>::new(key.clone())
                .sign(input)
                .to_vec(),
            Algorithm::Sha512Rsa => SigningKey::<sha2::Sha512>::new(key.clone())
                .sign(input)
                .to_vec(),
        })
        .collect()
}

fn esign_request(code: &str, base64_value: &str, serial: &str) -> Value {
    json!({
        "CODE": code, "BASE64_VALUE": base64_value, "SERIAL_NUMBER": serial,
        "CHECK_CERT_SERIAL": "true", "CHECK_TIN": "false", "TIN_VALID": "",
        "DATE_SIGNING": "", "TIMEZONE_SIGNING": "", "PIN_CODE": "",
        "PKCS_11_LIBRARY_NAME_LIST": ""
    })
}

/// Synthetic certificate, serial and private keys recorded from the Java app.
fn esign_fixture() -> Result<(Value, Vec<u8>, String), String> {
    let data: Value = serde_json::from_str(ESIGN_FIXTURES).map_err(|e| e.to_string())?;
    let cert = STANDARD
        .decode(data["certificate"]["der_base64"].as_str().unwrap_or(""))
        .map_err(|e| e.to_string())?;
    let serial = data["certificate"]["serial_decimal"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    Ok((data, cert, serial))
}

fn fixture_key(case: &Value) -> Result<rsa::RsaPrivateKey, String> {
    use rsa::pkcs8::DecodePrivateKey;
    rsa::RsaPrivateKey::from_pkcs8_der(
        &STANDARD
            .decode(case["private_pkcs8_base64"].as_str().unwrap_or(""))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Test signing through the real ESIGN legacy flow (certificate checks, signing,
/// verification, response shape) with the TEST ONLY certificate instead of a token.
pub fn esign_sample_sign(data: &[u8], code: &str) -> Result<Value, String> {
    let (fixtures, cert, serial) = esign_fixture()?;
    let key = fixture_key(&fixtures["signatures"][0])?;
    let request = esign_request(code, &STANDARD.encode(data), &serial);
    legacy::sign_request(&request, &cert, true, |inputs, algorithm| {
        Ok(software_sign(&key, inputs, algorithm))
    })
    .map_err(|e| e.to_string())
}

/// Signs browser requests with the TEST ONLY certificate and approves every
/// request without asking. For self-tests and tests only; never give it to a
/// service that real pages can reach.
pub struct SampleBackend {
    cert: Vec<u8>,
    key: rsa::RsaPrivateKey,
    serial: String,
}

impl SampleBackend {
    pub fn new() -> Result<Self, String> {
        let (fixtures, cert, serial) = esign_fixture()?;
        Ok(Self {
            key: fixture_key(&fixtures["signatures"][0])?,
            cert,
            serial,
        })
    }

    /// Decimal serial, as the portal sends it in SERIAL_NUMBER.
    pub fn serial(&self) -> &str {
        &self.serial
    }
}

impl Key for SampleBackend {
    fn certificate(&self) -> &[u8] {
        &self.cert
    }

    fn sign(
        &self,
        data: &[u8],
        algorithm: Algorithm,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(software_sign(&self.key, &[data.to_vec()], algorithm).remove(0))
    }
}

impl Backend for SampleBackend {
    fn with_key(
        &self,
        request: &KeyRequest,
        sign: &mut dyn FnMut(&dyn Key) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !request.accepts(&self.cert) {
            return Err("chứng thư mẫu không khớp serial/MST của yêu cầu".into());
        }
        sign(self)
    }
}

/// The three signing commands over a temporary WebSocket service backed by the
/// TEST ONLY key. A running service is never used: its backend asks the user.
fn esign_bridge(settings: &EsignSettings, origin: &str) -> Result<String, String> {
    let backend = SampleBackend::new()?;
    let serial = backend.serial().to_owned();
    let mut temp = EsignService::with_backend(Arc::new(backend));
    let address = temp.start(&EsignSettings {
        port: 0,
        fallback_port: None,
        ..settings.clone()
    })?;
    let one = STANDARD.encode("Ký thử qua cầu nối".as_bytes());
    for code in bridge::SIGNING_COMMANDS {
        let values = if code == "ONLINE_SUBMIT_SIGNING_001" {
            format!("{one},{one}")
        } else {
            one.clone()
        };
        let request = esign_request(code, &values, &serial);
        let text = websocket(address, origin, &request.to_string())?;
        let reply: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if reply["SUCCESS"] != "TRUE" || reply["SIGNED_CONTENT"].as_str().is_none_or(str::is_empty)
        {
            return Err(format!("{code}: phản hồi không đúng: {text}"));
        }
    }
    temp.stop();
    Ok(format!(
        "{} ký và tự xác minh qua ws://{address}/plugin/sign (khóa TEST ONLY)",
        bridge::SIGNING_COMMANDS.join(", ")
    ))
}

fn esign_vectors() -> Result<String, String> {
    let (data, cert, serial) = esign_fixture()?;
    let mut matched = 0;
    for (case, code) in data["signatures"]
        .as_array()
        .ok_or("fixture has no signatures")?
        .iter()
        .zip(["CER_001", "ONLINE_XML_CER_001"])
    {
        let key = fixture_key(case)?;
        let request = esign_request(code, case["base64_input"].as_str().unwrap_or(""), &serial);
        let response = legacy::sign_request(&request, &cert, true, |inputs, algorithm| {
            Ok(software_sign(&key, inputs, algorithm))
        })
        .map_err(|e| e.to_string())?;
        if response["SIGNED_CONTENT"] != case["signature_base64"] {
            return Err(format!("{code}: chữ ký khác vector Java"));
        }
        matched += 1;
    }
    Ok(format!(
        "{matched}/2 chữ ký trùng byte-for-byte với Java (khóa TEST ONLY)"
    ))
}

/// A signing state holding only the CTSIGN TEST ONLY certificate.
pub fn ctsign_sample_state() -> Result<SigningState, String> {
    use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey};
    let state = SigningState::default();
    let key = RsaPrivateKey::from_pkcs8_pem(CTSIGN_KEY).map_err(|e| e.to_string())?;
    state
        .add(vec![
            Identity::software(CTSIGN_CERT.to_vec(), key).map_err(|e| e.to_string())?,
        ])
        .map_err(|e| e.to_string())?;
    Ok(state)
}

/// Hashes `data` with SHA-256 and signs it through CTSIGN's SignHash path.
pub fn ctsign_sample_sign(state: &SigningState, serial: &str, data: &[u8]) -> Value {
    use sha2::Digest;
    let request = SignHashRequest {
        request_id: None,
        signature: None,
        serinumber: serial.to_owned(),
        hash: vec![crate::ctsign::model::HashInfo {
            ma: Some("ky-thu".into()),
            digest: STANDARD.encode(sha2::Sha256::digest(data)),
            signature: None,
        }],
    };
    serde_json::to_value(state.sign_hash(request)).unwrap_or(Value::Null)
}
fn esign_live(address: SocketAddr, origin: &str) -> Vec<Check> {
    let reply = websocket(address, origin, r#"{"CODE":"CER_002"}"#).and_then(|text| {
        let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if value == json!({"CODE":"CER_002","SUCCESS":"FALSE"}) {
            Ok(format!("ws://{address}/plugin/sign → {text}"))
        } else {
            Err(format!("phản hồi không đúng: {text}"))
        }
    });
    let rejected = match websocket(address, "https://attacker.invalid", r#"{"CODE":"CER_002"}"#) {
        Ok(text) => Err(format!("origin lạ không bị chặn: {text}")),
        Err(e) if e.contains("403") => Ok("origin lạ bị từ chối (HTTP 403)".into()),
        Err(e) => Err(format!("lỗi không mong đợi: {e}")),
    };
    vec![
        Check::new("ESIGN", "WebSocket: origin hợp lệ", reply),
        Check::new("ESIGN", "WebSocket: chặn origin lạ", rejected),
    ]
}

pub fn esign(settings: &EsignSettings, live: Option<SocketAddr>) -> Vec<Check> {
    let catalog = protocol::catalog();
    let subcommands: usize = catalog.commands.iter().map(|c| c.subcommands.len()).sum();
    let mut checks = vec![
        Check::new(
            "ESIGN",
            "Danh mục lệnh giao thức",
            if catalog.commands.len() == 24 && subcommands == 9 {
                Ok("24 lệnh, 9 lệnh con".into())
            } else {
                Err(format!(
                    "{} lệnh, {subcommands} lệnh con",
                    catalog.commands.len()
                ))
            },
        ),
        Check::new("ESIGN", "Vector chữ ký Java", esign_vectors()),
    ];
    let origin = settings
        .origins
        .first()
        .cloned()
        .unwrap_or_else(|| crate::esign::websocket::DEFAULT_ORIGIN.into());
    checks.push(Check::new(
        "ESIGN",
        "Cầu nối ký WebSocket (dịch vụ tạm)",
        esign_bridge(settings, &origin),
    ));
    match live {
        Some(address) => checks.extend(esign_live(address, &origin)),
        None => {
            // Service is off: exercise an identical temporary instance on a free port.
            let mut temp = EsignService::default();
            let temp_settings = EsignSettings {
                port: 0,
                fallback_port: None,
                ..settings.clone()
            };
            match temp.start(&temp_settings) {
                Ok(address) => checks.extend(esign_live(address, &origin)),
                Err(e) => checks.push(Check::new("ESIGN", "WebSocket tạm", Err(e))),
            }
            temp.stop();
        }
    }
    checks.push(certificate_source_check());
    checks
}

/// Certificates the user can sign with, found without any configuration: the
/// Windows certificate store (as CTSigningHub) or auto-discovered tokens.
fn certificate_source_check() -> Check {
    #[cfg(windows)]
    {
        Check::new(
            "CHUNG",
            "Kho chứng thư Windows",
            crate::certstore::personal()
                .map(|certs| format!("{} chứng thư có khóa ký", certs.len())),
        )
    }
    #[cfg(not(windows))]
    {
        let modules = crate::esign::discovery::modules();
        if modules.is_empty() {
            Check::skip(
                "CHUNG",
                "Driver USB token",
                "Không tìm thấy driver token đã cài",
            )
        } else {
            Check::new(
                "CHUNG",
                "Driver USB token",
                Ok(format!("{} driver tự dò được", modules.len())),
            )
        }
    }
}

fn ctsign_vector() -> Result<String, String> {
    let vector: Value = serde_json::from_str(CTSIGN_VECTOR).map_err(|e| e.to_string())?;
    let state = ctsign_sample_state()?;
    let request: SignHashRequest = serde_json::from_value(json!({
        "serinumber": vector["serial"], "hash": [{"Ma": "selftest", "DLieuHash": vector["digest"]}]
    }))
    .map_err(|e| e.to_string())?;
    let response = state.sign_hash(request);
    let signature = response
        .data
        .and_then(|d| d.into_iter().next())
        .and_then(|h| h.signature);
    if signature.as_deref() == vector["signature"].as_str() {
        Ok("Chữ ký trùng byte-for-byte với .NET RSA.SignHash (khóa TEST ONLY)".into())
    } else {
        Err(response
            .message
            .unwrap_or_else(|| "chữ ký khác vector .NET".into()))
    }
}
fn expect(
    address: SocketAddr,
    path: &str,
    body: Option<&Value>,
    check: impl Fn(&Value) -> bool,
) -> Result<String, String> {
    let method = if body.is_some() { "POST" } else { "GET" };
    let (status, text) = http(address, method, path, body)?;
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status == 200 && check(&value) {
        Ok(format!("{method} {path} → {text}"))
    } else {
        Err(format!("{method} {path} → HTTP {status} {text}"))
    }
}

fn ctsign_live(address: SocketAddr, state: &SigningState) -> Vec<Check> {
    let api_enabled = state
        .api_signing_enabled
        .load(std::sync::atomic::Ordering::SeqCst);
    let mut checks = vec![
        Check::new(
            "CTSIGN",
            "API GetInfoCert",
            expect(address, "/api/Info/GetInfoCert", None, |v| v == "1"),
        ),
        Check::new(
            "CTSIGN",
            "API SignHash/Version",
            expect(address, "/api/SignHash/Version", None, |v| {
                v == "3333333333333333"
            }),
        ),
        Check::new(
            "CTSIGN",
            "API GetCert",
            expect(address, "/api/HubClient/GetCert", None, |v| {
                v["status"] == 0 || v["status"] == 200
            }),
        ),
        Check::new(
            "CTSIGN",
            "API signxml trả 501 (chưa triển khai)",
            expect(address, "/api/HubClient/signxml", Some(&json!({})), |v| {
                v["status"] == 501
            }),
        ),
    ];
    if api_enabled {
        checks.push(Check::skip(
            "CTSIGN",
            "API SignHash bị khóa khi chưa cho phép",
            "Đang bật ký hash qua API",
        ));
    } else {
        checks.push(Check::new(
            "CTSIGN",
            "API SignHash bị khóa khi chưa cho phép",
            expect(
                address,
                "/api/HubClient/SignHash",
                Some(&json!({"serinumber": "00", "hash": []})),
                |v| v["status"] == 403,
            ),
        ));
        checks.push(Check::new(
            "CTSIGN",
            "API signpdf bị khóa khi chưa cho phép",
            expect(
                address,
                "/api/HubClient/signpdf",
                Some(&json!({"serinumber": "00", "sig": []})),
                |v| v["status"] == 403,
            ),
        ));
    }
    checks
}

pub fn ctsign(state: &SigningState, live: Option<SocketAddr>) -> Vec<Check> {
    let mut checks = vec![Check::new("CTSIGN", "Vector chữ ký .NET", ctsign_vector())];
    match live {
        Some(address) => checks.extend(ctsign_live(address, state)),
        None => match CtsignService::new(SigningState::default()) {
            Ok(mut temp) => {
                let temp_settings = CtsignSettings {
                    port: 0,
                    ..Default::default()
                };
                match temp.start(&temp_settings) {
                    Ok(address) => checks.extend(ctsign_live(address, &temp.state)),
                    Err(e) => checks.push(Check::new("CTSIGN", "API tạm", Err(e))),
                }
                temp.stop();
            }
            Err(e) => checks.push(Check::new("CTSIGN", "API tạm", Err(e.to_string()))),
        },
    }
    checks
}

pub fn live_address(status: &Status) -> Option<SocketAddr> {
    match status {
        Status::Running(address) => Some(*address),
        _ => None,
    }
}
