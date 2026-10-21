// Start/stop both services on free ports and run the same self-test the UI uses.
use macsigner::{
    config::{CtsignSettings, EsignSettings, Settings},
    ctsign::signing::SigningState,
    selftest::{self, Outcome},
    services::{CtsignService, EsignService, Status},
};

fn assert_all_pass(checks: &[selftest::Check]) {
    for check in checks {
        assert_ne!(
            check.outcome,
            Outcome::Fail,
            "{} {}: {}",
            check.module,
            check.name,
            check.detail
        );
    }
}

#[test]
fn esign_service_starts_answers_and_stops() {
    let mut service = EsignService::default();
    let settings = EsignSettings {
        port: 0,
        fallback_port: None,
        ..Default::default()
    };
    let address = service.start(&settings).unwrap();
    assert_eq!(service.status(), Status::Running(address));
    let checks = selftest::esign(&settings, Some(address));
    assert!(checks.iter().filter(|c| c.outcome == Outcome::Pass).count() >= 4);
    assert_all_pass(&checks);
    service.stop();
    assert_eq!(service.status(), Status::Stopped);
    assert!(selftest::websocket(address, &settings.origins[0], "{}").is_err());
}

#[test]
fn esign_service_reports_occupied_port() {
    let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let mut service = EsignService::default();
    let settings = EsignSettings {
        port,
        fallback_port: None,
        ..Default::default()
    };
    assert!(service.start(&settings).is_err());
}

#[test]
fn ctsign_service_starts_answers_and_stops() {
    let mut service = CtsignService::new(SigningState::default()).unwrap();
    let settings = CtsignSettings {
        port: 0,
        ..Default::default()
    };
    let address = service.start(&settings).unwrap();
    assert!(service.status().is_running());
    let checks = selftest::ctsign(&service.state, Some(address));
    assert!(checks.iter().filter(|c| c.outcome == Outcome::Pass).count() >= 6);
    assert_all_pass(&checks);
    service.stop();
    assert_eq!(service.status(), Status::Stopped);
    assert!(selftest::http(address, "GET", "/api/Info/GetInfoCert", None).is_err());
}

#[test]
fn selftest_without_running_services_uses_temporary_instances() {
    let settings = Settings::default();
    assert_all_pass(&selftest::esign(&settings.esign, None));
    assert_all_pass(&selftest::ctsign(&SigningState::default(), None));
}

#[test]
fn settings_round_trip_and_validation() {
    let dir = std::env::temp_dir().join(format!("macsigner-test-{}", std::process::id()));
    let path = dir.join("settings.json");
    let mut settings = Settings::default();
    assert_eq!(Settings::load(&path).unwrap(), settings);
    settings.esign.enabled = true;
    settings
        .esign
        .origins
        .push("chrome-extension://abcdef".into());
    settings.ctsign.port = 32101;
    settings.save(&path).unwrap();
    assert_eq!(Settings::load(&path).unwrap(), settings);

    let mut bad = settings.clone();
    bad.esign.origins = vec!["http://thuedientu.gdt.gov.vn".into()];
    assert!(bad.save(&path).is_err());
    let mut bad = settings.clone();
    bad.ctsign.port = bad.esign.port;
    assert!(bad.validate().is_err());
    // Partial files fall back to defaults for missing fields.
    std::fs::write(&path, r#"{"ctsign":{"enabled":true}}"#).unwrap();
    let partial = Settings::load(&path).unwrap();
    assert!(partial.ctsign.enabled);
    assert_eq!(partial.ctsign.port, 32001);
    assert_eq!(partial.esign, EsignSettings::default());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn vnpt_settings_are_backward_compatible_but_cannot_enable_an_unimplemented_service() {
    let settings: Settings = serde_json::from_str(r#"{"ctsign":{"port":32101}}"#).unwrap();
    assert_eq!(settings.vnpt.port, 4433);
    assert!(!settings.vnpt.enabled);
    assert!(settings.validate().is_ok());
    let mut invalid = settings.clone();
    invalid.vnpt.port = settings.esign.port;
    assert!(invalid.validate().is_err());
    invalid.vnpt.port = settings.esign.fallback_port.unwrap();
    assert!(invalid.validate().is_err());
    invalid.vnpt.port = settings.ctsign.port;
    assert!(invalid.validate().is_err());
    invalid.vnpt.port = 0;
    assert!(invalid.validate().is_err());
    invalid.vnpt.port = 4433;
    invalid.vnpt.enabled = true;
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .contains("VNPT chưa sẵn sàng")
    );
}

#[test]
fn vnpt_selftest_does_not_claim_signing_or_transport_success() {
    let checks = selftest::vnpt();
    assert_eq!(checks.len(), 4);
    assert_eq!(checks[0].outcome, Outcome::Pass);
    assert!(
        checks
            .iter()
            .skip(1)
            .all(|check| check.outcome == Outcome::Skip)
    );
}

#[test]
fn sample_signing_works_for_each_module() {
    let esign = selftest::esign_sample_sign("Ký thử".as_bytes(), "ONLINE_XML_CER_001").unwrap();
    assert_eq!(esign["SUCCESS"], "TRUE");
    assert!(!esign["SIGNED_CONTENT"].as_str().unwrap().is_empty());

    let state = selftest::ctsign_sample_state().unwrap();
    let serial = state.certificates().unwrap()[0].serinumber.clone();
    let ctsign = selftest::ctsign_sample_sign(&state, &serial, "Ký thử".as_bytes());
    assert_eq!(ctsign["status"], 200);
    assert!(!ctsign["data"][0]["CKy"].as_str().unwrap().is_empty());
    assert_eq!(
        selftest::ctsign_sample_sign(&state, "FFFF", b"x")["status"],
        404
    );
}
