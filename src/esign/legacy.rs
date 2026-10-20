use crate::esign::{
    certificate,
    online::{self, Algorithm},
    protocol,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rsa::{
    pkcs1v15::{Signature, VerifyingKey},
    signature::Verifier,
};
use serde_json::{Map, Value};
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn required<'a>(input: &'a Value, field: &str) -> Result<&'a str> {
    input
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {field}").into())
}

fn verify(data: &[u8], sig: &[u8], cert_der: &[u8], algorithm: Algorithm) -> Result<()> {
    let key = certificate::rsa_public_key(cert_der)?;
    let signature = Signature::try_from(sig)?;
    match algorithm {
        Algorithm::Sha1Rsa => VerifyingKey::<sha1::Sha1>::new(key).verify(data, &signature)?,
        Algorithm::Sha256Rsa => VerifyingKey::<sha2::Sha256>::new(key).verify(data, &signature)?,
        Algorithm::Sha384Rsa => VerifyingKey::<sha2::Sha384>::new(key).verify(data, &signature)?,
        Algorithm::Sha512Rsa => VerifyingKey::<sha2::Sha512>::new(key).verify(data, &signature)?,
    }
    Ok(())
}

/// Checks validity period, and SERIAL_NUMBER / TIN_VALID when the request asks.
pub fn check_certificate(input: &Value, cert_der: &[u8]) -> Result<()> {
    let info = certificate::summary(cert_der)?;
    if info["currently_valid"] != true {
        return Err("certificate is outside its validity period".into());
    }
    if required(input, "CHECK_CERT_SERIAL")?.eq_ignore_ascii_case("true") {
        let serial = info["serial_decimal"]
            .as_str()
            .ok_or("missing certificate serial")?;
        if !required(input, "SERIAL_NUMBER")?
            .split(',')
            .any(|s| s.trim() == serial)
        {
            return Err("selected certificate serial differs from request".into());
        }
    }
    if required(input, "CHECK_TIN")?.eq_ignore_ascii_case("true") {
        let subject = info["subject"]
            .as_str()
            .ok_or("missing certificate subject")?
            .to_ascii_lowercase();
        let pos = subject
            .find("mst:")
            .ok_or("certificate subject has no MST")?;
        let actual = subject[pos + 4..]
            .split(',')
            .next()
            .unwrap_or("")
            .replace([' ', '-'], "");
        let expected = required(input, "TIN_VALID")?
            .to_ascii_lowercase()
            .replace([' ', '-'], "");
        if actual.is_empty() || actual != expected {
            return Err("certificate tax ID differs from request".into());
        }
    }
    Ok(())
}

// Implements the three legacy commands that sign already-prepared bytes. XMLDSig
// canonicalization and browser interaction remain separate obligations.
pub fn sign_request<F>(input: &Value, cert_der: &[u8], allow_sha1: bool, signer: F) -> Result<Value>
where
    F: FnOnce(&[Vec<u8>], Algorithm) -> Result<Vec<Vec<u8>>>,
{
    protocol::validate(input)?;
    let code = required(input, "CODE")?;
    let algorithm = match code.to_ascii_uppercase().as_str() {
        "CER_001" => Algorithm::Sha1Rsa,
        "ONLINE_XML_CER_001" | "ONLINE_SUBMIT_SIGNING_001" => {
            certificate::signature_algorithm(cert_der)?
        }
        _ => return Err("legacy command is not a supported prepared-byte signing command".into()),
    };
    if algorithm == Algorithm::Sha1Rsa && !allow_sha1 {
        return Err("SHA-1 compatibility requires --allow-sha1".into());
    }
    check_certificate(input, cert_der)?;
    let inputs = if code.eq_ignore_ascii_case("ONLINE_SUBMIT_SIGNING_001") {
        required(input, "BASE64_VALUE")?
            .split(',')
            .collect::<Vec<_>>()
    } else {
        vec![required(input, "BASE64_VALUE")?]
    };
    if inputs.is_empty() || inputs.len() > 100 {
        return Err("invalid signing input count".into());
    }
    let payloads = inputs
        .iter()
        .map(|value| online::decode_input(value))
        .collect::<Result<Vec<_>>>()?;
    let signatures = signer(&payloads, algorithm)?;
    if signatures.len() != payloads.len() {
        return Err("signer returned the wrong number of signatures".into());
    }
    let mut signed = Vec::with_capacity(signatures.len());
    for (bytes, signature) in payloads.iter().zip(signatures.iter()) {
        verify(bytes, signature, cert_der, algorithm)?;
        signed.push(STANDARD.encode(signature));
    }
    let chain = certificate::pki_path(&[cert_der.to_vec()])?;
    let mut payload = Map::new();
    payload.insert("SIGNED_CONTENT".into(), Value::String(signed.join(",")));
    payload.insert("CERT".into(), Value::String(STANDARD.encode(cert_der)));
    payload.insert("CERT_CHAIN".into(), Value::String(chain));
    payload.insert("PIN_CODE".into(), Value::String(String::new()));
    Ok(protocol::response(code, true, payload))
}
