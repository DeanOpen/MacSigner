use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::BodyExt;
use macsigner::ctsign::{
    api, certificate,
    model::{HashInfo, Response, SignHashRequest},
    signing::{self, Identity, SigningState},
};
use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey};
use serde_json::{Value, json};
use std::{path::Path, sync::atomic::Ordering};
use tower::ServiceExt;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/ctsign/dotnet-vector.json")).unwrap()
}
fn state() -> SigningState {
    let state = SigningState::default();
    state
        .add(vec![
            Identity::software(
                include_bytes!("fixtures/ctsign/certificate.der").to_vec(),
                RsaPrivateKey::from_pkcs8_pem(include_str!("fixtures/ctsign/private-key.fixture"))
                    .unwrap(),
            )
            .unwrap(),
        ])
        .unwrap();
    state
}
fn request(digest: &str) -> SignHashRequest {
    serde_json::from_value(json!({ "requestId": "test", "serinumber": fixture()["serial"], "hash": [{"Ma": "one", "DLieuHash": digest}] })).unwrap()
}

#[test]
fn matches_independent_dotnet_signature_byte_for_byte() {
    let vector = fixture();
    let response = state().sign_hash(request(vector["digest"].as_str().unwrap()));
    assert_eq!(response.status, 200);
    let signed = response.data.unwrap();
    assert_eq!(signed[0].signature.as_deref(), vector["signature"].as_str());
    assert_eq!(signed[0].ma.as_deref(), Some("one"));
    assert_eq!(signed[0].digest, vector["digest"]);
}

#[test]
fn imports_dotnet_pkcs12_and_rejects_wrong_password() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ctsign/pkcs12.fixture");
    assert!(signing::load_pkcs12(&path, "wrong-password").is_err());
    let loaded = signing::load_pkcs12(&path, "test-only").unwrap();
    let state = SigningState::default();
    state.add(loaded).unwrap();
    assert_eq!(
        state
            .sign_hash(request(fixture()["digest"].as_str().unwrap()))
            .data
            .unwrap()[0]
            .signature
            .as_deref(),
        fixture()["signature"].as_str()
    );
}

#[test]
fn imports_pem_and_rejects_mismatched_key() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ctsign");
    assert_eq!(
        signing::load_pem(
            &root.join("certificate.der"),
            &root.join("private-key.fixture")
        )
        .unwrap()
        .len(),
        1
    );
    let other_key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
    assert!(
        Identity::software(
            include_bytes!("fixtures/ctsign/certificate.der").to_vec(),
            other_key
        )
        .is_err()
    );
}

#[test]
fn rejects_malformed_digest_batches_and_preserves_missing_certificate_error() {
    let state = state();
    for digest in ["!not-base64", "", "YWJj"] {
        let response = state.sign_hash(request(digest));
        assert_eq!(response.status, 500);
        assert!(response.data.is_none());
    }
    let mut batch = request(fixture()["digest"].as_str().unwrap());
    batch.hash.push(HashInfo {
        ma: None,
        digest: "bad".into(),
        signature: None,
    });
    assert!(state.sign_hash(batch).data.is_none());
    let mut missing = request("invalid");
    missing.serinumber = "FFFF".into();
    let result = state.sign_hash(missing);
    assert_eq!(result.status, 404);
    assert_eq!(
        result.message.as_deref(),
        Some("Không tìm thấy CTS có serinumber FFFF")
    );
}

#[test]
fn serial_comparison_and_identity_replacement_are_consistent() {
    assert_eq!(certificate::normalize_serial(" 00ab cd ").unwrap(), "ABCD");
    assert!(certificate::normalize_serial("x").is_err());
    let state = state();
    let loaded = signing::load_pem(
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/ctsign/certificate.der"
        )),
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/ctsign/private-key.fixture"
        )),
    )
    .unwrap();
    assert_eq!(state.add(loaded).unwrap(), 1);
    state.api_signing_enabled.store(true, Ordering::SeqCst);
    state.clear().unwrap();
    assert!(state.certificates().unwrap().is_empty());
    assert!(!state.api_signing_enabled.load(Ordering::SeqCst));
}

async fn call(state: SigningState, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = api::router(state)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn original_probe_and_empty_store_contracts() {
    for method in ["GET", "POST"] {
        assert_eq!(
            call(
                SigningState::default(),
                method,
                "/API/Info/GetInfoCert",
                Value::Null
            )
            .await,
            (StatusCode::OK, json!("1"))
        );
        assert_eq!(
            call(
                SigningState::default(),
                method,
                "/api/SignHash/Version/id",
                Value::Null
            )
            .await
            .1,
            json!("3333333333333333")
        );
        assert_eq!(
            call(
                SigningState::default(),
                method,
                "/api/HubClient/GetCert",
                Value::Null
            )
            .await
            .1,
            json!({"status":0,"data":null,"message":null})
        );
    }
}

#[tokio::test]
async fn signhash_http_contract_and_explicit_api_enable() {
    let state = state();
    let vector = fixture();
    let body =
        json!({"serinumber":vector["serial"],"hash":[{"Ma":"one","DLieuHash":vector["digest"]}]});
    assert_eq!(
        call(
            state.clone(),
            "POST",
            "/api/HubClient/SignHash",
            body.clone()
        )
        .await
        .1["status"],
        403
    );
    state.api_signing_enabled.store(true, Ordering::SeqCst);
    let (status, response) = call(state, "POST", "/api/HubClient/SignHash", body).await;
    assert_eq!(status, StatusCode::OK);
    let response: Response<Vec<HashInfo>> = serde_json::from_value(response).unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(
        response.data.unwrap()[0].signature.as_deref(),
        vector["signature"].as_str()
    );
}

#[tokio::test]
async fn certificate_wire_shape_and_unimplemented_operations_are_honest() {
    let (_, response) = call(state(), "GET", "/api/HubClient/GetCert", Value::Null).await;
    let cert = &response["data"][0];
    for name in [
        "serinumber",
        "subjectDN",
        "issuedDN",
        "pemCert",
        "valid",
        "validFrom",
        "validTo",
        "error",
    ] {
        assert!(cert.get(name).is_some(), "missing {name}");
    }
    assert_eq!(cert["valid"], false);
    assert_eq!(
        cert["subjectDN"],
        "CN=CTSign TEST ONLY, O=Local Interoperability Fixture"
    );
    assert_eq!(
        cert["issuedDN"],
        "CN=CTSign TEST ONLY, O=Local Interoperability Fixture"
    );
    assert_eq!(
        STANDARD.decode(cert["pemCert"].as_str().unwrap()).unwrap(),
        include_bytes!("fixtures/ctsign/certificate.der")
    );
    for action in ["signxml", "signooxml"] {
        let (_, response) = call(
            state(),
            "POST",
            &format!("/api/HubClient/{action}"),
            json!({}),
        )
        .await;
        assert_eq!(response["status"], 501);
        assert!(response["data"].is_null());
    }
    assert!(
        !STANDARD
            .decode(fixture()["signature"].as_str().unwrap())
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn live_original_probe_headers_and_cors_match() {
    let probe = api::router(SigningState::default())
        .oneshot(
            Request::builder()
                .uri("/api/Info/GetInfoCert")
                .header(header::ORIGIN, "https://comparison.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(probe.status(), StatusCode::OK);
    assert_eq!(
        probe.headers()[header::CONTENT_TYPE],
        "application/json; charset=utf-8"
    );
    assert_eq!(probe.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");

    let missing = api::router(SigningState::default())
        .oneshot(
            Request::builder()
                .uri("/api/Info/GetInfoCert/a/b")
                .header(header::ORIGIN, "https://comparison.invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert!(
        missing
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}

#[tokio::test]
async fn signpdf_follows_original_contract() {
    let pdf_in = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>\nendobj\nxref\n0 4\n0000000000 65535 f\r\n0000000009 00000 n\r\n0000000058 00000 n\r\n0000000115 00000 n\r\ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n186\n%%EOF\n";
    let serial = fixture()["serial"].clone();
    let body = json!({
        "serinumber": serial,
        "dataFile": STANDARD.encode(pdf_in),
        "sig": [{"type": 5, "page": 1}, {"type": 2, "page": 1, "x": 300, "y": 40, "w": 220, "h": 60, "info": "Ghi chú: đã duyệt"}]
    });
    let state = state();
    assert_eq!(
        call(
            state.clone(),
            "POST",
            "/api/HubClient/signpdf",
            body.clone()
        )
        .await
        .1["status"],
        403
    );
    state.api_signing_enabled.store(true, Ordering::SeqCst);
    let mut unknown = body.clone();
    unknown["serinumber"] = json!("ABCDEF0123");
    let (_, missing) = call(state.clone(), "POST", "/api/HubClient/signpdf", unknown).await;
    assert_eq!(missing["status"], 301);
    let (_, response) = call(state, "POST", "/api/HubClient/signpdf", body).await;
    assert_eq!(response["status"], 200, "{response}");
    assert_eq!(response["message"], "Ký pdf thành công");
    let signed = STANDARD.decode(response["data"].as_str().unwrap()).unwrap();
    let reports = macsigner::pdf::verify(&signed);
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|r| r.valid()));
}
