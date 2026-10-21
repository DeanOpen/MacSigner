use serde_json::Value;
use std::{
    error::Error,
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn check_module_architecture(module: &Path) -> Result<(), Box<dyn Error>> {
    if cfg!(target_os = "macos") {
        let architectures = super::macho::architectures(&mut fs::File::open(module)?)?;
        if !architectures
            .iter()
            .any(|cpu| super::macho::matches(*cpu, std::env::consts::ARCH))
        {
            return Err(format!(
                "PKCS#11 architecture mismatch: MacSigner {}; module CPUs {architectures:x?}. An ARM64 process cannot load an Intel-only module; use a compatible connector or matching executable",
                std::env::consts::ARCH
            ).into());
        }
        return Ok(());
    }
    if !cfg!(windows) {
        return Ok(());
    }
    let mut file = fs::File::open(module)?;
    let mut header = [0_u8; 64];
    file.read_exact(&mut header)?;
    if &header[..2] != b"MZ" {
        return Err("PKCS#11 module is not a Windows PE library".into());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into()?);
    file.seek(SeekFrom::Start(offset.into()))?;
    let mut pe = [0_u8; 6];
    file.read_exact(&mut pe)?;
    if &pe[..4] != b"PE\0\0" {
        return Err("Invalid Windows PE signature".into());
    }
    let machine = u16::from_le_bytes(pe[4..6].try_into()?);
    if !machine_matches(machine, std::env::consts::ARCH) {
        return Err(format!(
            "PKCS#11 architecture mismatch: PE machine 0x{machine:04x}, MacSigner {}; use matching vendor middleware and executable",
            std::env::consts::ARCH
        ).into());
    }
    Ok(())
}

fn machine_matches(machine: u16, architecture: &str) -> bool {
    matches!(
        (machine, architecture),
        (0x014c, "x86") | (0x8664, "x86_64") | (0xaa64, "aarch64")
    )
}

/// Run untrusted vendor middleware in a child, so a broken DLL cannot crash the parent.
pub(super) fn probe(module: &Path, mode: &str, slot: Option<u64>) -> Result<Value, Box<dyn Error>> {
    if let Some(worker) = crate::token::remote::WorkerClient::for_module(module)? {
        return match mode {
            "probe-token" => worker.tokens(),
            "probe-cert" => worker.certificates(slot.ok_or("missing token slot")?),
            _ => Err("unsupported token probe operation".into()),
        };
    }
    check_module_architecture(module)?;
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg(mode)
        .arg(module)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(slot) = slot {
        cmd.arg(slot.to_string());
    }
    #[cfg(windows)]
    {
        // Probes are console processes; without this each one flashes a window.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn()?;
    let mut stdout = child.stdout.take().ok_or("missing probe output")?;
    let reader = thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut output = Vec::new();
        stdout
            .by_ref()
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut output)?;
        io::copy(&mut stdout, &mut io::sink())?;
        Ok(output)
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            let _ = reader.join();
            return Err("PKCS#11 probe timed out".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    let output = reader.join().map_err(|_| "PKCS#11 probe reader failed")??;
    if !status.success() {
        return Err("PKCS#11 probe failed".into());
    }
    if output.len() > 4 * 1024 * 1024 {
        return Err("PKCS#11 probe output too large".into());
    }
    Ok(serde_json::from_slice(&output)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pe_architecture_must_match_the_running_process() {
        assert!(machine_matches(0x8664, "x86_64"));
        assert!(machine_matches(0x014c, "x86"));
        assert!(machine_matches(0xaa64, "aarch64"));
        assert!(!machine_matches(0x014c, "x86_64"));
        assert!(!machine_matches(0x8664, "aarch64"));
        assert!(!machine_matches(0, "x86_64"));
    }
}
