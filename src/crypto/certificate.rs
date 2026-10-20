use base64::{Engine, engine::general_purpose::STANDARD};
use rsa::{RsaPublicKey, pkcs8::DecodePublicKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::error::Error;
use x509_parser::{num_bigint::BigUint, parse_x509_certificate};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn summary(der: &[u8]) -> Result<Value> {
    let (remaining, cert) =
        parse_x509_certificate(der).map_err(|_| "invalid X.509 certificate DER")?;
    if !remaining.is_empty() {
        return Err("trailing bytes after X.509 certificate".into());
    }
    let subject = cert.subject().to_string();
    Ok(json!({
        "fingerprint_sha256": hex::encode(Sha256::digest(der)),
        "serial_decimal": BigUint::from_bytes_be(cert.raw_serial()).to_str_radix(10),
        "subject": subject,
        "issuer": cert.issuer().to_string(),
        "not_before": cert.validity().not_before.to_string(),
        "not_after": cert.validity().not_after.to_string(),
        "not_before_unix": cert.validity().not_before.timestamp(),
        "not_after_unix": cert.validity().not_after.timestamp(),
        "currently_valid": cert.validity().is_valid(),
        "signature_algorithm_oid": cert.signature_algorithm.algorithm.to_id_string(),
    }))
}

pub fn rsa_public_key(der: &[u8]) -> Result<RsaPublicKey> {
    let (remaining, cert) =
        parse_x509_certificate(der).map_err(|_| "invalid X.509 certificate DER")?;
    if !remaining.is_empty() {
        return Err("trailing bytes after X.509 certificate".into());
    }
    Ok(RsaPublicKey::from_public_key_der(cert.public_key().raw)?)
}

pub fn verify_rsa_digest(
    der: &[u8],
    digest: &[u8],
    signature: &[u8],
    algorithm: super::Algorithm,
) -> Result<()> {
    verify_rsa_digest_with_key(&rsa_public_key(der)?, digest, signature, algorithm)
}

pub fn verify_rsa_digest_with_key(
    public_key: &RsaPublicKey,
    digest: &[u8],
    signature: &[u8],
    algorithm: super::Algorithm,
) -> Result<()> {
    let padding = match algorithm {
        super::Algorithm::Sha1Rsa => rsa::Pkcs1v15Sign::new::<sha1::Sha1>(),
        super::Algorithm::Sha256Rsa => rsa::Pkcs1v15Sign::new::<sha2::Sha256>(),
        super::Algorithm::Sha384Rsa => rsa::Pkcs1v15Sign::new::<sha2::Sha384>(),
        super::Algorithm::Sha512Rsa => rsa::Pkcs1v15Sign::new::<sha2::Sha512>(),
    };
    public_key.verify(padding, digest, signature)?;
    Ok(())
}

pub fn signature_algorithm(der: &[u8]) -> Result<super::Algorithm> {
    let (remaining, cert) =
        parse_x509_certificate(der).map_err(|_| "invalid X.509 certificate DER")?;
    if !remaining.is_empty() {
        return Err("trailing bytes after X.509 certificate".into());
    }
    match cert.signature_algorithm.algorithm.to_id_string().as_str() {
        "1.2.840.113549.1.1.5" => Ok(super::Algorithm::Sha1Rsa),
        "1.2.840.113549.1.1.11" => Ok(super::Algorithm::Sha256Rsa),
        "1.2.840.113549.1.1.12" => Ok(super::Algorithm::Sha384Rsa),
        "1.2.840.113549.1.1.13" => Ok(super::Algorithm::Sha512Rsa),
        _ => Err("unsupported certificate signature algorithm".into()),
    }
}

fn der_length(len: usize) -> Result<Vec<u8>> {
    if len > 16 * 1024 * 1024 {
        return Err("certificate path exceeds 16 MiB".into());
    }
    if len < 128 {
        return Ok(vec![len as u8]);
    }
    let bytes = len.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap();
    let mut result = vec![0x80 | (bytes.len() - first) as u8];
    result.extend_from_slice(&bytes[first..]);
    Ok(result)
}

// Java CertPath.getEncoded("PkiPath") is a DER SEQUENCE of certificates in
// reverse order, from trust anchor toward leaf.
pub fn pki_path(certs_leaf_first: &[Vec<u8>]) -> Result<String> {
    if certs_leaf_first.is_empty() {
        return Err("certificate chain is empty".into());
    }
    let mut content = Vec::new();
    for cert in certs_leaf_first.iter().rev() {
        summary(cert)?;
        content.extend_from_slice(cert);
        if content.len() > 16 * 1024 * 1024 {
            return Err("certificate path exceeds 16 MiB".into());
        }
    }
    let mut der = vec![0x30];
    der.extend_from_slice(&der_length(content.len())?);
    der.extend_from_slice(&content);
    Ok(STANDARD.encode(der))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::{Pkcs1v15Sign, RsaPrivateKey, pkcs8::DecodePrivateKey};

    #[test]
    fn verifies_worker_digests_and_rejects_changed_payloads() {
        let der = include_bytes!("../../tests/fixtures/ctsign/certificate.der");
        let key = RsaPrivateKey::from_pkcs8_pem(include_str!(
            "../../tests/fixtures/ctsign/private-key.fixture"
        ))
        .unwrap();
        for algorithm in [
            super::super::Algorithm::Sha1Rsa,
            super::super::Algorithm::Sha256Rsa,
            super::super::Algorithm::Sha384Rsa,
            super::super::Algorithm::Sha512Rsa,
        ] {
            let (mut digest, padding) = match algorithm {
                super::super::Algorithm::Sha1Rsa => (
                    sha1::Sha1::digest(b"worker test").to_vec(),
                    Pkcs1v15Sign::new::<sha1::Sha1>(),
                ),
                super::super::Algorithm::Sha256Rsa => (
                    Sha256::digest(b"worker test").to_vec(),
                    Pkcs1v15Sign::new::<Sha256>(),
                ),
                super::super::Algorithm::Sha384Rsa => (
                    sha2::Sha384::digest(b"worker test").to_vec(),
                    Pkcs1v15Sign::new::<sha2::Sha384>(),
                ),
                super::super::Algorithm::Sha512Rsa => (
                    sha2::Sha512::digest(b"worker test").to_vec(),
                    Pkcs1v15Sign::new::<sha2::Sha512>(),
                ),
            };
            let signature = key.sign(padding, &digest).unwrap();
            verify_rsa_digest(der, &digest, &signature, algorithm).unwrap();
            digest[0] ^= 1;
            assert!(verify_rsa_digest(der, &digest, &signature, algorithm).is_err());
            assert!(
                verify_rsa_digest(der, &digest[..digest.len() - 1], &signature, algorithm).is_err()
            );
        }
    }
}
