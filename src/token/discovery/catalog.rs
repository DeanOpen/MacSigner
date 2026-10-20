use serde_json::Value;
use std::{fs, io::Read, path::Path, sync::OnceLock};

const MODULE_NAMES: &str = include_str!("../data/token-modules.json");

fn module_names() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| serde_json::from_str(MODULE_NAMES).expect("static module catalog"))
}

pub(super) fn known_module_paths() -> impl Iterator<Item = &'static str> {
    module_names()["macos_linux_module_paths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

pub(super) fn candidate_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if !library_name(&name) {
        return false;
    }
    if name == "j2pkcs11.dll" {
        return false;
    }
    let legacy = module_names();
    legacy["windows_module_names"]
        .as_array()
        .is_some_and(|names| {
            names
                .iter()
                .any(|item| item.as_str().is_some_and(|v| v.eq_ignore_ascii_case(&name)))
        })
        || [
            "pkcs11",
            "csp11",
            "opensc",
            "softhsm",
            "coolkey",
            "p11-kit",
            "_p11",
            "p11_",
            "p11v",
            "viettel",
            "vnpt",
            "fpt-ca",
            "fptca",
            "bkav",
            "newca",
            "etoken",
            "etpkcs",
            "safe-ca",
            "vina-ca",
            "shuttlecsp",
            "gclib",
            "psapkcs",
            "hilo",
            "misa",
            "ca2_",
            "eca_",
        ]
        .iter()
        .any(|part| name.contains(part))
        || name.starts_with("epass")
        || name.starts_with("libepass")
}

fn library_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let extension = if cfg!(windows) {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    };
    name.ends_with(extension) || name.contains(&format!("{extension}."))
}

pub(super) fn portable_candidate(path: &Path) -> bool {
    if !path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(library_name)
    {
        return false;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut data = Vec::new();
    if file.take(32 * 1024 * 1024).read_to_end(&mut data).is_err() {
        return false;
    }
    data.windows(b"C_GetFunctionList\0".len())
        .any(|bytes| bytes == b"C_GetFunctionList\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_provider_modules_have_explicit_catalog_entries() {
        let names = module_names()["windows_module_names"].as_array().unwrap();
        for name in [
            "fptca_v3.dll",
            "fptca_v4.dll",
            "fptca_v464.dll",
            "hiloca_csp11_v1.dll",
            "trustca_csp11_v1.dll",
            "mfca_v1.dll",
            "bit4ipki.dll",
            "bit4npki.dll",
            "bit4opki.dll",
            "bit4xpki.dll",
            "dkck201.dll",
        ] {
            assert!(names.iter().any(|value| value.as_str() == Some(name)));
        }
    }

    #[test]
    fn observed_vendor_bundle_path_is_in_the_catalog() {
        assert!(known_module_paths().any(|path| {
            path == "/Applications/VNPT-CA Token Manager.app/Contents/Resources/libvnptca_p11_v8.dylib"
        }));
    }

    #[test]
    fn middleware_names_cover_legacy_and_newly_observed_token() {
        for name in [
            "st3csp11.dll",
            "ca2_ace_csp11.dll",
            "opensc-pkcs11.dll",
            "HILO-CA.dll",
            "MISA-CA.dll",
            "LCS-CAv2.0.dll",
            "one-ca_v1.dll",
            "fptca_v3.dll",
            "fptca_v4.dll",
            "fptca_v464.dll",
            "hiloca_csp11_v1.dll",
            "trustca_csp11_v1.dll",
            "mfca_v1.dll",
            "nca_v5.1.dll",
            "fca_v1.dll",
        ] {
            if cfg!(windows) {
                assert!(candidate_name(name), "missing middleware candidate: {name}");
            }
        }
        assert!(!candidate_name("some-unrelated.dll"));
        assert!(!candidate_name("Windows.UI.PicturePassword.dll"));
        assert!(!candidate_name("j2pkcs11.dll"));
    }
}
