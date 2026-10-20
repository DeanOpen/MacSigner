use base64::{Engine, engine::general_purpose::STANDARD};
use macsigner::esign::{certificate, native, online, pkcs11, protocol};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    signature::{SignatureEncoding, Signer, Verifier},
};
use serde_json::{Map, Value, json};
use std::io::{self, Cursor, Read};

#[test]
fn responses_match_original_java_writer() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../src/esign/data/legacy-writer-fixtures.json"
    ))
    .unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let response = protocol::response(
            case["code"].as_str().unwrap(),
            case["success"].as_bool().unwrap(),
            case["payload"].as_object().unwrap().clone(),
        );
        assert_eq!(response, case["expected"], "{}", case["name"]);
        let original = STANDARD
            .decode(case["frame_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(original[..4].try_into().unwrap()) as usize,
            original.len() - 4
        );
        let decoded = native::read_frame(&mut Cursor::new(original))
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&decoded).unwrap(), response);
        let mut frame = vec![];
        native::write_frame(&mut frame, &serde_json::to_vec(&response).unwrap()).unwrap();
        // JSON object order is insignificant; compare the UTF-8 frame contents as JSON.
        let decoded = native::read_frame(&mut Cursor::new(frame))
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&decoded).unwrap(),
            case["expected"]
        );
    }
}

// Simulates pipes returning one byte per read; read() is never guaranteed to fill a frame.
struct Fragmented(Cursor<Vec<u8>>);
impl Read for Fragmented {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(1);
        self.0.read(&mut buf[..len])
    }
}

#[test]
fn native_stream_handles_fragmentation_unicode_and_multiple_messages() {
    let requests = [json!({"CODE":"CER_002"}), json!({"CODE":"ký số"})];
    let mut input = vec![];
    for request in &requests {
        native::write_frame(&mut input, &serde_json::to_vec(request).unwrap()).unwrap();
    }
    let mut output = vec![];
    native::serve(&mut Fragmented(Cursor::new(input)), &mut output).unwrap();
    let mut reader = Cursor::new(output);
    for request in requests {
        let bytes = native::read_frame(&mut reader).unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"CODE":request["CODE"], "SUCCESS":"FALSE"})
        );
    }
    assert!(native::read_frame(&mut reader).unwrap().is_none());
}

#[test]
fn malformed_native_frames_fail_without_unbounded_allocation() {
    for bytes in [
        vec![1],
        vec![2, 0, 0],
        vec![3, 0, 0, 0, b'{'],
        vec![0, 0, 0, 0],
        vec![255; 4],
        ((native::MAX_MESSAGE + 1) as u32).to_le_bytes().to_vec(),
    ] {
        assert!(native::read_frame(&mut Cursor::new(bytes)).is_err());
    }
    assert!(
        native::read_frame(&mut Cursor::new(vec![]))
            .unwrap()
            .is_none()
    );
    assert!(native::write_frame(&mut vec![], &[]).is_err());
    assert!(native::write_frame(&mut vec![], &vec![0; native::MAX_MESSAGE + 1]).is_err());
}

fn minimal(command: &protocol::Command) -> Value {
    let mut input = Map::new();
    input.insert(
        "CODE".into(),
        Value::String(command.code.to_ascii_lowercase()),
    );
    for name in &command.required_strings {
        input.insert(name.clone(), Value::String(String::new()));
    }
    if let Some(sub) = command.subcommands.first() {
        input.insert("SUB_CODE".into(), Value::String(sub.code.clone()));
        for field in &sub.required_strings {
            input.insert(field.clone(), Value::String(String::new()));
        }
    }
    Value::Object(input)
}

#[test]
fn all_observed_commands_validate_required_string_fields_and_never_fake_success() {
    let catalog = protocol::catalog();
    assert_eq!(catalog.commands.len(), 24);
    assert_eq!(
        catalog
            .commands
            .iter()
            .map(|c| c.subcommands.len())
            .sum::<usize>(),
        9
    );
    for command in &catalog.commands {
        let input = minimal(command);
        assert!(protocol::validate(&input).is_ok(), "{}", command.code);
        assert_eq!(
            protocol::handle(&serde_json::to_vec(&input).unwrap())["SUCCESS"],
            if command.code == "CHECK_VERSION" {
                "TRUE"
            } else {
                "FALSE"
            }
        );
        for name in &command.required_strings {
            for invalid in [
                None,
                Some(Value::Null),
                Some(json!(true)),
                Some(json!(123)),
                Some(json!([])),
            ] {
                let mut bad = input.clone();
                match invalid {
                    None => {
                        bad.as_object_mut().unwrap().remove(name);
                    }
                    Some(value) => {
                        bad[name] = value;
                    }
                }
                assert!(
                    protocol::validate(&bad).is_err(),
                    "{} field {}",
                    command.code,
                    name
                );
            }
        }
    }
}

#[test]
fn import_subcommands_have_individual_schemas() {
    let command = protocol::catalog()
        .commands
        .iter()
        .find(|c| c.code == "IMP_002")
        .unwrap();
    for sub in &command.subcommands {
        let mut input = json!({"CODE":"IMP_002","SUB_CODE":sub.code.to_ascii_lowercase()});
        for field in &sub.required_strings {
            input[field] = json!("");
        }
        assert!(protocol::validate(&input).is_ok());
        for field in &sub.required_strings {
            let mut bad = input.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(protocol::validate(&bad).is_err(), "{} {}", sub.code, field);
        }
    }
    assert!(protocol::validate(&json!({"CODE":"IMP_002","SUB_CODE":"unknown"})).is_err());
}

#[test]
fn multiple_payment_signing_requires_both_paths_only_for_subcode_05() {
    let command = protocol::catalog()
        .commands
        .iter()
        .find(|c| c.code == "NTDT_CER_002")
        .unwrap();
    let mut input = minimal(command);
    input["NTDT_SIGN_CODE"] = json!("00");
    assert!(protocol::validate(&input).is_ok());
    input["NTDT_SIGN_CODE"] = json!("05");
    assert!(protocol::validate(&input).is_err());
    input["GDT_SIGN_PATH"] = json!("/gdt");
    assert!(protocol::validate(&input).is_err());
    input["BANK_SIGN_PATH"] = json!("/bank");
    assert!(protocol::validate(&input).is_ok());
}

#[test]
fn version_check_matches_the_original_no_update_reply_without_a_backend() {
    for code in ["CHECK_VERSION", "check_version"] {
        let bytes = json!({"CODE": code, "ROOT_URL": "https://thuedientu.gdt.gov.vn"}).to_string();
        let expected = json!({
            "CODE": code, "SUCCESS": "TRUE", "RESULT": "SUCC", "IS_UPDATE": "N"
        });
        let handled = protocol::handle_with(bytes.as_bytes(), "", None);
        assert_eq!(handled.reply, expected);
        assert_eq!(handled.action, code);
        assert!(handled.message.is_empty());
        assert!(!handled.signing);
        assert!(handled.certificate.is_empty() && handled.signer.is_empty());
        assert_eq!(protocol::summary(bytes.as_bytes(), false).1, "");

        let mut input = vec![];
        native::write_frame(&mut input, bytes.as_bytes()).unwrap();
        let mut output = vec![];
        native::serve(&mut Cursor::new(input), &mut output).unwrap();
        let reply = native::read_frame(&mut Cursor::new(output))
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&reply).unwrap(), expected);
    }
    for input in [
        json!({"CODE": "CHECK_VERSION"}),
        json!({"CODE": "CHECK_VERSION", "ROOT_URL": null}),
        json!({"CODE": "CHECK_VERSION", "ROOT_URL": 1}),
    ] {
        let handled = protocol::handle_with(input.to_string().as_bytes(), "", None);
        assert_eq!(
            handled.reply,
            json!({"CODE": "CHECK_VERSION", "SUCCESS": "FALSE"})
        );
        assert!(!handled.message.is_empty());
    }
}

#[test]
fn invalid_json_or_unknown_commands_are_explicit_failures() {
    for bytes in [
        b"null".as_slice(),
        b"[]",
        b"{",
        b"{}",
        b"{\"CODE\":false}",
        b"\xff",
    ] {
        assert_eq!(
            protocol::handle(bytes),
            json!({"CODE":"","SUCCESS":"FALSE"})
        );
    }
    assert!(protocol::validate(&json!({"CODE":"IMP_001"})).is_err());
    assert_eq!(
        protocol::handle(b"{\"CODE\":\"UNKNOWN\"}"),
        json!({"CODE":"UNKNOWN","SUCCESS":"FALSE"})
    );
}

#[test]
fn token_selection_rejects_invalid_ids_and_nonexistent_modules() {
    for id in ["", "f", "01 02", "zz", "😀", "éé"] {
        assert!(pkcs11::decode_hex(id).is_err());
    }
    assert_eq!(pkcs11::decode_hex("aB00ff").unwrap(), [171, 0, 255]);
    assert!(pkcs11::open(std::path::Path::new("relative.dll")).is_err());
    assert_eq!(pkcs11::encode_hex(&[171, 0, 255]), "ab00ff");
}

#[test]
fn online_signing_bytes_match_private_helpers_in_installed_java() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../src/esign/data/legacy-writer-fixtures.json"
    ))
    .unwrap();
    for case in fixtures["signatures"].as_array().unwrap() {
        let private = RsaPrivateKey::from_pkcs8_der(
            &STANDARD
                .decode(case["private_pkcs8_base64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        let public = RsaPublicKey::from_public_key_der(
            &STANDARD
                .decode(case["public_spki_base64"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        let algorithm = online::Algorithm::parse(case["algorithm"].as_str().unwrap()).unwrap();
        let input = case["base64_input"].as_str().unwrap();
        let output = online::sign_base64(input, algorithm, |data, alg| {
            let bytes = match alg {
                online::Algorithm::Sha1Rsa => SigningKey::<sha1::Sha1>::new(private.clone())
                    .sign(data)
                    .to_vec(),
                online::Algorithm::Sha256Rsa => SigningKey::<sha2::Sha256>::new(private.clone())
                    .sign(data)
                    .to_vec(),
                online::Algorithm::Sha384Rsa => SigningKey::<sha2::Sha384>::new(private.clone())
                    .sign(data)
                    .to_vec(),
                online::Algorithm::Sha512Rsa => SigningKey::<sha2::Sha512>::new(private.clone())
                    .sign(data)
                    .to_vec(),
            };
            Ok(bytes)
        })
        .unwrap();
        assert_eq!(output, case["signature_base64"], "{}", case["name"]);
        let data = online::decode_input(input).unwrap();
        let signature = Signature::try_from(STANDARD.decode(output).unwrap().as_slice()).unwrap();
        match algorithm {
            online::Algorithm::Sha1Rsa => VerifyingKey::<sha1::Sha1>::new(public)
                .verify(&data, &signature)
                .unwrap(),
            online::Algorithm::Sha256Rsa => VerifyingKey::<sha2::Sha256>::new(public)
                .verify(&data, &signature)
                .unwrap(),
            online::Algorithm::Sha384Rsa => VerifyingKey::<sha2::Sha384>::new(public)
                .verify(&data, &signature)
                .unwrap(),
            online::Algorithm::Sha512Rsa => VerifyingKey::<sha2::Sha512>::new(public)
                .verify(&data, &signature)
                .unwrap(),
        }
    }
}

#[test]
fn online_multi_sign_splits_and_rejects_invalid_payloads() {
    let a = STANDARD.encode("first");
    let b = STANDARD.encode("second");
    let output = online::sign_list(
        &format!("{a},{b}"),
        online::Algorithm::Sha256Rsa,
        |data, _| Ok(data.to_vec()),
    )
    .unwrap();
    assert_eq!(output, format!("{a},{b}"));
    for invalid in ["", ",", "%%%", "a,", "AAA=,%%%"] {
        assert!(
            online::sign_list(invalid, online::Algorithm::Sha256Rsa, |data, _| Ok(
                data.to_vec()
            ))
            .is_err()
        );
    }
    assert!(online::sign_base64(&a, online::Algorithm::Sha256Rsa, |_, _| Ok(vec![])).is_err());
}

#[test]
fn certificate_serial_and_pkipath_match_java() {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../src/esign/data/legacy-writer-fixtures.json"
    ))
    .unwrap();
    let cert = &fixtures["certificate"];
    let der = STANDARD
        .decode(cert["der_base64"].as_str().unwrap())
        .unwrap();
    let parsed = certificate::summary(&der).unwrap();
    assert_eq!(parsed["serial_decimal"], cert["serial_decimal"]);
    assert!(parsed["currently_valid"].as_bool().unwrap());
    assert_eq!(
        certificate::pki_path(std::slice::from_ref(&der)).unwrap(),
        cert["pki_path_base64"]
    );
    assert!(certificate::summary(&[der, vec![0]].concat()).is_err());
    assert!(certificate::pki_path(&[]).is_err());
}
