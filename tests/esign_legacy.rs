use base64::{Engine, engine::general_purpose::STANDARD};
use macsigner::esign::{legacy, online::Algorithm};
use rsa::{
    RsaPrivateKey,
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};
use serde_json::{Value, json};

fn fixture() -> (Vec<u8>, RsaPrivateKey, String) {
    let data: Value = serde_json::from_str(include_str!(
        "../src/esign/data/legacy-writer-fixtures.json"
    ))
    .unwrap();
    let cert = STANDARD
        .decode(data["certificate"]["der_base64"].as_str().unwrap())
        .unwrap();
    let private = RsaPrivateKey::from_pkcs8_der(
        &STANDARD
            .decode(
                data["signatures"][0]["private_pkcs8_base64"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    let serial = data["certificate"]["serial_decimal"]
        .as_str()
        .unwrap()
        .to_string();
    (cert, private, serial)
}

fn request(code: &str, serial: &str, values: &str) -> Value {
    json!({
        "CODE":code, "BASE64_VALUE":values, "SERIAL_NUMBER":serial,
        "CHECK_CERT_SERIAL":"true", "CHECK_TIN":"false", "TIN_VALID":"",
        "DATE_SIGNING":"", "TIMEZONE_SIGNING":"",
        "PIN_CODE":"", "PKCS_11_LIBRARY_NAME_LIST":""
    })
}

fn sign(
    private: &RsaPrivateKey,
    inputs: &[Vec<u8>],
    algorithm: Algorithm,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    Ok(inputs
        .iter()
        .map(|input| match algorithm {
            Algorithm::Sha1Rsa => SigningKey::<sha1::Sha1>::new(private.clone())
                .sign(input)
                .to_vec(),
            Algorithm::Sha256Rsa => SigningKey::<sha2::Sha256>::new(private.clone())
                .sign(input)
                .to_vec(),
            Algorithm::Sha384Rsa => SigningKey::<sha2::Sha384>::new(private.clone())
                .sign(input)
                .to_vec(),
            Algorithm::Sha512Rsa => SigningKey::<sha2::Sha512>::new(private.clone())
                .sign(input)
                .to_vec(),
        })
        .collect())
}

#[test]
fn prepared_byte_commands_sign_and_verify_with_selected_certificate() {
    let (cert, private, serial) = fixture();
    let one = STANDARD.encode(b"canonical SignedInfo");
    for code in ["CER_001", "ONLINE_XML_CER_001", "ONLINE_SUBMIT_SIGNING_001"] {
        let values = if code == "ONLINE_SUBMIT_SIGNING_001" {
            format!("{one},{}", STANDARD.encode(b"second"))
        } else {
            one.clone()
        };
        let response = legacy::sign_request(
            &request(code, &serial, &values),
            &cert,
            true,
            |inputs, algorithm| sign(&private, inputs, algorithm),
        )
        .unwrap();
        assert_eq!(response["SUCCESS"], "TRUE");
        assert_eq!(response["CODE"], code);
        assert_eq!(response["CERT"], STANDARD.encode(&cert));
        assert_eq!(response["PIN_CODE"], "");
        assert_eq!(
            response["SIGNED_CONTENT"]
                .as_str()
                .unwrap()
                .split(',')
                .count(),
            if code == "ONLINE_SUBMIT_SIGNING_001" {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn prepared_byte_signatures_match_the_original_java_helpers() {
    let data: Value = serde_json::from_str(include_str!(
        "../src/esign/data/legacy-writer-fixtures.json"
    ))
    .unwrap();
    let (cert, private, serial) = fixture();
    for (case, code) in data["signatures"]
        .as_array()
        .unwrap()
        .iter()
        .zip(["CER_001", "ONLINE_XML_CER_001"])
    {
        let input = request(code, &serial, case["base64_input"].as_str().unwrap());
        let response = legacy::sign_request(&input, &cert, true, |inputs, algorithm| {
            sign(&private, inputs, algorithm)
        })
        .unwrap();
        assert_eq!(response["SIGNED_CONTENT"], case["signature_base64"]);
    }
}

#[test]
fn prepared_byte_signing_accepts_matching_tax_id() {
    let (cert, private, serial) = fixture();
    let mut input = request("ONLINE_XML_CER_001", &serial, &STANDARD.encode(b"data"));
    input["CHECK_TIN"] = json!("true");
    input["TIN_VALID"] = json!("1234567890");
    let response = legacy::sign_request(&input, &cert, false, |inputs, alg| {
        sign(&private, inputs, alg)
    })
    .unwrap();
    assert_eq!(response["SUCCESS"], "TRUE");
}

#[test]
fn ntdt_registration_returns_signed_xml_and_keeps_the_pin_out() {
    let (cert, private, serial) = fixture();
    let xml = STANDARD.encode(
        br#"<?xml version="1.0" encoding="UTF-8"?><DKY_DICHVU><TTIN_DANGKY><MST>1234567890</MST><TEN>Cong ty thu</TEN></TTIN_DANGKY></DKY_DICHVU>"#,
    );
    let mut input = request("NTDT_CER_002", &serial, &format!("@{xml}"));
    input["NTDT_SIGN_CODE"] = json!("01");
    let response = macsigner::esign::ntdt::sign_request(
        &input,
        &cert,
        "05112026143000",
        |payload, algorithm| Ok(sign(&private, &[payload.to_vec()], algorithm)?.remove(0)),
    )
    .unwrap();
    assert_eq!(response["SUCCESS"], "TRUE");
    assert_eq!(response["CODE"], "NTDT_CER_002");
    assert!(response.get("PIN_CODE").is_none());
    assert!(response.get("CERT").is_none());
    let signed = response["SIGNED_CONTENT"].as_str().unwrap();
    assert!(signed.starts_with('@'), "{signed}");
    let document =
        String::from_utf8(STANDARD.decode(signed.trim_start_matches('@')).unwrap()).unwrap();
    let document = document.as_str();
    assert!(document.contains("<DateTimeStamp>05112026143000</DateTimeStamp>"));
    assert!(document.contains("Filter=\"intersect\">//TTIN_DANGKY</XPath>"));
    assert!(document.contains("http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"));
    assert!(document.contains("<MST>1234567890</MST>"));
    // The business element stays outside the signature element.
    let business = document.split("<Signature ").next().unwrap();
    assert!(business.contains("<TTIN_DANGKY>"));
}

#[test]
fn ntdt_variants_join_documents_the_way_the_plugin_does() {
    let (cert, private, serial) = fixture();
    let bank =
        STANDARD.encode(br#"<DKY_NTHUE_NH><DKY_NTHUE><MST>1</MST></DKY_NTHUE></DKY_NTHUE_NH>"#);
    let gdt =
        STANDARD.encode(br#"<DKY_DICHVU><TTIN_DANGKY><MST>1</MST></TTIN_DANGKY></DKY_DICHVU>"#);
    let stop =
        STANDARD.encode(br#"<TBAO_NGUNG><TBAO_NGUNG_NT><MST>1</MST></TBAO_NGUNG_NT></TBAO_NGUNG>"#);
    let sign_one = |input: &Value| {
        macsigner::esign::ntdt::sign_request(
            input,
            &cert,
            "05112026143000",
            |payload, algorithm| Ok(sign(&private, &[payload.to_vec()], algorithm)?.remove(0)),
        )
        .unwrap()
    };
    let mut register = request("NTDT_CER_002", &serial, &format!("{bank}@{gdt}"));
    register["NTDT_SIGN_CODE"] = json!("01");
    let joined = sign_one(&register)["SIGNED_CONTENT"]
        .as_str()
        .unwrap()
        .to_owned();
    let (banks, gdt_signed) = joined.split_once('@').unwrap();
    assert_eq!(banks.split(',').count(), 1);
    assert!(!gdt_signed.is_empty());

    let mut add = request("NTDT_CER_002", &serial, &format!("{bank},{bank}"));
    add["NTDT_SIGN_CODE"] = json!("02");
    assert_eq!(
        sign_one(&add)["SIGNED_CONTENT"]
            .as_str()
            .unwrap()
            .split(',')
            .count(),
        2
    );

    let mut stopped = request("NTDT_CER_002", &serial, &stop);
    stopped["NTDT_SIGN_CODE"] = json!("03");
    let decoded = String::from_utf8(
        STANDARD
            .decode(sign_one(&stopped)["SIGNED_CONTENT"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(decoded.contains("//TBAO_NGUNG_NT"));

    let mut multiple = request("NTDT_CER_002", &serial, &gdt);
    multiple["NTDT_SIGN_CODE"] = json!("05");
    multiple["GDT_SIGN_PATH"] = json!("/DKY_DICHVU/TTIN_DANGKY");
    multiple["BANK_SIGN_PATH"] = json!("/DKY_NTHUE_NH/DKY_NTHUE");
    let body = sign_one(&multiple)["SIGNED_CONTENT"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(body.starts_with('@'));

    let mut gnt = request(
        "NTDT_CER_001",
        &serial,
        &STANDARD.encode(br#"<CHUNGTU><NDUNG_CTU_NH><NDUNG_CTU><MST>1</MST></NDUNG_CTU></NDUNG_CTU_NH></CHUNGTU>"#),
    );
    gnt["GDT_SIGN_PATH"] = json!("/CHUNGTU/NDUNG_CTU_NH/NDUNG_CTU");
    let gnt_reply = sign_one(&gnt);
    assert_eq!(gnt_reply["PIN_CODE"], "");
    assert_eq!(gnt_reply["CERT"], "");
    assert_eq!(gnt_reply["CERT_CHAIN"], "");
    let gnt_xml = String::from_utf8(
        STANDARD
            .decode(gnt_reply["SIGNED_CONTENT"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(gnt_xml.contains("//NDUNG_CTU</XPath>"));
}

#[test]
fn ntdt_rejects_unknown_codes_and_missing_nodes_without_a_signature() {
    let (cert, private, serial) = fixture();
    let xml =
        STANDARD.encode(br#"<DKY_DICHVU><TTIN_DANGKY><MST>1</MST></TTIN_DANGKY></DKY_DICHVU>"#);
    let mut input = request("NTDT_CER_002", &serial, &format!("@{xml}"));
    input["NTDT_SIGN_CODE"] = json!("99");
    let attempt = |input: &Value| {
        macsigner::esign::ntdt::sign_request(
            input,
            &cert,
            "05112026143000",
            |payload, algorithm| Ok(sign(&private, &[payload.to_vec()], algorithm)?.remove(0)),
        )
    };
    assert!(attempt(&input).is_err());
    input["NTDT_SIGN_CODE"] = json!("01");
    input["BASE64_VALUE"] = json!(xml);
    assert!(attempt(&input).is_err());
}

#[test]
fn prepared_byte_signing_rejects_policy_and_signature_failures() {
    let (cert, private, serial) = fixture();
    let input = request("CER_001", &serial, &STANDARD.encode(b"data"));
    assert!(
        legacy::sign_request(&input, &cert, false, |inputs, alg| sign(
            &private, inputs, alg
        ))
        .is_err()
    );
    let mut bad_serial = input.clone();
    bad_serial["SERIAL_NUMBER"] = json!("999");
    assert!(
        legacy::sign_request(&bad_serial, &cert, true, |inputs, alg| sign(
            &private, inputs, alg
        ))
        .is_err()
    );
    let mut bad_tin = input.clone();
    bad_tin["CHECK_TIN"] = json!("true");
    bad_tin["TIN_VALID"] = json!("not-the-tax-id");
    assert!(
        legacy::sign_request(&bad_tin, &cert, true, |inputs, alg| sign(
            &private, inputs, alg
        ))
        .is_err()
    );
    assert!(
        legacy::sign_request(&input, &cert, true, |_inputs, _alg| Ok(vec![vec![0; 256]])).is_err()
    );
    assert!(legacy::sign_request(&input, &cert, true, |_inputs, _alg| Ok(vec![])).is_err());
}
