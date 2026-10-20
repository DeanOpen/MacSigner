use crate::esign::{
    discovery, legacy, native,
    online::{self, Algorithm},
    pkcs11, protocol, reader, websocket,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use cryptoki::types::AuthPin;
use std::{
    error::Error,
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
};

const HELP: &str = "ESIGN (eSigner) Rust core.
Usage:
  macsigner esign capabilities
  macsigner esign reader-list
  macsigner esign module-list
  macsigner esign token-list
  macsigner esign cert-list
  macsigner esign token-diagnostics
  macsigner esign sign-file INPUT OUTPUT
  macsigner esign sign-base64 sha256|sha384|sha512 BASE64_VALUE
  macsigner esign sign-base64-list sha256 COMMA_SEPARATED_VALUES
  macsigner esign sign-request INPUT_JSON OUTPUT_JSON [--allow-sha1]
  macsigner esign native
  macsigner esign websocket [--port 8768] [--origin https://thuedientu.gdt.gov.vn]...
  macsigner esign token-list ABSOLUTE_MODULE_PATH
  macsigner esign token-mechanisms ABSOLUTE_MODULE_PATH SLOT
  macsigner esign cert-list ABSOLUTE_MODULE_PATH SLOT
  macsigner esign sign-file ABSOLUTE_MODULE_PATH SLOT KEY_ID_HEX INPUT OUTPUT
  macsigner esign sign-base64 ABSOLUTE_MODULE_PATH SLOT KEY_ID_HEX sha256|sha384|sha512 BASE64_VALUE
  macsigner esign sign-base64 ABSOLUTE_MODULE_PATH SLOT KEY_ID_HEX sha1 BASE64_VALUE --allow-sha1
  macsigner esign sign-base64-list ABSOLUTE_MODULE_PATH SLOT KEY_ID_HEX sha256 COMMA_SEPARATED_VALUES
  macsigner esign sign-request ABSOLUTE_MODULE_PATH SLOT KEY_ID_HEX INPUT_JSON OUTPUT_JSON [--allow-sha1]

sign-file prompts for a PIN and produces a raw RSA/SHA-256 signature.
It does not produce a signed XML/PDF document. OUTPUT must not already exist.
Browser requests never load module paths or perform raw signing. The native
and websocket commands here answer FALSE to every signing command: browser
signing (CER_001, ONLINE_XML_CER_001, ONLINE_SUBMIT_SIGNING_001) needs the
approval prompt of the MacSigner app (app/).
WebSocket defaults to loopback 8768, falling back to 8769 if occupied.
Native messaging writes only length-prefixed JSON to stdout.";

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    // Without module/slot/key arguments, find the token certificate automatically.
    let mut args = args.to_vec();
    let implicit_auto = matches!(
        args.first().map(String::as_str),
        Some("sign-file" | "sign-base64" | "sign-base64-list" | "sign-request")
    ) && (args.len() == 3 || (args.len() == 4 && args[3] == "--allow-sha1"));
    if implicit_auto {
        let (module, slot, id) = discovery::unique_identity()?;
        let mut expanded = vec![
            args[0].clone(),
            module.to_string_lossy().into_owned(),
            slot.to_string(),
            pkcs11::encode_hex(&id),
        ];
        expanded.extend(args.drain(1..));
        args = expanded;
    }
    let args = args.as_slice();
    match args.first().map(String::as_str) {
        Some("reader-list") if args.len() == 1 => {
            println!("{}", serde_json::to_string_pretty(&reader::readers()?)?)
        }
        Some("module-list") if args.len() == 1 => {
            println!("{}", serde_json::to_string_pretty(&discovery::modules())?)
        }
        Some("token-diagnostics") if args.len() == 1 => {
            println!(
                "{}",
                serde_json::to_string_pretty(&discovery::diagnostics()?)?
            )
        }
        Some("token-list") if args.len() == 1 => {
            println!("{}", serde_json::to_string_pretty(&discovery::tokens())?)
        }
        Some("cert-list") if args.len() == 1 => {
            println!(
                "{}",
                serde_json::to_string_pretty(&discovery::certificates()?)?
            )
        }
        // Child-process probes used by discovery, so a faulty vendor DLL cannot
        // crash the caller.
        Some("probe-token") if args.len() == 2 => {
            println!("{}", pkcs11::tokens(&pkcs11::open(Path::new(&args[1]))?)?)
        }
        Some("probe-cert") if args.len() == 3 => {
            println!(
                "{}",
                pkcs11::certificates(&pkcs11::open(Path::new(&args[1]))?, args[2].parse()?)?
            )
        }
        _ => run_explicit(args)?,
    }
    Ok(())
}

fn run_explicit(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args.first().map(String::as_str) {
        Some("--help" | "-h") | None => println!("{HELP}"),
        Some("capabilities") if args.len() == 1 => println!(
            "{}",
            serde_json::to_string_pretty(&protocol::capabilities())?
        ),
        Some("native") if args.len() == 1 => {
            native::serve(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())?
        }
        Some("websocket") => {
            let mut port = 8768;
            let mut fallback = Some(8769);
            let mut origins = vec![];
            let mut i = 1;
            while i < args.len() {
                let value = args.get(i + 1).ok_or("missing option value")?;
                match args[i].as_str() {
                    "--port" => {
                        port = value.parse::<u16>()?;
                        fallback = None;
                    }
                    "--origin" => {
                        websocket::validate_origin(value)?;
                        origins.push(value.clone());
                    }
                    _ => return Err("unknown websocket option".into()),
                }
                i += 2;
            }
            if origins.is_empty() {
                origins.push(websocket::DEFAULT_ORIGIN.into());
            }
            websocket::serve(websocket::bind(port, fallback)?, origins)?;
        }
        Some("token-list") if args.len() == 2 => {
            println!("{}", pkcs11::tokens(&pkcs11::open(Path::new(&args[1]))?)?)
        }
        Some("token-mechanisms") if args.len() == 3 => println!(
            "{}",
            pkcs11::signing_mechanisms(&pkcs11::open(Path::new(&args[1]))?, args[2].parse()?)?
        ),
        Some("cert-list") if args.len() == 3 => println!(
            "{}",
            pkcs11::certificates(&pkcs11::open(Path::new(&args[1]))?, args[2].parse()?)?
        ),
        Some("sign-file") if args.len() == 6 => {
            let id = pkcs11::decode_hex(&args[3])?;
            let slot = args[2].parse()?;
            let mut data = vec![];
            std::fs::File::open(&args[4])?
                .take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut data)?;
            if data.len() > 16 * 1024 * 1024 {
                return Err("input exceeds 16 MiB".into());
            }
            // Reserve output before asking the token to sign; never overwrite a file.
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[5])?;
            let result = (|| {
                let ctx = pkcs11::open(Path::new(&args[1]))?;
                pkcs11::ensure_signing_supported(&ctx, slot, Algorithm::Sha256Rsa)?;
                let pin = AuthPin::new(rpassword::prompt_password("Token PIN: ")?.into());
                let signature = pkcs11::sign(&ctx, slot, &id, pin, &data)?;
                output.write_all(&signature)?;
                output.sync_all()?;
                Ok::<(), Box<dyn Error>>(())
            })();
            drop(output);
            if let Err(e) = result {
                let _ = std::fs::remove_file(&args[5]);
                return Err(e);
            }
        }
        Some("sign-base64" | "sign-base64-list") if args.len() == 6 || args.len() == 7 => {
            let algorithm = Algorithm::parse(&args[4])?;
            if algorithm == Algorithm::Sha1Rsa
                && args.get(6).map(String::as_str) != Some("--allow-sha1")
            {
                return Err("SHA-1 compatibility requires --allow-sha1".into());
            }
            if args.len() == 7 && (algorithm != Algorithm::Sha1Rsa || args[6] != "--allow-sha1") {
                return Err("unexpected signing option".into());
            }
            let values = if args[0] == "sign-base64" {
                vec![args[5].as_str()]
            } else {
                args[5].split(',').collect::<Vec<_>>()
            };
            if values.is_empty() || values.len() > 100 {
                return Err("invalid signature input count".into());
            }
            let payloads = values
                .iter()
                .map(|v| online::decode_input(v))
                .collect::<Result<Vec<_>, _>>()?;
            let ctx = pkcs11::open(Path::new(&args[1]))?;
            let slot = args[2].parse()?;
            let id = pkcs11::decode_hex(&args[3])?;
            pkcs11::ensure_signing_supported(&ctx, slot, algorithm)?;
            let pin = AuthPin::new(rpassword::prompt_password("Token PIN: ")?.into());
            let signatures = pkcs11::sign_many(&ctx, slot, &id, pin, &payloads, algorithm)?;
            let output = signatures
                .iter()
                .map(|signature| STANDARD.encode(signature))
                .collect::<Vec<_>>()
                .join(",");
            println!("{output}");
        }
        Some("sign-request") if args.len() == 6 || args.len() == 7 => {
            if args.len() == 7 && args[6] != "--allow-sha1" {
                return Err("unexpected signing option".into());
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&args[4])?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1024 * 1024 {
                return Err("request exceeds 1 MiB".into());
            }
            let request: serde_json::Value = serde_json::from_slice(&bytes)?;
            // Validate before opening the module or asking for a token PIN.
            protocol::validate(&request)?;
            let code = request["CODE"].as_str().unwrap_or("");
            if !matches!(
                code.to_ascii_uppercase().as_str(),
                "CER_001" | "ONLINE_XML_CER_001" | "ONLINE_SUBMIT_SIGNING_001"
            ) {
                return Err("this legacy command is not supported by sign-request".into());
            }
            let slot = args[2].parse()?;
            let id = pkcs11::decode_hex(&args[3])?;
            let ctx = pkcs11::open(Path::new(&args[1]))?;
            let cert = pkcs11::certificate_by_id(&ctx, slot, &id)?;
            let response =
                legacy::sign_request(&request, &cert, args.len() == 7, |payloads, algorithm| {
                    pkcs11::ensure_signing_supported(&ctx, slot, algorithm)?;
                    let pin = AuthPin::new(rpassword::prompt_password("Token PIN: ")?.into());
                    pkcs11::sign_many(&ctx, slot, &id, pin, payloads, algorithm)
                })?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[5])?;
            serde_json::to_writer_pretty(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.sync_all()?;
        }
        _ => return Err(format!("invalid arguments\n{HELP}").into()),
    }
    Ok(())
}
