use crate::ctsign::{
    certificate,
    model::{CertificateInfo, HashInfo, Response, SignHashRequest},
};
use crate::esign::online::Algorithm;
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use cryptoki::{
    mechanism::{Mechanism, MechanismType},
    object::{Attribute, AttributeType, KeyType, ObjectClass, ObjectHandle},
    session::{Session, UserType},
    types::AuthPin,
};
use p12_keystore::{KeyStore, KeyStoreEntry, Pkcs12ImportPolicy};
use rsa::{Pkcs1v15Sign, RsaPrivateKey, pkcs1::DecodeRsaPrivateKey, pkcs8::DecodePrivateKey};
use sha2::Sha256;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use zeroize::Zeroizing;

enum Backend {
    Software(Box<RsaPrivateKey>),
    Token {
        session: Session,
        key: ObjectHandle,
    },
    WorkerToken {
        client: crate::token::remote::WorkerClient,
        slot: u64,
        key_id: Vec<u8>,
        pin: Option<Zeroizing<String>>,
    },
    #[cfg(windows)]
    Store(crate::certstore::StoreCert),
}

/// Where a signing identity's private key lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    File,
    Token,
    WindowsStore,
}

pub struct Identity {
    pub info: CertificateInfo,
    der: Vec<u8>,
    backend: Backend,
}

fn digest_of(data: &[u8], algorithm: Algorithm) -> Vec<u8> {
    use sha2::Digest;
    match algorithm {
        Algorithm::Sha1Rsa => sha1::Sha1::digest(data).to_vec(),
        Algorithm::Sha256Rsa => Sha256::digest(data).to_vec(),
        Algorithm::Sha384Rsa => sha2::Sha384::digest(data).to_vec(),
        Algorithm::Sha512Rsa => sha2::Sha512::digest(data).to_vec(),
    }
}

fn padding(algorithm: Algorithm) -> Pkcs1v15Sign {
    match algorithm {
        Algorithm::Sha1Rsa => Pkcs1v15Sign::new::<sha1::Sha1>(),
        Algorithm::Sha256Rsa => Pkcs1v15Sign::new::<Sha256>(),
        Algorithm::Sha384Rsa => Pkcs1v15Sign::new::<sha2::Sha384>(),
        Algorithm::Sha512Rsa => Pkcs1v15Sign::new::<sha2::Sha512>(),
    }
}

// DER DigestInfo prefixes for CKM_RSA_PKCS, which signs DigestInfo, not the hash.
fn digest_info_prefix(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::Sha1Rsa => "3021300906052b0e03021a05000414",
        Algorithm::Sha256Rsa => "3031300d060960864801650304020105000420",
        Algorithm::Sha384Rsa => "3041300d060960864801650304020205000430",
        Algorithm::Sha512Rsa => "3051300d060960864801650304020305000440",
    }
}

impl Identity {
    pub fn software(der: Vec<u8>, key: RsaPrivateKey) -> Result<Self> {
        key.validate()?;
        ensure!(
            certificate::public_key(&der)? == key.to_public_key(),
            "Certificate and private key do not match"
        );
        Ok(Self {
            info: certificate::describe(&der)?,
            der,
            backend: Backend::Software(Box::new(key)),
        })
    }

    #[cfg(windows)]
    pub fn windows_store(cert: crate::certstore::StoreCert) -> Result<Self> {
        let der = cert.der();
        certificate::public_key(&der)?;
        Ok(Self {
            info: certificate::describe(&der)?,
            der,
            backend: Backend::Store(cert),
        })
    }

    pub fn der(&self) -> &[u8] {
        &self.der
    }

    pub fn source(&self) -> Source {
        match &self.backend {
            Backend::Software(_) => Source::File,
            Backend::Token { .. } | Backend::WorkerToken { .. } => Source::Token,
            #[cfg(windows)]
            Backend::Store(_) => Source::WindowsStore,
        }
    }

    /// RSASSA-PKCS1-v1_5 over `data` with the given hash, verified before return.
    pub fn sign(&self, data: &[u8], algorithm: Algorithm) -> Result<Vec<u8>> {
        self.sign_digest(&digest_of(data, algorithm), algorithm)
    }

    pub fn sign_sha256(&self, digest: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            digest.len() == 32,
            "SHA-256 digest must contain exactly 32 bytes"
        );
        self.sign_digest(digest, Algorithm::Sha256Rsa)
    }

    fn sign_digest(&self, digest: &[u8], algorithm: Algorithm) -> Result<Vec<u8>> {
        ensure!(
            certificate::time_valid(&self.der)?,
            "Certificate is outside its validity period"
        );
        let signature = match &self.backend {
            Backend::Software(key) => {
                key.sign_with_rng(&mut rsa::rand_core::OsRng, padding(algorithm), digest)?
            }
            Backend::Token { session, key } => {
                let mut digest_info = hex::decode(digest_info_prefix(algorithm))?;
                digest_info.extend_from_slice(digest);
                session.sign(&Mechanism::RsaPkcs, *key, &digest_info)?
            }
            Backend::WorkerToken {
                client,
                slot,
                key_id,
                pin,
            } => client
                .sign_digest(
                    *slot,
                    key_id,
                    &self.der,
                    pin.as_ref().map(|value| value.as_str()),
                    digest,
                    algorithm,
                )
                .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            #[cfg(windows)]
            Backend::Store(cert) => cert
                .sign_digest(digest, algorithm)
                .map_err(|e| anyhow::anyhow!(e))?,
        };
        certificate::public_key(&self.der)?
            .verify(padding(algorithm), digest, &signature)
            .context("Signature does not verify against selected certificate")?;
        Ok(signature)
    }
}
#[derive(Clone, Default)]
pub struct SigningState {
    pub identities: Arc<Mutex<Vec<Identity>>>,
    pub api_signing_enabled: Arc<AtomicBool>,
}

impl SigningState {
    pub fn certificates(&self) -> Result<Vec<CertificateInfo>> {
        Ok(self
            .identities
            .lock()
            .map_err(|_| anyhow::anyhow!("Signer lock poisoned"))?
            .iter()
            .map(|i| i.info.clone())
            .collect())
    }

    pub fn add(&self, additions: Vec<Identity>) -> Result<usize> {
        ensure!(
            !additions.is_empty(),
            "No RSA private-key/certificate pairs found"
        );
        let mut identities = self
            .identities
            .lock()
            .map_err(|_| anyhow::anyhow!("Signer lock poisoned"))?;
        for identity in additions {
            certificate::normalize_serial(&identity.info.serinumber)?;
            identities.retain(|existing| existing.der() != identity.der());
            identities.push(identity);
        }
        Ok(identities.len())
    }

    pub fn clear(&self) -> Result<()> {
        self.api_signing_enabled.store(false, Ordering::SeqCst);
        self.identities
            .lock()
            .map_err(|_| anyhow::anyhow!("Signer lock poisoned"))?
            .clear();
        Ok(())
    }

    /// Runs `f` with the identity whose serial matches. On Windows a certificate
    /// that is not loaded yet is looked up in CurrentUser\My, like the original.
    /// Returns `Ok(None)` when no such certificate exists.
    #[cfg_attr(not(windows), allow(unused_mut))]
    pub fn with_identity<R>(
        &self,
        serial: &str,
        f: impl FnOnce(&Identity) -> Result<R>,
    ) -> Result<Option<R>> {
        let wanted = certificate::normalize_serial(serial)?;
        let matches = |i: &Identity| {
            certificate::normalize_serial(&i.info.serinumber)
                .ok()
                .as_ref()
                == Some(&wanted)
        };
        let mut identities = self
            .identities
            .lock()
            .map_err(|_| anyhow::anyhow!("Signer lock poisoned"))?;
        if !identities.iter().any(&matches) {
            #[cfg(windows)]
            for cert in crate::certstore::personal().unwrap_or_default() {
                if let Ok(identity) = Identity::windows_store(cert)
                    && matches(&identity)
                    && !identities
                        .iter()
                        .any(|existing| existing.der() == identity.der())
                {
                    identities.push(identity);
                }
            }
        }
        let mut matching = identities.iter().filter(|identity| matches(identity));
        let identity = matching.next();
        ensure!(
            matching.next().is_none(),
            "Có nhiều chứng thư cùng serial; hãy chỉ nạp chứng thư cần ký trước khi gửi yêu cầu"
        );
        match identity {
            Some(identity) => f(identity).map(Some),
            None => Ok(None),
        }
    }

    /// DER of the loaded certificate with this serial, if any.
    pub fn certificate_der(&self, serial: &str) -> Option<Vec<u8>> {
        self.with_identity(serial, |i| Ok(i.der().to_vec()))
            .ok()
            .flatten()
    }

    pub fn sign_hash(&self, request: SignHashRequest) -> Response<Vec<HashInfo>> {
        let run = || -> Result<Response<Vec<HashInfo>>> {
            // The original reports a missing certificate before validating input.
            if self
                .with_identity(&request.serinumber, |_| Ok(()))?
                .is_none()
            {
                return Ok(Response::error(
                    404,
                    format!("Không tìm thấy CTS có serinumber {}", request.serinumber),
                ));
            }
            // Validate the whole batch before asking a token to sign any item.
            let digests = request
                .hash
                .iter()
                .map(|h| -> Result<Vec<u8>> {
                    let bytes = STANDARD
                        .decode(&h.digest)
                        .context("Invalid base64 DLieuHash")?;
                    ensure!(
                        bytes.len() == 32,
                        "SHA-256 digest must contain exactly 32 bytes"
                    );
                    Ok(bytes)
                })
                .collect::<Result<Vec<_>>>()?;
            let serial = request.serinumber.clone();
            let signed = self.with_identity(&serial, |identity| {
                request
                    .hash
                    .into_iter()
                    .zip(digests)
                    .map(|(mut h, digest)| -> Result<HashInfo> {
                        h.signature = Some(STANDARD.encode(identity.sign_sha256(&digest)?));
                        Ok(h)
                    })
                    .collect::<Result<Vec<_>>>()
            })?;
            Ok(match signed {
                Some(signed) => Response::success(signed, "SUCCESS"),
                None => Response::error(404, format!("Không tìm thấy CTS có serinumber {serial}")),
            })
        };
        run().unwrap_or_else(|e| Response::error(500, e.to_string()))
    }
}

impl SigningState {
    /// Implements `HubClient/signpdf` with the original request/response shape.
    pub fn sign_pdf(&self, request: crate::ctsign::model::SignPdfRequest) -> Response<String> {
        use crate::pdf::{Appearance, SignOptions};
        let run = || -> Result<Response<String>> {
            let mut data = match request
                .file_path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
            {
                Some(path) => {
                    std::fs::read(path).with_context(|| format!("Không đọc được {path}"))?
                }
                None => STANDARD
                    .decode(request.data_file.as_deref().unwrap_or(""))
                    .context("dataFile không phải base64")?,
            };
            ensure!(!request.sig.is_empty(), "Thiếu danh sách vị trí ký (sig)");
            let found = self.with_identity(&request.serinumber, |identity| {
                for sig in &request.sig {
                    let mut sig = sig.clone();
                    let mut text = sig.info.clone().unwrap_or_default();
                    if sig.kind == 5 {
                        (sig.x, sig.y, sig.w, sig.h, sig.kind) = (0, 0, 200, 100, 2);
                        text = crate::pdf::default_text(identity.der());
                    }
                    let image = match (sig.kind, sig.image.as_deref()) {
                        (2, _) | (_, None) | (_, Some("")) => None,
                        (_, Some(b64)) => {
                            Some(STANDARD.decode(b64).context("image không phải base64")?)
                        }
                    };
                    if sig.kind == 1 && image.is_some() {
                        text.clear();
                    }
                    let page = sig.page.max(1) as u32;
                    let (x, y) = (sig.x as f32, sig.y as f32);
                    let (w, h) = (sig.w.max(1) as f32, sig.h.max(1) as f32);
                    let options = SignOptions {
                        appearance: Appearance::Visible {
                            page,
                            rect: [x, y, x + w, y + h],
                            text,
                            image,
                            image_background: sig.kind == 3,
                        },
                        field_name: sig.sig_field.clone(),
                        ..Default::default()
                    };
                    data = crate::pdf::sign(&data, &[identity.der().to_vec()], &options, |bytes| {
                        identity
                            .sign(bytes, Algorithm::Sha256Rsa)
                            .map_err(|e| e.to_string())
                    })
                    .map_err(|e| anyhow::anyhow!(e))?;
                }
                Ok(())
            })?;
            Ok(match found {
                Some(()) => Response::success(STANDARD.encode(&data), "Ký pdf thành công"),
                None => Response::error(
                    301,
                    format!(
                        "Không tìm thấy chứng thư số với serinumber:{}",
                        request.serinumber
                    ),
                ),
            })
        };
        run().unwrap_or_else(|e| Response::error(500, format!("Lỗi không xác định:{e}")))
    }
}

pub fn load_pkcs12(path: &Path, password: &str) -> Result<Vec<Identity>> {
    let bytes = Zeroizing::new(std::fs::read(path)?);
    let store = KeyStore::from_pkcs12(&bytes, password, Pkcs12ImportPolicy::Strict)?;
    let mut result = Vec::new();
    for (_, entry) in store.entries() {
        if let KeyStoreEntry::PrivateKeyChain(chain) = entry {
            let cert = chain.certs().first().context("No leaf certificate")?;
            let key = RsaPrivateKey::from_pkcs8_der(chain.key().as_der())
                .context("Only RSA keys match the original SignHash contract")?;
            result.push(Identity::software(cert.as_der().to_vec(), key)?);
        }
    }
    Ok(result)
}

pub fn load_pem(cert_path: &Path, key_path: &Path) -> Result<Vec<Identity>> {
    let data = std::fs::read(cert_path)?;
    let der = if data.starts_with(b"-----BEGIN") {
        x509_parser::pem::parse_x509_pem(&data)
            .map_err(|e| anyhow::anyhow!("Certificate PEM: {e}"))?
            .1
            .contents
    } else {
        data
    };
    let key_text = Zeroizing::new(std::fs::read_to_string(key_path)?);
    let key = RsaPrivateKey::from_pkcs8_pem(&key_text)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(&key_text))
        .context("Expected unencrypted RSA PKCS#8 or PKCS#1 PEM key")?;
    Ok(vec![Identity::software(der, key)?])
}

pub fn load_token(module: &Path, slot_index: usize, pin: Option<&str>) -> Result<Vec<Identity>> {
    let pkcs11 = crate::token::context(module).map_err(|e| anyhow::anyhow!(e))?;
    let slots = pkcs11.get_slots_with_token()?;
    let slot = *slots
        .get(slot_index)
        .context("No token at selected slot index")?;
    let mechanisms = pkcs11.get_mechanism_list(slot)?;
    ensure!(
        mechanisms.contains(&MechanismType::RSA_PKCS)
            && pkcs11
                .get_mechanism_info(slot, MechanismType::RSA_PKCS)?
                .sign(),
        "Token does not support CKM_RSA_PKCS signing required for the original RSA/SHA-256 contract"
    );
    let token_info = pkcs11.get_token_info(slot)?;
    ensure!(!token_info.user_pin_locked(), "Token PIN bị khóa");
    let login = login_mode(
        token_info.login_required(),
        token_info.protected_authentication_path(),
        pin,
    )?;
    // Retain one session per identity: object handles belong to their session.
    let inventory = pkcs11.open_ro_session(slot)?;
    match login {
        LoginMode::Skip => {}
        LoginMode::Pin(pin) => {
            inventory.login(UserType::User, Some(&AuthPin::new(pin.into())))?;
        }
        LoginMode::ProtectedPath => {
            inventory.login(UserType::User, None)?;
        }
    }
    let certificates = inventory.find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE)])?;
    let mut result = Vec::new();
    for cert in certificates {
        let attrs = inventory.get_attributes(cert, &[AttributeType::Id, AttributeType::Value])?;
        let id = attrs.iter().find_map(|a| {
            if let Attribute::Id(v) = a {
                Some(v.clone())
            } else {
                None
            }
        });
        let der = attrs.iter().find_map(|a| {
            if let Attribute::Value(v) = a {
                Some(v.clone())
            } else {
                None
            }
        });
        let (Some(id), Some(der)) = (id, der) else {
            continue;
        };
        if certificate::public_key(&der).is_err() {
            continue;
        }
        let session = pkcs11.open_ro_session(slot)?;
        let keys = session.find_objects(&[
            Attribute::Class(ObjectClass::PRIVATE_KEY),
            Attribute::Id(id),
            Attribute::KeyType(KeyType::RSA),
        ])?;
        if keys.len() > 1 {
            bail!("Ambiguous private key ID on token");
        }
        if let Some(key) = keys.first() {
            result.push(Identity {
                info: certificate::describe(&der)?,
                der,
                backend: Backend::Token { session, key: *key },
            });
        }
    }
    Ok(result)
}

/// Opens one token certificate (by slot ID and CKA_ID) for signing.
pub fn open_token(
    module: &Path,
    slot_id: u64,
    key_id: &[u8],
    pin: Option<&str>,
) -> Result<Identity> {
    if let Some(client) = crate::token::remote::WorkerClient::for_module(module)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
    {
        let tokens = client
            .tokens()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let token = tokens
            .as_array()
            .and_then(|tokens| {
                tokens
                    .iter()
                    .find(|token| token["slot"].as_u64() == Some(slot_id))
            })
            .context("Không thấy token ở khe đã chọn (đã rút token?)")?;
        ensure!(
            token["user_pin_locked"].as_bool() != Some(true),
            "Token PIN bị khóa"
        );
        let login = login_mode(
            token["login_required"]
                .as_bool()
                .context("Thiếu trạng thái đăng nhập token")?,
            token["protected_authentication_path"]
                .as_bool()
                .unwrap_or(false),
            pin,
        )?;
        let certificates = client
            .certificates(slot_id)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let expected_id = hex::encode(key_id);
        let matches: Vec<_> = certificates
            .as_array()
            .context("Danh sách chứng thư không hợp lệ")?
            .iter()
            .filter(|certificate| certificate["id_hex"].as_str() == Some(expected_id.as_str()))
            .collect();
        ensure!(
            matches.len() == 1,
            "Khóa phải tương ứng chính xác một chứng thư"
        );
        let der = STANDARD.decode(
            matches[0]["certificate_der_base64"]
                .as_str()
                .context("Chứng thư không có dữ liệu")?,
        )?;
        certificate::public_key(&der)?;
        let pin = match login {
            LoginMode::Pin(pin) => Some(Zeroizing::new(pin.to_owned())),
            LoginMode::Skip | LoginMode::ProtectedPath => None,
        };
        return Ok(Identity {
            info: certificate::describe(&der)?,
            der,
            backend: Backend::WorkerToken {
                client,
                slot: slot_id,
                key_id: key_id.to_vec(),
                pin,
            },
        });
    }
    let pkcs11 = crate::token::context(module).map_err(|e| anyhow::anyhow!(e))?;
    let slot = pkcs11
        .get_slots_with_token()?
        .into_iter()
        .find(|s| s.id() == slot_id)
        .context("Không thấy token ở khe đã chọn (đã rút token?)")?;
    let info = pkcs11.get_token_info(slot)?;
    ensure!(!info.user_pin_locked(), "Token PIN bị khóa");
    let login = login_mode(
        info.login_required(),
        info.protected_authentication_path(),
        pin,
    )?;
    let session = pkcs11.open_ro_session(slot)?;
    match login {
        LoginMode::Skip => {}
        LoginMode::Pin(pin) => session
            .login(UserType::User, Some(&AuthPin::new(pin.into())))
            .context("Sai PIN hoặc token bị khóa")?,
        LoginMode::ProtectedPath => session.login(UserType::User, None)?,
    }
    let certs = session.find_objects(&[
        Attribute::Class(ObjectClass::CERTIFICATE),
        Attribute::Id(key_id.to_vec()),
    ])?;
    ensure!(
        certs.len() == 1,
        "Khóa phải tương ứng chính xác một chứng thư"
    );
    let cert = &certs[0];
    let der = session
        .get_attributes(*cert, &[AttributeType::Value])?
        .into_iter()
        .find_map(|a| match a {
            Attribute::Value(v) => Some(v),
            _ => None,
        })
        .context("Chứng thư không có dữ liệu")?;
    certificate::public_key(&der)?;
    let keys = session.find_objects(&[
        Attribute::Class(ObjectClass::PRIVATE_KEY),
        Attribute::Id(key_id.to_vec()),
    ])?;
    ensure!(
        keys.len() == 1,
        "Không thấy khóa bí mật tương ứng trên token"
    );
    Ok(Identity {
        info: certificate::describe(&der)?,
        der,
        backend: Backend::Token {
            session,
            key: keys[0],
        },
    })
}

#[derive(Debug, PartialEq, Eq)]
enum LoginMode<'a> {
    Skip,
    Pin(&'a str),
    ProtectedPath,
}

fn login_mode<'a>(
    login_required: bool,
    protected_path: bool,
    pin: Option<&'a str>,
) -> Result<LoginMode<'a>> {
    if !login_required {
        return Ok(LoginMode::Skip);
    }
    if let Some(pin) = pin.filter(|value| !value.is_empty()) {
        return Ok(LoginMode::Pin(pin));
    }
    if protected_path {
        return Ok(LoginMode::ProtectedPath);
    }
    bail!("Token requires a PIN and has no protected PIN-pad authentication path")
}

#[cfg(test)]
mod tests {
    use super::{Identity, LoginMode, SigningState, login_mode};
    use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey};

    fn software_identity(signature_variant: bool) -> Identity {
        let mut der = include_bytes!("../../tests/fixtures/ctsign/certificate.der").to_vec();
        if signature_variant {
            *der.last_mut().unwrap() ^= 1;
        }
        Identity::software(
            der,
            RsaPrivateKey::from_pkcs8_pem(include_str!(
                "../../tests/fixtures/ctsign/private-key.fixture"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn distinct_certificates_with_the_same_serial_are_not_replaced() {
        let state = SigningState::default();
        let original = software_identity(false);
        let variant = software_identity(true);
        assert_eq!(original.info.serinumber, variant.info.serinumber);
        assert_ne!(original.der(), variant.der());
        assert_eq!(state.add(vec![original]).unwrap(), 1);
        assert_eq!(state.add(vec![variant]).unwrap(), 2);
        assert_eq!(state.add(vec![software_identity(false)]).unwrap(), 2);
    }

    #[test]
    fn ambiguous_serial_never_invokes_the_signing_operation() {
        let state = SigningState::default();
        let original = software_identity(false);
        let serial = original.info.serinumber.clone();
        state.add(vec![original, software_identity(true)]).unwrap();
        let mut invoked = false;
        let result = state.with_identity(&format!(" 00{serial} "), |_| {
            invoked = true;
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("cùng serial"));
        assert!(!invoked);
        assert!(state.certificate_der(&serial).is_none());
    }

    #[test]
    fn token_login_requires_pin_or_advertised_protected_path() {
        assert_eq!(login_mode(false, false, None).unwrap(), LoginMode::Skip);
        assert_eq!(
            login_mode(true, false, Some("1234")).unwrap(),
            LoginMode::Pin("1234")
        );
        assert_eq!(
            login_mode(true, true, None).unwrap(),
            LoginMode::ProtectedPath
        );
        assert_eq!(
            login_mode(true, true, Some("")).unwrap(),
            LoginMode::ProtectedPath
        );
        assert!(login_mode(true, false, None).is_err());
        assert!(login_mode(true, false, Some("")).is_err());
    }
}
