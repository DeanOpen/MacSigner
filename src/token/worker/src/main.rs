#[path = "../../context.rs"]
mod context;
#[path = "../../../crypto/mod.rs"]
pub mod crypto;
#[path = "../../loader.rs"]
mod loader;
#[path = "../../pkcs11.rs"]
pub mod pkcs11;
#[path = "../../worker_protocol.rs"]
pub mod worker_protocol;

pub mod token {
    pub use crate::context::context;
}

use base64::{Engine, engine::general_purpose::STANDARD};
use std::{error::Error, io::Write, path::PathBuf};

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mode = arguments.next().ok_or("missing worker operation")?;
    if mode == "version" {
        if arguments.next().is_some() {
            return Err("unexpected worker arguments".into());
        }
        println!(
            "{}",
            serde_json::json!({"protocol": 1, "architecture": std::env::consts::ARCH,
                "capabilities": ["probe-token", "probe-cert", "sign-digest"]})
        );
        return Ok(());
    }
    if mode != "probe-token" && mode != "probe-cert" && mode != "sign-digest" {
        return Err("unsupported worker operation".into());
    }
    let module = PathBuf::from(arguments.next().ok_or("missing module path")?);
    if !module.is_absolute() || !module.is_file() {
        return Err("module must be an absolute path to an existing library".into());
    }
    let slot = if mode == "probe-cert" {
        Some(
            arguments
                .next()
                .ok_or("missing token slot")?
                .to_str()
                .ok_or("invalid token slot")?
                .parse::<u64>()?,
        )
    } else {
        None
    };
    if arguments.next().is_some() {
        return Err("unexpected worker arguments".into());
    }
    if mode == "sign-digest" {
        let input = worker_protocol::read_frame(std::io::stdin().lock())?;
        let mut request: worker_protocol::SignDigestRequest =
            serde_json::from_slice(&input).map_err(|_| "invalid worker signing request")?;
        drop(input);
        if request.protocol != 1
            || request.key_id_hex.len() > 2048
            || request.certificate_sha256.len() != 64
            || !request
                .certificate_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || request
                .pin
                .as_ref()
                .is_some_and(|pin| pin.expose().is_empty() || pin.expose().len() > 1024)
        {
            return Err("invalid worker signing parameters".into());
        }
        let algorithm = crypto::Algorithm::parse(&request.algorithm)?;
        let id = pkcs11::decode_hex(&request.key_id_hex)?;
        let digest = STANDARD.decode(&request.digest_base64)?;
        let context = pkcs11::open(&module)?;
        let pin = request
            .pin
            .take()
            .map(|mut pin| cryptoki::types::AuthPin::new(pin.take().into()));
        let signed = pkcs11::sign_digest(
            &context,
            request.slot,
            &id,
            pin,
            &digest,
            algorithm,
            &request.certificate_sha256,
        );
        let result = match signed {
            Ok(signature) => serde_json::json!({
                "protocol": 1, "certificate_sha256": request.certificate_sha256,
                "signature_base64": STANDARD.encode(signature)
            }),
            Err(error) => {
                let code = match error.downcast_ref::<pkcs11::AuthenticationError>() {
                    Some(pkcs11::AuthenticationError::PinRequired) => "pin_required",
                    Some(pkcs11::AuthenticationError::PinLocked) => "pin_locked",
                    None => return Err(error),
                };
                serde_json::json!({"protocol": 1, "certificate_sha256": request.certificate_sha256, "error": code})
            }
        };
        let response = serde_json::to_vec(&result)?;
        worker_protocol::write_frame(std::io::stdout().lock(), &response)?;
        return Ok(());
    }
    let context = pkcs11::open(&module)?;
    let result = match slot {
        Some(slot) => pkcs11::certificates(&context, slot)?,
        None => pkcs11::tokens(&context)?,
    };
    let output = serde_json::to_vec(&result)?;
    if output.len() > 4 * 1024 * 1024 {
        return Err("worker output exceeds protocol limit".into());
    }
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("token worker: {error}");
        std::process::exit(1);
    }
}
