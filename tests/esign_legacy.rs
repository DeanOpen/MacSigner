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
