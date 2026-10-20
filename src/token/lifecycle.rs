use crate::crypto::certificate;
use base64::{Engine, engine::general_purpose::STANDARD};
use cryptoki::context::Pkcs11;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CertificateVersion {
    pub fingerprint_sha256: String,
    pub not_before_unix: i64,
    pub not_after_unix: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidityStatus {
    NotYetValid,
    Valid,
    ExpiringSoon,
    Expired,
}

impl CertificateVersion {
    pub fn from_der(der: &[u8]) -> Result<Self> {
        let parsed = certificate::summary(der)?;
        Ok(Self {
            fingerprint_sha256: parsed["fingerprint_sha256"]
                .as_str()
                .ok_or("missing certificate fingerprint")?
                .to_owned(),
            not_before_unix: parsed["not_before_unix"]
                .as_i64()
                .ok_or("missing certificate start time")?,
            not_after_unix: parsed["not_after_unix"]
                .as_i64()
                .ok_or("missing certificate end time")?,
        })
    }

    pub fn validity(&self, now_unix: i64, warning_seconds: u32) -> ValidityStatus {
        if now_unix < self.not_before_unix {
            ValidityStatus::NotYetValid
        } else if now_unix > self.not_after_unix {
            ValidityStatus::Expired
        } else if self.not_after_unix.saturating_sub(now_unix) <= i64::from(warning_seconds) {
            ValidityStatus::ExpiringSoon
        } else {
            ValidityStatus::Valid
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct InventoryChange {
    pub added: Vec<CertificateVersion>,
    pub removed: Vec<CertificateVersion>,
}

#[derive(Default)]
pub struct CertificateInventory {
    certificates: BTreeMap<String, CertificateVersion>,
}

impl CertificateInventory {
    pub fn replace(&mut self, certificates_der: &[Vec<u8>]) -> Result<InventoryChange> {
        let mut next = BTreeMap::new();
        for der in certificates_der {
            let version = CertificateVersion::from_der(der)?;
            next.insert(version.fingerprint_sha256.clone(), version);
        }
        let change = InventoryChange {
            added: next
                .iter()
                .filter(|(fingerprint, _)| !self.certificates.contains_key(*fingerprint))
                .map(|(_, version)| version.clone())
                .collect(),
            removed: self
                .certificates
                .iter()
                .filter(|(fingerprint, _)| !next.contains_key(*fingerprint))
                .map(|(_, version)| version.clone())
                .collect(),
        };
        self.certificates = next;
        Ok(change)
    }

    pub fn refresh_pkcs11(&mut self, context: &Pkcs11, slot_id: u64) -> Result<InventoryChange> {
        let snapshot = super::pkcs11::certificates(context, slot_id)?;
        let mut certificates = Vec::new();
        for entry in snapshot.as_array().ok_or("invalid certificate snapshot")? {
            let encoded = entry["certificate_der_base64"]
                .as_str()
                .ok_or("missing certificate data")?;
            certificates.push(STANDARD.decode(encoded)?);
        }
        self.replace(&certificates)
    }
}

pub fn validate_ca_update(
    certificates_der: &[Vec<u8>],
    approved_sha256: &BTreeSet<String>,
    now_unix: i64,
) -> Result<Vec<CertificateVersion>> {
    if certificates_der.is_empty() || certificates_der.len() > 256 {
        return Err("CA update must contain between 1 and 256 certificates".into());
    }
    let mut versions = Vec::new();
    let mut fingerprints = BTreeSet::new();
    let mut total_bytes = 0usize;
    for der in certificates_der {
        total_bytes = total_bytes.saturating_add(der.len());
        if total_bytes > 16 * 1024 * 1024 {
            return Err("CA update exceeds 16 MiB".into());
        }
        let version = CertificateVersion::from_der(der)?;
        let (_, parsed) =
            x509_parser::parse_x509_certificate(der).map_err(|_| "invalid CA certificate")?;
        let constraints = parsed
            .basic_constraints()?
            .ok_or("missing CA constraints")?;
        if !constraints.value.ca {
            return Err("end-entity certificate cannot be installed as a CA".into());
        }
        if let Some(usage) = parsed.key_usage()?
            && !usage.value.key_cert_sign()
        {
            return Err("CA certificate does not permit certificate signing".into());
        }
        if matches!(
            version.validity(now_unix, 0),
            ValidityStatus::NotYetValid | ValidityStatus::Expired
        ) {
            return Err("CA certificate is outside its validity period".into());
        }
        if !approved_sha256.contains(&version.fingerprint_sha256) {
            return Err("CA certificate fingerprint has not been approved".into());
        }
        if !fingerprints.insert(version.fingerprint_sha256.clone()) {
            return Err("duplicate CA certificate in update".into());
        }
        versions.push(version);
    }
    if &fingerprints != approved_sha256 {
        return Err("CA update does not match the complete approved fingerprint set".into());
    }
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERTIFICATE: &[u8] = include_bytes!("../../tests/fixtures/ctsign/certificate.der");

    #[test]
    fn validity_boundaries_are_deterministic() {
        let version = CertificateVersion {
            fingerprint_sha256: "test".into(),
            not_before_unix: 100,
            not_after_unix: 200,
        };
        assert_eq!(version.validity(99, 10), ValidityStatus::NotYetValid);
        assert_eq!(version.validity(100, 10), ValidityStatus::Valid);
        assert_eq!(version.validity(190, 10), ValidityStatus::ExpiringSoon);
        assert_eq!(version.validity(200, 0), ValidityStatus::ExpiringSoon);
        assert_eq!(version.validity(201, 10), ValidityStatus::Expired);
    }

    #[test]
    fn inventory_is_atomic_and_deduplicates_der() {
        let mut inventory = CertificateInventory::default();
        let certs = vec![CERTIFICATE.to_vec(), CERTIFICATE.to_vec()];
        assert_eq!(inventory.replace(&certs).unwrap().added.len(), 1);
        assert!(inventory.replace(&[vec![0]]).is_err());
        let unchanged = inventory.replace(&certs).unwrap();
        assert!(unchanged.added.is_empty());
        assert!(unchanged.removed.is_empty());
        assert_eq!(inventory.replace(&[]).unwrap().removed.len(), 1);
    }

    #[test]
    fn unapproved_and_empty_ca_updates_are_rejected() {
        assert!(validate_ca_update(&[], &BTreeSet::new(), 0).is_err());
        let version = CertificateVersion::from_der(CERTIFICATE).unwrap();
        assert!(
            validate_ca_update(
                &[CERTIFICATE.to_vec()],
                &BTreeSet::new(),
                version.not_before_unix,
            )
            .is_err()
        );
        assert_eq!(version.fingerprint_sha256.len(), 64);
    }

    #[test]
    fn approved_leaf_is_not_a_trust_anchor() {
        let version = CertificateVersion::from_der(CERTIFICATE).unwrap();
        let approved = BTreeSet::from([version.fingerprint_sha256]);
        assert!(
            validate_ca_update(&[CERTIFICATE.to_vec()], &approved, version.not_before_unix,)
                .is_err()
        );
    }

    #[test]
    fn fingerprint_distinguishes_der_even_when_serial_is_unchanged() {
        let mut changed = CERTIFICATE.to_vec();
        *changed.last_mut().unwrap() ^= 1;
        let mut inventory = CertificateInventory::default();
        inventory.replace(&[CERTIFICATE.to_vec()]).unwrap();
        let change = inventory.replace(&[changed]).unwrap();
        assert_eq!(change.added.len(), 1);
        assert_eq!(change.removed.len(), 1);
        assert_ne!(
            change.added[0].fingerprint_sha256,
            change.removed[0].fingerprint_sha256,
        );
    }
}
