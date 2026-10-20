use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_FRAME_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretPin(String);

impl SecretPin {
    pub fn new(pin: String) -> Self {
        Self(pin)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

impl Drop for SecretPin {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignDigestRequest {
    pub protocol: u32,
    pub slot: u64,
    pub key_id_hex: String,
    pub certificate_sha256: String,
    pub algorithm: String,
    pub digest_base64: String,
    pub pin: Option<SecretPin>,
}

pub fn read_frame(mut input: impl Read) -> std::io::Result<Zeroizing<Vec<u8>>> {
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let length = u32::from_le_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid worker frame length",
        ));
    }
    let mut body = Zeroizing::new(vec![0; length]);
    input.read_exact(&mut body)?;
    let mut trailing = [0];
    if input.read(&mut trailing)? != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "trailing worker input",
        ));
    }
    Ok(body)
}

pub fn write_frame(mut output: impl Write, body: &[u8]) -> std::io::Result<()> {
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid worker frame length",
        ));
    }
    output.write_all(&(body.len() as u32).to_le_bytes())?;
    output.write_all(body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framed_messages_are_bounded_and_exact() {
        let mut frame = Vec::new();
        write_frame(&mut frame, b"payload").unwrap();
        assert_eq!(read_frame(frame.as_slice()).unwrap().as_slice(), b"payload");
        assert!(read_frame(&frame[..frame.len() - 1]).is_err());
        frame.push(0);
        assert!(read_frame(frame.as_slice()).is_err());
        assert!(read_frame((MAX_FRAME_BYTES as u32 + 1).to_le_bytes().as_slice()).is_err());
        assert!(read_frame(0u32.to_le_bytes().as_slice()).is_err());
    }

    #[test]
    fn protocol_rejects_unknown_fields_and_erases_pin() {
        let mut request = SignDigestRequest {
            protocol: 1,
            slot: 1,
            key_id_hex: "01".into(),
            certificate_sha256: String::new(),
            algorithm: "sha256".into(),
            digest_base64: String::new(),
            pin: Some(SecretPin::new("test only".into())),
        };
        let mut value = serde_json::to_value(&request).unwrap();
        value["unexpected"] = true.into();
        assert!(serde_json::from_value::<SignDigestRequest>(value).is_err());
        assert_eq!(request.pin.as_mut().unwrap().take(), "test only");
        assert!(request.pin.as_ref().unwrap().expose().is_empty());
    }
}
