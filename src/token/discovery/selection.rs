use super::certificates;
use crate::token::pkcs11;
use std::{
    error::Error,
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

pub fn unique_identity() -> Result<(PathBuf, u64, Vec<u8>), Box<dyn Error>> {
    let list = certificates()?;
    let certs = list["certificates"]
        .as_array()
        .ok_or("invalid certificate list")?;
    if certs.is_empty() {
        return Err("no signing certificate was found; run reader-list and token-list".into());
    }
    let index = if certs.len() == 1 {
        0
    } else {
        if !io::stdin().is_terminal() {
            return Err(format!(
                "found {} certificates; run this command in a terminal to choose one",
                certs.len()
            )
            .into());
        }
        eprintln!("Choose the certificate for this signature:");
        for (i, cert) in certs.iter().enumerate() {
            eprintln!(
                "  {}. {} | expires {} | serial {}",
                i + 1,
                cert["parsed"]["subject"]
                    .as_str()
                    .unwrap_or("unknown subject"),
                cert["parsed"]["not_after"].as_str().unwrap_or("unknown"),
                cert["parsed"]["serial_decimal"]
                    .as_str()
                    .unwrap_or("unknown")
            );
        }
        eprint!("Certificate number: ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        let selected = answer.trim().parse::<usize>()?;
        if selected == 0 || selected > certs.len() {
            return Err("invalid certificate number".into());
        }
        selected - 1
    };
    let cert = &certs[index];
    let module = PathBuf::from(cert["module"].as_str().ok_or("missing module path")?);
    let slot = cert["slot"].as_u64().ok_or("missing token slot")?;
    let id = pkcs11::decode_hex(cert["id_hex"].as_str().ok_or("missing certificate ID")?)?;
    Ok((module, slot, id))
}
