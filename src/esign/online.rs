use base64::{Engine, engine::general_purpose::STANDARD};
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub use crate::crypto::Algorithm;

// Base64 payload contains the bytes Java's Signature.update receives. The browser
// often sends a canonicalized SignedInfo element; this layer must not hash it early.
pub fn decode_input(value: &str) -> Result<Vec<u8>> {
    if value.len() > 1024 * 1024 {
        return Err("BASE64_VALUE exceeds 1 MiB".into());
    }
    let data = STANDARD.decode(value)?;
    if data.is_empty() {
        return Err("BASE64_VALUE is empty".into());
    }
    Ok(data)
}

pub fn sign_base64<F>(value: &str, algorithm: Algorithm, mut signer: F) -> Result<String>
where
    F: FnMut(&[u8], Algorithm) -> Result<Vec<u8>>,
{
    let data = decode_input(value)?;
    let signature = signer(&data, algorithm)?;
    if signature.is_empty() {
        return Err("token returned an empty signature".into());
    }
    Ok(STANDARD.encode(signature))
}

pub fn sign_list<F>(values: &str, algorithm: Algorithm, mut signer: F) -> Result<String>
where
    F: FnMut(&[u8], Algorithm) -> Result<Vec<u8>>,
{
    if values.is_empty() || values.len() > 1024 * 1024 {
        return Err("invalid signature list length".into());
    }
    let mut signed = Vec::new();
    for value in values.split(',') {
        if signed.len() >= 100 {
            return Err("too many signature inputs".into());
        }
        signed.push(sign_base64(value, algorithm, &mut signer)?);
    }
    Ok(signed.join(","))
}
