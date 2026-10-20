// Browser signing bridge: routes the prepared-byte legacy commands that arrive
// over the WebSocket to `legacy::sign_request`, with a key supplied by a
// `Backend` (the desktop app, which asks the user; tests use software keys).
//
// Certificate choice: a request with CHECK_CERT_SERIAL="true" accepts only the
// serials listed in SERIAL_NUMBER, and one with CHECK_TIN="true" only
// certificates whose MST equals TIN_VALID. The backend offers the user the
// certificates that pass and the user chooses; `legacy::sign_request` checks
// the chosen certificate again before and after signing.
//
// PIN: the user types it in the app's prompt. A non-empty PIN_CODE in the
// request is used only when the user leaves the prompt empty. The PIN is
// never logged and never returned (the reply's PIN_CODE is always empty).
//
// SHA-1 policy: CER_001 is SHA1withRSA in the original plugin, and the portal
// verifies it as such, so SHA-1 is allowed for CER_001 only. The ONLINE_*
// commands sign with the certificate's own algorithm and refuse SHA-1.
use crate::esign::{
    legacy,
    online::{self, Algorithm},
};
use serde_json::{Value, json};
use std::error::Error;
pub use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Commands this bridge signs; every other code keeps answering FALSE.
pub const SIGNING_COMMANDS: [&str; 3] =
    ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"];

pub fn is_signing_command(code: &str) -> bool {
    SIGNING_COMMANDS
        .iter()
        .any(|c| c.eq_ignore_ascii_case(code))
}

/// What a browser signing request asks for, without the data to sign.
pub struct KeyRequest {
    /// Upper-case command code.
    pub code: String,
    /// Origin of the page that sent the request.
    pub origin: String,
    /// Decimal serials the request accepts; empty when it accepts any.
    pub serials: Vec<String>,
    /// Tax ID (MST) the certificate must carry, when the request checks it.
    pub tin: Option<String>,
    /// Number of values to sign.
    pub count: usize,
    /// PIN_CODE from the request, when non-empty.
    pub pin: Option<Zeroizing<String>>,
    constraints: Value,
}

impl KeyRequest {
    pub fn from_input(input: &Value, origin: &str) -> Self {
        let text = |field: &str| input.get(field).and_then(Value::as_str).unwrap_or("");
        let checked = |field: &str| text(field).eq_ignore_ascii_case("true");
        let code = text("CODE").to_ascii_uppercase();
        Self {
            count: if code == "ONLINE_SUBMIT_SIGNING_001" {
                text("BASE64_VALUE").split(',').count()
            } else {
                1
            },
            code,
            origin: origin.to_owned(),
            serials: if checked("CHECK_CERT_SERIAL") {
                text("SERIAL_NUMBER")
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            } else {
                Vec::new()
            },
            tin: checked("CHECK_TIN").then(|| text("TIN_VALID").to_owned()),
            pin: Some(text("PIN_CODE"))
                .filter(|p| !p.is_empty())
                .map(|p| Zeroizing::new(p.to_owned())),
            constraints: json!({
                "CHECK_CERT_SERIAL": text("CHECK_CERT_SERIAL"),
                "SERIAL_NUMBER": text("SERIAL_NUMBER"),
                "CHECK_TIN": text("CHECK_TIN"),
                "TIN_VALID": text("TIN_VALID"),
            }),
        }
    }

    /// Whether the certificate is currently valid and matches the request's
    /// serial and tax ID checks: the certificates to offer the user.
    pub fn accepts(&self, cert_der: &[u8]) -> bool {
        legacy::check_certificate(&self.constraints, cert_der).is_ok()
    }

    /// True when signing will use SHA-1 (CER_001), so the prompt can say so.
    pub fn uses_sha1(&self) -> bool {
        self.code == "CER_001"
    }
}

/// A private key and its certificate.
pub trait Key {
    fn certificate(&self) -> &[u8];
    fn sign(&self, data: &[u8], algorithm: Algorithm) -> Result<Vec<u8>>;
}

impl Key for crate::ctsign::signing::Identity {
    fn certificate(&self) -> &[u8] {
        self.der()
    }

    fn sign(&self, data: &[u8], algorithm: Algorithm) -> Result<Vec<u8>> {
        crate::ctsign::signing::Identity::sign(self, data, algorithm)
            .map_err(|e| e.to_string().into())
    }
}

/// Supplies the key for a browser signing request.
///
/// An implementation must ask the user to approve every request (choosing among
/// the certificates `KeyRequest::accepts`, and entering the PIN when the key
/// needs one), must not keep the PIN after the call, and returns an error when
/// the user declines. The reply sent to the browser is built only from what
/// `sign` produced; returning `Ok` without calling it is reported as a failure.
pub trait Backend: Send + Sync {
    fn with_key(
        &self,
        request: &KeyRequest,
        sign: &mut dyn FnMut(&dyn Key) -> Result<()>,
    ) -> Result<()>;
}

/// A completed signature: the reply for the browser and the certificate used.
pub struct Signed {
    pub reply: Value,
    pub certificate_der: Vec<u8>,
}

/// Signs one validated prepared-byte request through `backend`.
pub fn sign(input: &Value, origin: &str, backend: &dyn Backend) -> Result<Signed> {
    let request = KeyRequest::from_input(input, origin);
    if !is_signing_command(&request.code) {
        return Err("not a prepared-byte signing command".into());
    }
    // Reject malformed data before the user is asked for a certificate or PIN.
    let values = input
        .get("BASE64_VALUE")
        .and_then(Value::as_str)
        .unwrap_or("");
    let values: Vec<&str> = if request.code == "ONLINE_SUBMIT_SIGNING_001" {
        values.split(',').collect()
    } else {
        vec![values]
    };
    if values.len() > 100 {
        return Err("invalid signing input count".into());
    }
    for value in values {
        online::decode_input(value)?;
    }
    let allow_sha1 = request.uses_sha1();
    let mut done: Option<Signed> = None;
    backend.with_key(&request, &mut |key| {
        let der = key.certificate();
        let reply = legacy::sign_request(input, der, allow_sha1, |payloads, algorithm| {
            payloads.iter().map(|p| key.sign(p, algorithm)).collect()
        })?;
        done = Some(Signed {
            reply,
            certificate_der: der.to_vec(),
        });
        Ok(())
    })?;
    let signed = done.ok_or("signing backend returned without signing")?;
    if signed.reply["SUCCESS"] != "TRUE" {
        return Err("signing did not complete".into());
    }
    Ok(signed)
}

/// Hex serial and subject of a certificate as the app shows them, for the
/// activity log.
pub fn describe(cert_der: &[u8]) -> (String, String) {
    crate::ctsign::certificate::describe(cert_der)
        .map(|info| (info.serinumber, info.subject))
        .unwrap_or_default()
}
