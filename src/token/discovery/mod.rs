mod catalog;
mod macho;
mod paths;
mod probe;
mod selection;

pub use paths::modules;
pub use selection::unique_identity;

use probe::probe;
use serde_json::{Value, json};
use std::{error::Error, path::Path};

pub fn tokens() -> Value {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    for module in modules() {
        match probe(&module, "probe-token", None) {
            Ok(Value::Array(found)) => {
                for mut token in found {
                    if let Some(object) = token.as_object_mut() {
                        object.insert("module".into(), json!(module));
                    }
                    tokens.push(token);
                }
            }
            Ok(_) => errors.push(json!({"module": module, "error": "invalid probe result"})),
            Err(err) => errors.push(json!({"module": module, "error": err.to_string()})),
        }
    }
    json!({"tokens": tokens, "module_errors": errors})
}

pub fn certificates() -> Result<Value, Box<dyn Error>> {
    let list = tokens();
    let tokens = list["tokens"].as_array().ok_or("invalid token list")?;
    let mut certs = Vec::new();
    let mut errors = Vec::new();
    for token in tokens {
        let Some(module) = token["module"].as_str() else {
            continue;
        };
        let Some(slot) = token["slot"].as_u64() else {
            continue;
        };
        match probe(Path::new(module), "probe-cert", Some(slot)) {
            Ok(Value::Array(found)) => {
                for mut cert in found {
                    if let Some(object) = cert.as_object_mut() {
                        object.insert("module".into(), json!(module));
                        object.insert("slot".into(), json!(slot));
                    }
                    certs.push(cert);
                }
            }
            Ok(_) => errors
                .push(json!({"module": module, "slot": slot, "error": "invalid probe result"})),
            Err(err) => {
                errors.push(json!({"module": module, "slot": slot, "error": err.to_string()}))
            }
        }
    }
    Ok(
        json!({"tokens": list["tokens"], "certificates": certs, "token_errors": errors, "module_errors": list["module_errors"]}),
    )
}

pub fn diagnostics() -> Result<Value, Box<dyn Error>> {
    let readers = match crate::token::reader::readers() {
        Ok(readers) => json!({"readers": readers}),
        Err(error) => json!({"error": error.to_string()}),
    };
    let certificates = certificates()?;
    Ok(json!({
        "architecture": std::env::consts::ARCH,
        "portable_module_directory": std::env::current_exe()?.parent().map(|path| path.join("pkcs11")),
        "reader_inventory": readers,
        "certificate_count": certificates["certificates"].as_array().map_or(0, Vec::len),
        "module_errors": certificates["module_errors"],
        "token_errors": certificates["token_errors"],
    }))
}
