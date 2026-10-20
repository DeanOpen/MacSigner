use crate::ctsign::model::CertificateInfo;
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use rsa::{RsaPublicKey, pkcs1::DecodeRsaPublicKey};
use x509_parser::prelude::*;

pub fn public_key(der: &[u8]) -> Result<RsaPublicKey> {
    let (rest, cert) = X509Certificate::from_der(der).map_err(|e| anyhow::anyhow!("X.509: {e}"))?;
    ensure!(rest.is_empty(), "Trailing certificate data");
    RsaPublicKey::from_pkcs1_der(&cert.public_key().subject_public_key.data)
        .context("Certificate must contain an RSA public key")
}

pub fn time_valid(der: &[u8]) -> Result<bool> {
    let (_, cert) = X509Certificate::from_der(der).map_err(|e| anyhow::anyhow!("X.509: {e}"))?;
    Ok(cert.validity().is_valid())
}

pub fn describe(der: &[u8]) -> Result<CertificateInfo> {
    let (_, cert) = X509Certificate::from_der(der).map_err(|e| anyhow::anyhow!("X.509: {e}"))?;
    let date = |t: ASN1Time| -> Result<String> {
        let utc =
            chrono::DateTime::<chrono::Utc>::from_timestamp(t.to_datetime().unix_timestamp(), 0)
                .context("Invalid certificate time")?;
        Ok(utc
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string())
    };
    let windows_dn = |name: &X509Name<'_>| {
        name.iter_rdn()
            .map(|rdn| X509Name::new(vec![rdn.clone()], &[]).to_string())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok(CertificateInfo {
        serinumber: hex::encode_upper(cert.raw_serial()),
        subject: windows_dn(cert.subject()),
        issuer: windows_dn(cert.issuer()),
        // Despite its name, the original ExportToPem returns plain base64 DER.
        pem: STANDARD.encode(der),
        // The original GetCert passes false to LoadCertificate and emits false.
        // Neither this field nor time_valid means chain/OCSP/CRL validation.
        valid: false,
        valid_from: date(cert.validity().not_before)?,
        valid_to: date(cert.validity().not_after)?,
        error: None,
    })
}

pub fn normalize_serial(value: &str) -> Result<String> {
    let cleaned: String = value.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    ensure!(
        !cleaned.is_empty() && cleaned.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid certificate serial number"
    );
    let normalized = cleaned.trim_start_matches('0').to_ascii_uppercase();
    Ok(if normalized.is_empty() {
        "0".to_owned()
    } else {
        normalized
    })
}
