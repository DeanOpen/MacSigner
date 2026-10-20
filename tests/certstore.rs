// Windows certificate-store signing through CNG/CryptoAPI, using a PKCS#12 imported
// into a memory-only store (nothing is added to the user's certificate store).
#![cfg(windows)]

use base64::{Engine, engine::general_purpose::STANDARD};
use macsigner::{
    certstore,
    ctsign::signing::{Identity, SigningState, Source},
    esign::online::Algorithm,
    pdf,
};
use serde_json::Value;

fn identity() -> Identity {
    let pfx = include_bytes!("fixtures/ctsign/pkcs12.fixture");
    let mut certs = certstore::import_pfx_ephemeral(pfx, "test-only").unwrap();
    assert_eq!(certs.len(), 1);
    Identity::windows_store(certs.remove(0)).unwrap()
}

#[test]
fn store_key_matches_dotnet_vector_byte_for_byte() {
    let vector: Value =
        serde_json::from_str(include_str!("fixtures/ctsign/dotnet-vector.json")).unwrap();
    let id = identity();
    assert_eq!(id.source(), Source::WindowsStore);
    let digest = STANDARD.decode(vector["digest"].as_str().unwrap()).unwrap();
    let signature = id.sign_sha256(&digest).unwrap();
    assert_eq!(
        STANDARD.encode(signature),
        vector["signature"].as_str().unwrap()
    );
    for algorithm in [
        Algorithm::Sha1Rsa,
        Algorithm::Sha384Rsa,
        Algorithm::Sha512Rsa,
    ] {
        id.sign(b"ESIGN legacy bytes", algorithm).unwrap();
    }
}

#[test]
fn store_identity_signs_pdf() {
    let id = identity();
    let state = SigningState::default();
    let der = id.der().to_vec();
    state.add(vec![id]).unwrap();
    let serial = state.certificates().unwrap()[0].serinumber.clone();
    let pdf_in = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>\nendobj\nxref\n0 4\n0000000000 65535 f\r\n0000000009 00000 n\r\n0000000058 00000 n\r\n0000000115 00000 n\r\ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n186\n%%EOF\n";
    let signed = state
        .with_identity(&serial, |id| {
            pdf::sign(
                pdf_in,
                std::slice::from_ref(&der),
                &pdf::SignOptions::default(),
                |data| {
                    id.sign(data, Algorithm::Sha256Rsa)
                        .map_err(|e| e.to_string())
                },
            )
            .map_err(|e| anyhow::anyhow!(e))
        })
        .unwrap()
        .unwrap();
    let reports = pdf::verify(&signed);
    assert!(reports[0].valid(), "{:?}", reports[0]);
}

#[test]
fn personal_store_enumerates_without_prompting() {
    // Read-only: lists certificates that have a private key; never signs.
    let certs = certstore::personal().unwrap();
    for cert in &certs {
        assert!(x509_parser::parse_x509_certificate(&cert.der()).is_ok());
    }
}
