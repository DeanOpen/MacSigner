// End to end over the WebSocket: the three prepared-byte commands reach a
// signing backend (TEST ONLY software key) and come back signed; everything
// else, every refusal and every backend failure answers FALSE.
use base64::{Engine, engine::general_purpose::STANDARD};
use macsigner::{
    activity,
    config::EsignSettings,
    esign::{
        bridge::{Backend, Key, KeyRequest},
        certificate,
        online::Algorithm,
        protocol,
    },
    selftest::{self, SampleBackend},
    services::EsignService,
};
use rsa::{
    pkcs1v15::{Signature, VerifyingKey},
    signature::Verifier,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const ORIGIN: &str = "https://thuedientu.gdt.gov.vn";

fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "../src/esign/data/legacy-writer-fixtures.json"
    ))
    .unwrap()
}

fn fixture_cert() -> Vec<u8> {
    STANDARD
        .decode(fixtures()["certificate"]["der_base64"].as_str().unwrap())
        .unwrap()
}

/// What the test backend saw of one request.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    code: String,
    origin: String,
    serials: Vec<String>,
    tin: Option<String>,
    count: usize,
    pin: Option<String>,
}

#[derive(Clone, Copy)]
enum Mode {
    /// Offers the certificate only when the request accepts it (as the app does).
    Approve,
    /// Signs with the certificate even when the request does not accept it.
    IgnoreConstraints,
    /// The user declined.
    Decline,
    /// A faulty backend that reports success without signing.
    SkipSigning,
}

struct TestBackend {
    sample: SampleBackend,
    mode: Mode,
    seen: Mutex<Vec<Seen>>,
}

impl TestBackend {
    fn new(mode: Mode) -> Arc<Self> {
        Arc::new(Self {
            sample: SampleBackend::new().unwrap(),
            mode,
            seen: Mutex::new(Vec::new()),
        })
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

impl Backend for TestBackend {
    fn with_key(
        &self,
        request: &KeyRequest,
        sign: &mut dyn FnMut(&dyn Key) -> Result<()>,
    ) -> Result<()> {
        self.seen.lock().unwrap().push(Seen {
            code: request.code.clone(),
            origin: request.origin.clone(),
            serials: request.serials.clone(),
            tin: request.tin.clone(),
            count: request.count,
            pin: request.pin.as_ref().map(|p| p.as_str().to_owned()),
        });
        match self.mode {
            Mode::Approve => self.sample.with_key(request, sign),
            Mode::IgnoreConstraints => sign(&self.sample),
            Mode::Decline => Err("người dùng từ chối".into()),
            Mode::SkipSigning => Ok(()),
        }
    }
}

fn start(backend: Option<Arc<dyn Backend>>) -> (EsignService, SocketAddr) {
    let mut service = match backend {
        Some(backend) => EsignService::with_backend(backend),
        None => EsignService::default(),
    };
    let address = service
        .start(&EsignSettings {
            port: 0,
            fallback_port: None,
            ..Default::default()
        })
        .unwrap();
    (service, address)
}

fn request(code: &str, values: &str, serial: &str) -> Value {
    json!({
        "CODE": code, "BASE64_VALUE": values, "SERIAL_NUMBER": serial,
        "CHECK_CERT_SERIAL": "true", "CHECK_TIN": "false", "TIN_VALID": "",
        "DATE_SIGNING": "", "TIMEZONE_SIGNING": "", "PIN_CODE": "",
        "PKCS_11_LIBRARY_NAME_LIST": ""
    })
}

fn send(address: SocketAddr, request: &Value) -> Value {
    let text = selftest::websocket(address, ORIGIN, &request.to_string()).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn verify(cert: &[u8], data: &[u8], signature_b64: &str, algorithm: Algorithm) {
    let key = certificate::rsa_public_key(cert).unwrap();
    let signature =
        Signature::try_from(STANDARD.decode(signature_b64).unwrap().as_slice()).unwrap();
    match algorithm {
        Algorithm::Sha1Rsa => VerifyingKey::<sha1::Sha1>::new(key).verify(data, &signature),
        Algorithm::Sha256Rsa => VerifyingKey::<sha2::Sha256>::new(key).verify(data, &signature),
        Algorithm::Sha384Rsa => VerifyingKey::<sha2::Sha384>::new(key).verify(data, &signature),
        Algorithm::Sha512Rsa => VerifyingKey::<sha2::Sha512>::new(key).verify(data, &signature),
    }
    .unwrap();
}

fn assert_false(reply: &Value, code: &str) {
    assert_eq!(reply, &json!({"CODE": code, "SUCCESS": "FALSE"}));
}

#[test]
fn signing_commands_succeed_over_websocket_and_verify() {
    let backend = TestBackend::new(Mode::Approve);
    let (mut service, address) = start(Some(backend.clone()));
    let cert = fixture_cert();
    let serial = backend.sample.serial().to_owned();
    let first = b"<SignedInfo>first</SignedInfo>".to_vec();
    let second = b"<SignedInfo>second</SignedInfo>".to_vec();
    let one = STANDARD.encode(&first);
    let both = format!("{one},{}", STANDARD.encode(&second));
    let pin = "1234-test-pin";

    for code in ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"] {
        let values = if code == "ONLINE_SUBMIT_SIGNING_001" {
            both.clone()
        } else {
            one.clone()
        };
        let mut input = request(code, &values, &serial);
        input["PIN_CODE"] = json!(pin);
        let reply = send(address, &input);
        assert_eq!(reply["SUCCESS"], "TRUE", "{code}: {reply}");
        assert_eq!(reply["CODE"], code);
        assert_eq!(reply["CERT"], STANDARD.encode(&cert));
        assert_eq!(
            reply["CERT_CHAIN"],
            fixtures()["certificate"]["pki_path_base64"]
        );
        // The PIN is used, never returned.
        assert_eq!(reply["PIN_CODE"], "");
        let algorithm = if code == "CER_001" {
            Algorithm::Sha1Rsa
        } else {
            certificate::signature_algorithm(&cert).unwrap()
        };
        let signatures: Vec<&str> = reply["SIGNED_CONTENT"]
            .as_str()
            .unwrap()
            .split(',')
            .collect();
        let inputs = if code == "ONLINE_SUBMIT_SIGNING_001" {
            vec![&first, &second]
        } else {
            vec![&first]
        };
        assert_eq!(signatures.len(), inputs.len(), "{code}");
        for (data, signature) in inputs.into_iter().zip(signatures) {
            verify(&cert, data, signature, algorithm);
        }
    }

    let seen = backend.seen();
    assert_eq!(
        seen.iter().map(|s| s.code.as_str()).collect::<Vec<_>>(),
        ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"]
    );
    for s in &seen {
        assert_eq!(s.origin, ORIGIN);
        assert_eq!(s.serials, std::slice::from_ref(&serial));
        assert_eq!(s.tin, None);
        assert_eq!(s.pin.as_deref(), Some(pin));
    }
    assert_eq!(seen[2].count, 2);

    // The history records the command, certificate and outcome; never the PIN
    // or the data that was signed.
    let entries = activity::entries();
    let signed = entries
        .iter()
        .find(|e| e.action == "ONLINE_SUBMIT_SIGNING_001" && e.ok)
        .expect("signing recorded in the activity history");
    assert!(signed.signing);
    assert_eq!(signed.status, "TRUE");
    assert!(!signed.certificate.is_empty());
    for entry in &entries {
        let text = serde_json::to_string(entry).unwrap();
        assert!(!text.contains(pin) && !text.contains(&one), "{text}");
    }
    service.stop();
}

#[test]
fn websocket_signatures_match_the_original_java_vectors() {
    let backend = TestBackend::new(Mode::Approve);
    let (mut service, address) = start(Some(backend.clone()));
    let data = fixtures();
    for (case, code) in data["signatures"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["CER_001", "ONLINE_XML_CER_001"])
    {
        let reply = send(
            address,
            &request(
                code,
                case["base64_input"].as_str().unwrap(),
                backend.sample.serial(),
            ),
        );
        assert_eq!(reply["SIGNED_CONTENT"], case["signature_base64"], "{code}");
    }
    service.stop();
}

#[test]
fn tax_id_check_is_passed_to_the_backend_and_enforced() {
    let backend = TestBackend::new(Mode::Approve);
    let (mut service, address) = start(Some(backend.clone()));
    let mut input = request("ONLINE_XML_CER_001", &STANDARD.encode(b"data"), "");
    input["CHECK_CERT_SERIAL"] = json!("false");
    input["CHECK_TIN"] = json!("true");
    input["TIN_VALID"] = json!("1234567890");
    assert_eq!(send(address, &input)["SUCCESS"], "TRUE");
    input["TIN_VALID"] = json!("0000000000");
    assert_false(&send(address, &input), "ONLINE_XML_CER_001");
    let seen = backend.seen();
    assert_eq!(seen[0].serials, Vec::<String>::new());
    assert_eq!(seen[0].tin.as_deref(), Some("1234567890"));
    assert_eq!(seen[1].tin.as_deref(), Some("0000000000"));
    service.stop();
}

#[test]
fn certificate_mismatch_fails_even_if_the_backend_ignores_it() {
    for mode in [Mode::Approve, Mode::IgnoreConstraints] {
        let backend = TestBackend::new(mode);
        let (mut service, address) = start(Some(backend.clone()));
        for code in ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"] {
            let reply = send(address, &request(code, &STANDARD.encode(b"data"), "999"));
            assert_false(&reply, code);
        }
        assert_eq!(backend.seen().len(), 3);
        service.stop();
    }
}

#[test]
fn declined_or_unsigned_requests_never_claim_success() {
    for mode in [Mode::Decline, Mode::SkipSigning] {
        let backend = TestBackend::new(mode);
        let (mut service, address) = start(Some(backend.clone()));
        let serial = backend.sample.serial().to_owned();
        for code in ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"] {
            assert_false(
                &send(address, &request(code, &STANDARD.encode(b"data"), &serial)),
                code,
            );
        }
        assert_eq!(backend.seen().len(), 3);
        service.stop();
    }
}

#[test]
fn other_commands_and_invalid_requests_do_not_reach_the_backend() {
    let backend = TestBackend::new(Mode::Approve);
    let (mut service, address) = start(Some(backend.clone()));
    let serial = backend.sample.serial().to_owned();
    assert_false(&send(address, &json!({"CODE": "CER_002"})), "CER_002");
    let mut ntdt = request("NTDT_CER_001", &STANDARD.encode(b"data"), &serial);
    ntdt["GDT_SIGN_PATH"] = json!("");
    assert_false(&send(address, &ntdt), "NTDT_CER_001");
    // Missing required fields are rejected before any certificate is chosen.
    let mut missing = request("ONLINE_XML_CER_001", &STANDARD.encode(b"data"), &serial);
    missing.as_object_mut().unwrap().remove("PIN_CODE");
    assert_false(&send(address, &missing), "ONLINE_XML_CER_001");
    // Malformed data fails before the user is asked for a certificate or PIN.
    for values in ["%%%", ""] {
        assert_false(
            &send(address, &request("ONLINE_XML_CER_001", values, &serial)),
            "ONLINE_XML_CER_001",
        );
    }
    let many = vec![STANDARD.encode(b"x"); 101].join(",");
    assert_false(
        &send(
            address,
            &request("ONLINE_SUBMIT_SIGNING_001", &many, &serial),
        ),
        "ONLINE_SUBMIT_SIGNING_001",
    );
    assert!(backend.seen().is_empty());
    service.stop();
}

#[test]
fn without_a_backend_signing_commands_answer_false() {
    let (mut service, address) = start(None);
    let serial = SampleBackend::new().unwrap().serial().to_owned();
    for code in ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"] {
        assert_false(
            &send(address, &request(code, &STANDARD.encode(b"data"), &serial)),
            code,
        );
    }
    service.stop();
}

#[test]
fn capabilities_and_activity_summary_describe_the_bridge() {
    let capabilities = protocol::capabilities();
    assert_eq!(
        capabilities["legacy_commands_implemented"],
        json!(["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"])
    );
    assert_eq!(capabilities["real_portal_verified"], false);
    let serial = SampleBackend::new().unwrap().serial().to_owned();
    let bytes = request("cer_001", &STANDARD.encode(b"data"), &serial).to_string();
    let (action, without) = protocol::summary(bytes.as_bytes(), false);
    assert_eq!(action, "cer_001");
    assert!(without.contains("MacSigner"), "{without}");
    assert_eq!(protocol::summary(bytes.as_bytes(), true).1, "");
    let (_, other) = protocol::summary(br#"{"CODE":"CER_002"}"#, true);
    assert_eq!(other, "Lệnh chưa được hỗ trợ trong bản này");
    // Case-insensitive codes are signed and answered with the code as sent.
    let backend = TestBackend::new(Mode::Approve);
    let handled = protocol::handle_with(bytes.as_bytes(), ORIGIN, Some(backend.as_ref()));
    assert_eq!(handled.reply["SUCCESS"], "TRUE");
    assert_eq!(handled.reply["CODE"], "cer_001");
    assert!(handled.signing && handled.message.is_empty());
    assert_eq!(
        macsigner::ctsign::certificate::normalize_serial(&handled.certificate).unwrap(),
        "AB54A98CEB1F0AD2"
    );
    assert_eq!(backend.seen()[0].code, "CER_001");
}

#[test]
fn origin_checks_stay_in_front_of_the_signing_backend() {
    let backend = TestBackend::new(Mode::Approve);
    let (mut service, address) = start(Some(backend.clone()));
    let input = request(
        "ONLINE_XML_CER_001",
        &STANDARD.encode(b"data"),
        backend.sample.serial(),
    );
    for origin in [
        "https://attacker.invalid",
        "http://thuedientu.gdt.gov.vn",
        "null",
    ] {
        let error = selftest::websocket(address, origin, &input.to_string()).unwrap_err();
        assert!(error.contains("403"), "{origin}: {error}");
    }
    assert!(backend.seen().is_empty());
    service.stop();
}
