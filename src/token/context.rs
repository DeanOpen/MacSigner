// Shared PKCS#11 contexts. ESIGN and CTSIGN may open the same vendor module in one
// process; a second C_Initialize fails and dropping a context calls C_Finalize,
// which would invalidate the other module's sessions. Keep one context per path.
use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

fn contexts() -> &'static Mutex<HashMap<PathBuf, Pkcs11>> {
    static CONTEXTS: OnceLock<Mutex<HashMap<PathBuf, Pkcs11>>> = OnceLock::new();
    CONTEXTS.get_or_init(Default::default)
}

pub fn context(module: &Path) -> Result<Pkcs11, Box<dyn std::error::Error + Send + Sync>> {
    if !module.is_absolute() || !module.is_file() {
        return Err("PKCS#11 module must be an absolute path to an existing vendor library".into());
    }
    let key = module.canonicalize()?;
    let mut contexts = contexts()
        .lock()
        .map_err(|_| "PKCS#11 registry lock poisoned")?;
    if let Some(ctx) = contexts.get(&key) {
        return Ok(ctx.clone());
    }
    #[cfg(windows)]
    let _library = super::loader::WindowsModule::load(&key)?;
    let ctx = Pkcs11::new(&key)?;
    ctx.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK))?;
    contexts.insert(key, ctx.clone());
    Ok(ctx)
}
