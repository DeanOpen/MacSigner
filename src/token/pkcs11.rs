use crate::crypto::{Algorithm, certificate};
use base64::{Engine, engine::general_purpose::STANDARD};
use cryptoki::{
    context::{Function, Pkcs11},
    error::{Error as Pkcs11Error, RvError},
    mechanism::{Mechanism, MechanismType},
    object::{Attribute, AttributeType, ObjectClass, ObjectHandle},
    session::{Session, UserType},
    types::AuthPin,
};
use rsa::{BigUint, RsaPublicKey};
use serde_json::{Value, json};
use sha2::Digest;
use std::{error::Error, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
pub enum AuthenticationError {
    PinRequired,
    PinLocked,
}

impl std::fmt::Display for AuthenticationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PinRequired => "PIN is required for this token",
            Self::PinLocked => "token PIN is locked",
        })
    }
}

impl Error for AuthenticationError {}

pub fn open(module: &Path) -> Result<Pkcs11> {
    crate::token::context(module).map_err(|e| e.to_string().into())
}

pub fn tokens(ctx: &Pkcs11) -> Result<Value> {
    let mut result = vec![];
    for (slot, info) in
        recognized_token_infos(ctx.get_slots_with_token()?, |slot| ctx.get_token_info(slot))?
    {
        result.push(json!({"slot": slot.id(), "label": info.label(), "manufacturer": info.manufacturer_id(), "model": info.model(), "serial": info.serial_number(), "login_required": info.login_required(), "protected_authentication_path": info.protected_authentication_path(), "user_pin_locked": info.user_pin_locked(), "user_pin_final_try": info.user_pin_final_try(), "user_pin_count_low": info.user_pin_count_low()}));
    }
    Ok(Value::Array(result))
}

fn recognized_token_infos<Slot: Copy, Info>(
    slots: impl IntoIterator<Item = Slot>,
    mut get_info: impl FnMut(Slot) -> std::result::Result<Info, Pkcs11Error>,
) -> Result<Vec<(Slot, Info)>> {
    let mut recognized = Vec::new();
    for slot in slots {
        match get_info(slot) {
            Ok(info) => recognized.push((slot, info)),
            Err(Pkcs11Error::Pkcs11(
                RvError::TokenNotRecognized
                | RvError::TokenNotPresent
                | RvError::DeviceRemoved
                | RvError::SlotIdInvalid,
                Function::GetTokenInfo,
            )) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(recognized)
}

pub fn signing_mechanisms(ctx: &Pkcs11, slot_id: u64) -> Result<Value> {
    let slot = find_slot(ctx, slot_id)?;
    let mut result = Vec::new();
    for mechanism in ctx.get_mechanism_list(slot)? {
        let info = ctx.get_mechanism_info(slot, mechanism)?;
        if info.sign() {
            result.push(json!({
                "mechanism": mechanism.to_string(),
                "min_key_size": info.min_key_size(),
                "max_key_size": info.max_key_size(),
                "hardware": info.hardware()
            }));
        }
    }
    Ok(Value::Array(result))
}

fn signature_plan(ctx: &Pkcs11, slot: cryptoki::slot::Slot, algorithm: Algorithm) -> Result<bool> {
    let combined = match algorithm {
        Algorithm::Sha1Rsa => MechanismType::SHA1_RSA_PKCS,
        Algorithm::Sha256Rsa => MechanismType::SHA256_RSA_PKCS,
        Algorithm::Sha384Rsa => MechanismType::SHA384_RSA_PKCS,
        Algorithm::Sha512Rsa => MechanismType::SHA512_RSA_PKCS,
    };
    let available = ctx.get_mechanism_list(slot)?;
    if available.contains(&combined) && ctx.get_mechanism_info(slot, combined)?.sign() {
        return Ok(false);
    }
    if available.contains(&MechanismType::RSA_PKCS)
        && ctx
            .get_mechanism_info(slot, MechanismType::RSA_PKCS)?
            .sign()
    {
        return Ok(true);
    }
    Err("token does not advertise a compatible RSA signing mechanism".into())
}

pub fn ensure_signing_supported(ctx: &Pkcs11, slot_id: u64, algorithm: Algorithm) -> Result<()> {
    signature_plan(ctx, find_slot(ctx, slot_id)?, algorithm)?;
    Ok(())
}

// DER DigestInfo is signed with CKM_RSA_PKCS when the token does not offer a
// combined hash-and-sign mechanism. The token still performs the private-key step.
fn hash_data(data: &[u8], algorithm: Algorithm) -> Vec<u8> {
    match algorithm {
        Algorithm::Sha1Rsa => sha1::Sha1::digest(data).to_vec(),
        Algorithm::Sha256Rsa => sha2::Sha256::digest(data).to_vec(),
        Algorithm::Sha384Rsa => sha2::Sha384::digest(data).to_vec(),
        Algorithm::Sha512Rsa => sha2::Sha512::digest(data).to_vec(),
    }
}

fn digest_info(data: &[u8], algorithm: Algorithm) -> Vec<u8> {
    let digest = hash_data(data, algorithm);
    digest_info_from_digest(&digest, algorithm).expect("hash output has the selected digest length")
}

fn digest_info_from_digest(digest: &[u8], algorithm: Algorithm) -> Result<Vec<u8>> {
    let (prefix, length): (&[u8], usize) = match algorithm {
        Algorithm::Sha1Rsa => (
            &[
                0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04,
                0x14,
            ],
            20,
        ),
        Algorithm::Sha256Rsa => (
            &[
                0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x01, 0x05, 0x00, 0x04, 0x20,
            ],
            32,
        ),
        Algorithm::Sha384Rsa => (
            &[
                0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x02, 0x05, 0x00, 0x04, 0x30,
            ],
            48,
        ),
        Algorithm::Sha512Rsa => (
            &[
                0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x03, 0x05, 0x00, 0x04, 0x40,
            ],
            64,
        ),
    };
    if digest.len() != length {
        return Err("digest length does not match the selected algorithm".into());
    }
    let mut encoded = prefix.to_vec();
    encoded.extend_from_slice(digest);
    Ok(encoded)
}

pub fn sign_digest(
    ctx: &Pkcs11,
    slot_id: u64,
    id: &[u8],
    pin: Option<AuthPin>,
    digest: &[u8],
    algorithm: Algorithm,
    expected_certificate_sha256: &str,
) -> Result<Vec<u8>> {
    if id.is_empty() {
        return Err("key ID must not be empty".into());
    }
    let encoded = digest_info_from_digest(digest, algorithm)?;
    let der = certificate_by_id(ctx, slot_id, id)?;
    let summary = certificate::summary(&der)?;
    if summary["fingerprint_sha256"].as_str() != Some(expected_certificate_sha256) {
        return Err("selected certificate changed; refresh and select it again".into());
    }
    if summary["currently_valid"].as_bool() != Some(true) {
        return Err("selected certificate is outside its validity period".into());
    }
    certificate::rsa_public_key(&der)?;
    let slot = find_slot(ctx, slot_id)?;
    let info = ctx.get_token_info(slot)?;
    if info.user_pin_locked() {
        return Err(AuthenticationError::PinLocked.into());
    }
    if info.login_required() && pin.is_none() && !info.protected_authentication_path() {
        return Err(AuthenticationError::PinRequired.into());
    }
    if !ctx
        .get_mechanism_list(slot)?
        .contains(&MechanismType::RSA_PKCS)
        || !ctx
            .get_mechanism_info(slot, MechanismType::RSA_PKCS)?
            .sign()
    {
        return Err("token does not support signing a prepared RSA digest".into());
    }
    let session = ctx.open_ro_session(slot)?;
    if info.login_required() {
        session.login(UserType::User, pin.as_ref())?;
    }
    let signature = with_logout(
        || -> Result<Vec<u8>> {
            let keys = session.find_objects(&[
                Attribute::Class(ObjectClass::PRIVATE_KEY),
                Attribute::Id(id.to_vec()),
            ])?;
            if keys.len() != 1 {
                return Err("key ID must identify exactly one private key".into());
            }
            Ok(session.sign(&Mechanism::RsaPkcs, keys[0], &encoded)?)
        },
        || {
            if info.login_required() {
                session.logout()?;
            }
            Ok(())
        },
    )?;
    certificate::verify_rsa_digest(&der, digest, &signature, algorithm)?;
    Ok(signature)
}

fn with_logout<T>(
    operation: impl FnOnce() -> Result<T>,
    logout: impl FnOnce() -> Result<()>,
) -> Result<T> {
    let outcome = operation();
    let cleanup = logout();
    let value = outcome?;
    cleanup?;
    Ok(value)
}

fn public_key_from_attributes(attributes: Vec<Attribute>) -> Result<RsaPublicKey> {
    let mut modulus = None;
    let mut exponent = None;
    for attribute in attributes {
        match attribute {
            Attribute::Modulus(value) => modulus = Some(value),
            Attribute::PublicExponent(value) => exponent = Some(value),
            _ => {}
        }
    }
    Ok(RsaPublicKey::new(
        BigUint::from_bytes_be(&modulus.ok_or("RSA modulus is unavailable")?),
        BigUint::from_bytes_be(&exponent.ok_or("RSA public exponent is unavailable")?),
    )?)
}

fn verification_key(
    session: &Session,
    id: &[u8],
    private_key: ObjectHandle,
) -> Result<RsaPublicKey> {
    let certificates = session.find_objects(&[
        Attribute::Class(ObjectClass::CERTIFICATE),
        Attribute::Id(id.to_vec()),
    ])?;
    if certificates.len() > 1 {
        return Err("key ID must identify at most one certificate".into());
    }
    if let Some(certificate) = certificates.first() {
        for attribute in session.get_attributes(*certificate, &[AttributeType::Value])? {
            if let Attribute::Value(der) = attribute {
                if certificate::summary(&der)?["currently_valid"].as_bool() != Some(true) {
                    return Err("selected certificate is outside its validity period".into());
                }
                return certificate::rsa_public_key(&der);
            }
        }
        return Err("certificate DER is unavailable".into());
    }
    let public_keys = session.find_objects(&[
        Attribute::Class(ObjectClass::PUBLIC_KEY),
        Attribute::Id(id.to_vec()),
    ])?;
    if public_keys.len() > 1 {
        return Err("key ID must identify at most one public key".into());
    }
    let public_key = public_keys.first().copied().unwrap_or(private_key);
    public_key_from_attributes(session.get_attributes(
        public_key,
        &[AttributeType::Modulus, AttributeType::PublicExponent],
    )?)
}

fn find_slot(ctx: &Pkcs11, slot_id: u64) -> Result<cryptoki::slot::Slot> {
    ctx.get_slots_with_token()?
        .into_iter()
        .find(|s| s.id() == slot_id)
        .ok_or_else(|| "selected token slot is unavailable".into())
}

pub fn certificates(ctx: &Pkcs11, slot_id: u64) -> Result<Value> {
    let session = ctx.open_ro_session(find_slot(ctx, slot_id)?)?;
    let objects = session.find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE)])?;
    let mut certs = vec![];
    for object in objects {
        let attrs = session.get_attributes(
            object,
            &[
                AttributeType::Id,
                AttributeType::Label,
                AttributeType::Value,
            ],
        )?;
        let mut id = vec![];
        let mut label = vec![];
        let mut der = vec![];
        for attr in attrs {
            match attr {
                Attribute::Id(v) => id = v,
                Attribute::Label(v) => label = v,
                Attribute::Value(v) => der = v,
                _ => {}
            }
        }
        let summary = certificate::summary(&der)?;
        certs.push(json!({"id_hex": encode_hex(&id), "label": String::from_utf8_lossy(&label), "certificate_der_base64": STANDARD.encode(der), "parsed":summary}));
    }
    Ok(Value::Array(certs))
}

pub fn certificate_by_id(ctx: &Pkcs11, slot_id: u64, id: &[u8]) -> Result<Vec<u8>> {
    let session = ctx.open_ro_session(find_slot(ctx, slot_id)?)?;
    let matches = session.find_objects(&[
        Attribute::Class(ObjectClass::CERTIFICATE),
        Attribute::Id(id.to_vec()),
    ])?;
    if matches.len() != 1 {
        return Err("key ID must identify exactly one certificate".into());
    }
    for attr in session.get_attributes(matches[0], &[AttributeType::Value])? {
        if let Attribute::Value(value) = attr {
            certificate::summary(&value)?;
            return Ok(value);
        }
    }
    Err("certificate DER is unavailable".into())
}

// Signs the supplied bytes once. This is RSA/SHA-256, not an XMLDSig or PDF container.
// Keep this API off browser transports until certificate policy and user consent exist.
pub fn sign(ctx: &Pkcs11, slot_id: u64, id: &[u8], pin: AuthPin, data: &[u8]) -> Result<Vec<u8>> {
    sign_with_algorithm(ctx, slot_id, id, pin, data, Algorithm::Sha256Rsa)
}

pub fn sign_with_algorithm(
    ctx: &Pkcs11,
    slot_id: u64,
    id: &[u8],
    pin: AuthPin,
    data: &[u8],
    algorithm: Algorithm,
) -> Result<Vec<u8>> {
    let signatures = sign_many(ctx, slot_id, id, pin, &[data.to_vec()], algorithm)?;
    Ok(signatures.into_iter().next().expect("one signature"))
}

pub fn sign_many(
    ctx: &Pkcs11,
    slot_id: u64,
    id: &[u8],
    pin: AuthPin,
    data: &[Vec<u8>],
    algorithm: Algorithm,
) -> Result<Vec<Vec<u8>>> {
    if id.is_empty() {
        return Err("key ID must not be empty".into());
    }
    if data.is_empty() || data.len() > 100 || data.iter().any(Vec::is_empty) {
        return Err("invalid signing input count or empty payload".into());
    }
    let slot = find_slot(ctx, slot_id)?;
    let host_hash = signature_plan(ctx, slot, algorithm)?;
    let info = ctx.get_token_info(slot)?;
    if info.user_pin_locked() {
        return Err(AuthenticationError::PinLocked.into());
    }
    let session = ctx.open_ro_session(slot)?;
    if info.login_required() {
        session.login(UserType::User, Some(&pin))?;
    }
    with_logout(
        || {
            let keys = session.find_objects(&[
                Attribute::Class(ObjectClass::PRIVATE_KEY),
                Attribute::Id(id.to_vec()),
            ])?;
            if keys.len() != 1 {
                return Err("key ID must identify exactly one private key".into());
            }
            let public_key = verification_key(&session, id, keys[0])?;
            let mechanism = if host_hash {
                Mechanism::RsaPkcs
            } else {
                match algorithm {
                    Algorithm::Sha1Rsa => Mechanism::Sha1RsaPkcs,
                    Algorithm::Sha256Rsa => Mechanism::Sha256RsaPkcs,
                    Algorithm::Sha384Rsa => Mechanism::Sha384RsaPkcs,
                    Algorithm::Sha512Rsa => Mechanism::Sha512RsaPkcs,
                }
            };
            let mut signatures = Vec::with_capacity(data.len());
            for item in data {
                let signature = if host_hash {
                    session.sign(&mechanism, keys[0], &digest_info(item, algorithm))?
                } else {
                    session.sign(&mechanism, keys[0], item)?
                };
                certificate::verify_rsa_digest_with_key(
                    &public_key,
                    &hash_data(item, algorithm),
                    &signature,
                    algorithm,
                )?;
                signatures.push(signature);
            }
            Ok(signatures)
        },
        || {
            if info.login_required() {
                session.logout()?;
            }
            Ok(())
        },
    )
}

pub fn encode_hex(data: &[u8]) -> String {
    data.iter().map(|v| format!("{v:02x}")).collect()
}

pub fn decode_hex(input: &str) -> Result<Vec<u8>> {
    if input.is_empty()
        || !input.len().is_multiple_of(2)
        || !input.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("key ID must be nonempty even-length hexadecimal".into());
    }
    (0..input.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&input[i..i + 2], 16).map_err(Into::into))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::{
        RsaPrivateKey, RsaPublicKey,
        pkcs1v15::{Pkcs1v15Sign, Signature, VerifyingKey},
        pkcs8::DecodePrivateKey,
        signature::Verifier,
        traits::PublicKeyParts,
    };
    use std::cell::Cell;

    #[test]
    fn token_inventory_continues_after_unrecognized_or_removed_tokens() {
        let visited = Cell::new(0);
        let tokens = recognized_token_infos([1, 2, 3, 4, 5, 6], |slot| {
            visited.set(visited.get() + 1);
            match slot {
                1 => Err(Pkcs11Error::Pkcs11(
                    RvError::TokenNotRecognized,
                    Function::GetTokenInfo,
                )),
                3 => Err(Pkcs11Error::Pkcs11(
                    RvError::DeviceRemoved,
                    Function::GetTokenInfo,
                )),
                4 => Err(Pkcs11Error::Pkcs11(
                    RvError::TokenNotPresent,
                    Function::GetTokenInfo,
                )),
                5 => Err(Pkcs11Error::Pkcs11(
                    RvError::SlotIdInvalid,
                    Function::GetTokenInfo,
                )),
                _ => Ok(slot * 10),
            }
        })
        .unwrap();
        assert_eq!(visited.get(), 6);
        assert_eq!(tokens, vec![(2, 20), (6, 60)]);
    }

    #[test]
    fn token_inventory_preserves_driver_and_authentication_failures() {
        for error in [
            RvError::DeviceError,
            RvError::GeneralError,
            RvError::PinIncorrect,
        ] {
            let result = recognized_token_infos([1], |_| -> std::result::Result<(), Pkcs11Error> {
                Err(Pkcs11Error::Pkcs11(error, Function::GetTokenInfo))
            });
            assert!(result.is_err());
        }
        let wrong_function =
            recognized_token_infos([1], |_| -> std::result::Result<(), Pkcs11Error> {
                Err(Pkcs11Error::Pkcs11(
                    RvError::TokenNotPresent,
                    Function::Login,
                ))
            });
        assert!(wrong_function.is_err());
    }

    #[test]
    fn unrecognized_tokens_never_become_inventory_entries() {
        let result = recognized_token_infos([1, 2], |_| -> std::result::Result<(), Pkcs11Error> {
            Err(Pkcs11Error::Pkcs11(
                RvError::TokenNotRecognized,
                Function::GetTokenInfo,
            ))
        })
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn signing_cleanup_runs_once_on_success_and_failure() {
        for fail in [false, true] {
            let operations = Cell::new(0);
            let logouts = Cell::new(0);
            let result = with_logout(
                || {
                    operations.set(operations.get() + 1);
                    if fail {
                        Err("signing failed".into())
                    } else {
                        Ok(vec![1])
                    }
                },
                || {
                    logouts.set(logouts.get() + 1);
                    Ok(())
                },
            );
            assert_eq!(operations.get(), 1);
            assert_eq!(logouts.get(), 1);
            assert_eq!(result.is_err(), fail);
        }
    }

    #[test]
    fn cleanup_failure_never_returns_a_signature_and_preserves_operation_errors() {
        let result = with_logout(|| Ok(vec![1]), || Err("logout failed".into()));
        assert_eq!(result.unwrap_err().to_string(), "logout failed");
        let result: Result<Vec<u8>> = with_logout(
            || Err("signing failed".into()),
            || Err("logout failed".into()),
        );
        assert_eq!(result.unwrap_err().to_string(), "signing failed");
    }

    #[test]
    fn public_key_attributes_require_complete_valid_rsa_parameters() {
        let private = RsaPrivateKey::from_pkcs8_pem(include_str!(
            "../../tests/fixtures/ctsign/private-key.fixture"
        ))
        .unwrap();
        let public = RsaPublicKey::from(&private);
        let attributes = vec![
            Attribute::Modulus(public.n().to_bytes_be()),
            Attribute::PublicExponent(public.e().to_bytes_be()),
        ];
        assert_eq!(public_key_from_attributes(attributes).unwrap(), public);
        assert!(
            public_key_from_attributes(vec![Attribute::Modulus(public.n().to_bytes_be())]).is_err()
        );
        assert!(
            public_key_from_attributes(vec![Attribute::PublicExponent(public.e().to_bytes_be())])
                .is_err()
        );
        assert!(
            public_key_from_attributes(vec![
                Attribute::Modulus(vec![0]),
                Attribute::PublicExponent(vec![0])
            ])
            .is_err()
        );
    }

    #[test]
    fn raw_rsa_digest_info_matches_standard_hash_and_sign_for_all_hashes() {
        let fixtures: Value =
            serde_json::from_str(include_str!("../esign/data/legacy-writer-fixtures.json"))
                .unwrap();
        let private = RsaPrivateKey::from_pkcs8_der(
            &STANDARD
                .decode(
                    fixtures["signatures"][0]["private_pkcs8_base64"]
                        .as_str()
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        let public = RsaPublicKey::from(&private);
        let data = b"PKCS11 raw RSA fallback";
        for algorithm in [
            Algorithm::Sha1Rsa,
            Algorithm::Sha256Rsa,
            Algorithm::Sha384Rsa,
            Algorithm::Sha512Rsa,
        ] {
            let raw = private
                .sign(
                    Pkcs1v15Sign::new_unprefixed(),
                    &digest_info(data, algorithm),
                )
                .unwrap();
            let signature = Signature::try_from(raw.as_slice()).unwrap();
            match algorithm {
                Algorithm::Sha1Rsa => VerifyingKey::<sha1::Sha1>::new(public.clone())
                    .verify(data, &signature)
                    .unwrap(),
                Algorithm::Sha256Rsa => VerifyingKey::<sha2::Sha256>::new(public.clone())
                    .verify(data, &signature)
                    .unwrap(),
                Algorithm::Sha384Rsa => VerifyingKey::<sha2::Sha384>::new(public.clone())
                    .verify(data, &signature)
                    .unwrap(),
                Algorithm::Sha512Rsa => VerifyingKey::<sha2::Sha512>::new(public.clone())
                    .verify(data, &signature)
                    .unwrap(),
            }
        }
    }
}
