// Runs against an ephemeral SoftHSM token in Linux CI. No physical token is touched.
use cryptoki::{
    mechanism::Mechanism,
    object::{Attribute, AttributeType, ObjectClass},
    session::UserType,
    types::AuthPin,
};
use macsigner::esign::{online::Algorithm, pkcs11};
use rsa::{
    BigUint, RsaPublicKey,
    pkcs1v15::{Signature, VerifyingKey},
    signature::Verifier,
};

#[test]
fn softhsm_generates_key_and_signs_through_real_pkcs11_api() {
    // CI sets the variable to "" where SoftHSM is not installed (macOS, Windows).
    let Some(module) = std::env::var("ESIGN_TEST_SOFTHSM_MODULE")
        .ok()
        .filter(|m| !m.is_empty())
    else {
        return;
    };
    let pin = std::env::var("ESIGN_TEST_SOFTHSM_PIN").expect("CI test PIN");
    let ctx = pkcs11::open(std::path::Path::new(&module)).unwrap();
    let slot = ctx
        .get_slots_with_token()
        .unwrap()
        .into_iter()
        .next()
        .expect("CI token");
    let capabilities = pkcs11::signing_mechanisms(&ctx, slot.id()).unwrap();
    assert!(!capabilities.as_array().unwrap().is_empty());
    let id = vec![0x45, 0x53, 0x49, 0x47, 0x4e, 0x2d, 0x54, 0x45, 0x53, 0x54];
    let session = ctx.open_rw_session(slot).unwrap();
    session
        .login(UserType::User, Some(&AuthPin::new(pin.clone().into())))
        .unwrap();
    let public_template = [
        Attribute::Token(true),
        Attribute::Private(false),
        Attribute::Label(b"ESIGN CI".to_vec()),
        Attribute::Id(id.clone()),
        Attribute::Verify(true),
        Attribute::ModulusBits(2048.into()),
        Attribute::PublicExponent(vec![1, 0, 1]),
    ];
    let private_template = [
        Attribute::Token(true),
        Attribute::Private(true),
        Attribute::Sensitive(true),
        Attribute::Label(b"ESIGN CI".to_vec()),
        Attribute::Id(id.clone()),
        Attribute::Sign(true),
    ];
    let (public, private) = session
        .generate_key_pair(
            &Mechanism::RsaPkcsKeyPairGen,
            &public_template,
            &private_template,
        )
        .unwrap();
    let attrs = session
        .get_attributes(
            public,
            &[AttributeType::Modulus, AttributeType::PublicExponent],
        )
        .unwrap();
    let mut modulus = Vec::new();
    let mut exponent = Vec::new();
    for attr in attrs {
        match attr {
            Attribute::Modulus(v) => modulus = v,
            Attribute::PublicExponent(v) => exponent = v,
            _ => {}
        }
    }
    let rsa_public = RsaPublicKey::new(
        BigUint::from_bytes_be(&modulus),
        BigUint::from_bytes_be(&exponent),
    )
    .unwrap();
    session.logout().unwrap();
    drop(session);

    let data = "Ký số trên SoftHSM — UTF-8".as_bytes();
    for algorithm in [
        Algorithm::Sha1Rsa,
        Algorithm::Sha256Rsa,
        Algorithm::Sha384Rsa,
        Algorithm::Sha512Rsa,
    ] {
        pkcs11::ensure_signing_supported(&ctx, slot.id(), algorithm).unwrap();
        let signature = pkcs11::sign_with_algorithm(
            &ctx,
            slot.id(),
            &id,
            AuthPin::new(pin.clone().into()),
            data,
            algorithm,
        )
        .unwrap();
        let signature = Signature::try_from(signature.as_slice()).unwrap();
        match algorithm {
            Algorithm::Sha1Rsa => VerifyingKey::<sha1::Sha1>::new(rsa_public.clone())
                .verify(data, &signature)
                .unwrap(),
            Algorithm::Sha256Rsa => VerifyingKey::<sha2::Sha256>::new(rsa_public.clone())
                .verify(data, &signature)
                .unwrap(),
            Algorithm::Sha384Rsa => VerifyingKey::<sha2::Sha384>::new(rsa_public.clone())
                .verify(data, &signature)
                .unwrap(),
            Algorithm::Sha512Rsa => VerifyingKey::<sha2::Sha512>::new(rsa_public.clone())
                .verify(data, &signature)
                .unwrap(),
        }
    }

    let cleanup = ctx.open_rw_session(slot).unwrap();
    cleanup
        .login(UserType::User, Some(&AuthPin::new(pin.into())))
        .unwrap();
    // Object handles are session-independent in SoftHSM, but locate by ID for portability.
    for class in [ObjectClass::PRIVATE_KEY, ObjectClass::PUBLIC_KEY] {
        for object in cleanup
            .find_objects(&[Attribute::Class(class), Attribute::Id(id.clone())])
            .unwrap()
        {
            cleanup.destroy_object(object).unwrap();
        }
    }
    cleanup.logout().unwrap();
    let _ = (public, private);
}
