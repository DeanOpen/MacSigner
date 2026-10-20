use super::worker_protocol::{self, SecretPin, SignDigestRequest};
use crate::crypto::{Algorithm, certificate};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::{
    error::Error,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub struct WorkerClient {
    executable: PathBuf,
    module: PathBuf,
}

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl WorkerClient {
    pub fn for_module(module: &Path) -> Result<Option<Self>> {
        if !cfg!(all(windows, target_arch = "x86_64")) || pe_machine(module)? != 0x014c {
            return Ok(None);
        }
        let executable = std::env::current_exe()?.with_file_name("macsigner-token-x86.exe");
        if !executable.is_file() {
            return Err(
                "32-bit token requires macsigner-token-x86.exe beside the application".into(),
            );
        }
        if pe_machine(&executable)? != 0x014c {
            return Err("token worker is not a 32-bit Windows executable".into());
        }
        Ok(Some(Self::new(&executable, module)?))
    }

    pub fn new(executable: &Path, module: &Path) -> Result<Self> {
        for path in [executable, module] {
            if !path.is_absolute() || !path.is_file() {
                return Err(
                    "worker and module must be absolute paths to trusted existing files".into(),
                );
            }
        }
        Ok(Self {
            executable: executable.canonicalize()?,
            module: module.canonicalize()?,
        })
    }

    fn execute(
        &self,
        mode: &str,
        slot: Option<u64>,
        input: Option<Zeroizing<Vec<u8>>>,
        timeout: Duration,
        maximum_output: usize,
    ) -> Result<Vec<u8>> {
        let mut command = Command::new(&self.executable);
        command
            .arg(mode)
            .arg(&self.module)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(slot) = slot {
            command.arg(slot.to_string());
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut process = Process(command.spawn()?);
        if let Some(input) = input {
            let mut stdin = process.0.stdin.take().ok_or("missing worker input pipe")?;
            thread::spawn(move || {
                let _ = stdin.write_all(&input);
            });
        }
        let stdout = process
            .0
            .stdout
            .take()
            .ok_or("missing worker output pipe")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut output = Vec::new();
            let result = stdout
                .take(maximum_output as u64 + 1)
                .read_to_end(&mut output)
                .map(|_| output);
            let _ = sender.send(result);
        });
        let deadline = Instant::now() + timeout;
        let mut output = None;
        loop {
            if output.is_none() {
                match receiver.try_recv() {
                    Ok(result) => output = Some(result?),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("worker output reader failed".into());
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if output
                .as_ref()
                .is_some_and(|bytes| bytes.len() > maximum_output)
            {
                return Err("worker output exceeds protocol limit".into());
            }
            if let Some(status) = process.0.try_wait()? {
                if !status.success() {
                    return Err("token worker failed; no PIN retry was attempted".into());
                }
                if let Some(output) = output {
                    return Ok(output);
                }
            }
            if Instant::now() >= deadline {
                return Err("token worker timed out; no PIN retry was attempted".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn tokens(&self) -> Result<Value> {
        let output = self.execute(
            "probe-token",
            None,
            None,
            Duration::from_secs(8),
            4 * 1024 * 1024,
        )?;
        let value: Value = serde_json::from_slice(&output)?;
        if !value.is_array() {
            return Err("invalid token worker inventory".into());
        }
        Ok(value)
    }

    pub fn certificates(&self, slot: u64) -> Result<Value> {
        let output = self.execute(
            "probe-cert",
            Some(slot),
            None,
            Duration::from_secs(8),
            4 * 1024 * 1024,
        )?;
        let value: Value = serde_json::from_slice(&output)?;
        if !value.is_array() {
            return Err("invalid token worker certificate inventory".into());
        }
        Ok(value)
    }

    pub fn sign_digest(
        &self,
        slot: u64,
        key_id: &[u8],
        expected_certificate_der: &[u8],
        pin: Option<&str>,
        digest: &[u8],
        algorithm: Algorithm,
    ) -> Result<Vec<u8>> {
        let (name, length) = match algorithm {
            Algorithm::Sha1Rsa => ("sha1", 20),
            Algorithm::Sha256Rsa => ("sha256", 32),
            Algorithm::Sha384Rsa => ("sha384", 48),
            Algorithm::Sha512Rsa => ("sha512", 64),
        };
        if key_id.is_empty()
            || key_id.len() > 1024
            || digest.len() != length
            || pin.is_some_and(|pin| pin.is_empty() || pin.len() > 1024)
        {
            return Err("invalid worker signing parameters".into());
        }
        let summary = certificate::summary(expected_certificate_der)?;
        if summary["currently_valid"].as_bool() != Some(true) {
            return Err("selected certificate is outside its validity period".into());
        }
        let fingerprint = summary["fingerprint_sha256"]
            .as_str()
            .ok_or("missing certificate fingerprint")?;
        let request = SignDigestRequest {
            protocol: 1,
            slot,
            key_id_hex: hex::encode(key_id),
            certificate_sha256: fingerprint.into(),
            algorithm: name.into(),
            digest_base64: STANDARD.encode(digest),
            pin: pin.map(|pin| SecretPin::new(pin.to_owned())),
        };
        let body = Zeroizing::new(serde_json::to_vec(&request)?);
        drop(request);
        let mut frame = Zeroizing::new(Vec::new());
        worker_protocol::write_frame(&mut *frame, &body)?;
        drop(body);
        let output = self.execute(
            "sign-digest",
            None,
            Some(frame),
            Duration::from_secs(30),
            worker_protocol::MAX_FRAME_BYTES + 4,
        )?;
        let body = worker_protocol::read_frame(output.as_slice())?;
        let response: Value = serde_json::from_slice(&body)?;
        if response["protocol"].as_u64() != Some(1)
            || response["certificate_sha256"].as_str() != Some(fingerprint)
        {
            return Err("invalid worker signing response".into());
        }
        match response["error"].as_str() {
            Some("pin_required") => return Err("token requires PIN; no login was attempted".into()),
            Some("pin_locked") => return Err("token PIN is locked; no login was attempted".into()),
            Some(_) => return Err("invalid worker signing error".into()),
            None => {}
        }
        let signature = STANDARD.decode(
            response["signature_base64"]
                .as_str()
                .ok_or("missing worker signature")?,
        )?;
        certificate::verify_rsa_digest(expected_certificate_der, digest, &signature, algorithm)?;
        Ok(signature)
    }
}

pub(crate) fn pe_machine(path: &Path) -> Result<u16> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0; 64];
    file.read_exact(&mut header)?;
    if &header[..2] != b"MZ" {
        return Err("invalid Windows executable header".into());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into()?);
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut signature = [0; 6];
    file.read_exact(&mut signature)?;
    if &signature[..4] != b"PE\0\0" {
        return Err("invalid Windows PE signature".into());
    }
    Ok(u16::from_le_bytes(signature[4..6].try_into()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_cross_arch_worker_reads_without_pin() {
        let (Some(worker), Some(module)) = (
            std::env::var_os("MACSIGNER_TEST_WORKER"),
            std::env::var_os("MACSIGNER_TEST_MODULE"),
        ) else {
            return;
        };
        let client = WorkerClient::new(Path::new(&worker), Path::new(&module)).unwrap();
        assert_eq!(pe_machine(Path::new(&worker)).unwrap(), 0x014c);
        let tokens = client.tokens().unwrap();
        let token = tokens
            .as_array()
            .unwrap()
            .first()
            .expect("configured token must be present");
        let slot = token["slot"].as_u64().unwrap();
        let certificates = client.certificates(slot).unwrap();
        let certificate = certificates
            .as_array()
            .unwrap()
            .first()
            .expect("configured token must expose a public certificate");
        let der = STANDARD
            .decode(certificate["certificate_der_base64"].as_str().unwrap())
            .unwrap();
        let id = hex::decode(certificate["id_hex"].as_str().unwrap()).unwrap();
        crate::crypto::certificate::rsa_public_key(&der).unwrap();
        if token["login_required"].as_bool() == Some(true)
            && token["protected_authentication_path"].as_bool() == Some(false)
        {
            let error = client
                .sign_digest(slot, &id, &der, None, &[0; 32], Algorithm::Sha256Rsa)
                .unwrap_err();
            assert!(error.to_string().contains("requires PIN"));
        }
    }
}
