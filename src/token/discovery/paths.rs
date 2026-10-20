use super::catalog::{candidate_name, known_module_paths, portable_candidate};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

fn visit(root: &Path, depth: usize, portable: bool, found: &mut BTreeSet<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() {
            if path
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(candidate_name)
                || (portable && portable_candidate(&path))
            {
                found.insert(path);
            }
        } else if kind.is_dir() && !kind.is_symlink() {
            visit(&path, depth - 1, portable, found);
        }
    }
}

#[cfg(windows)]
fn roots() -> Vec<(PathBuf, usize)> {
    let mut roots = Vec::new();
    if let Some(windows) = std::env::var_os("SystemRoot") {
        let windows = PathBuf::from(windows);
        roots.extend(windows_system_roots(&windows));
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(key) {
            roots.push((PathBuf::from(value), 5));
        }
    }
    roots
}

#[cfg(windows)]
fn windows_system_roots(windows: &Path) -> [(PathBuf, usize); 2] {
    [(windows.join("System32"), 1), (windows.join("SysWOW64"), 1)]
}

#[cfg(not(windows))]
fn roots() -> Vec<(PathBuf, usize)> {
    let mut roots = vec![
        (PathBuf::from("/usr/lib"), 5),
        (PathBuf::from("/usr/local/lib"), 4),
        (PathBuf::from("/lib"), 4),
        (PathBuf::from("/opt"), 6),
    ];
    if cfg!(target_os = "macos") {
        roots.extend([
            (PathBuf::from("/Library"), 6),
            (PathBuf::from("/opt/homebrew/lib"), 3),
        ]);
    }
    roots
}

#[cfg(not(windows))]
fn registered_modules(found: &mut BTreeSet<PathBuf>) {
    let mut dirs = vec![
        PathBuf::from("/etc/pkcs11/modules"),
        PathBuf::from("/usr/share/p11-kit/modules"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config/pkcs11/modules"));
    }
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|v| v != "module") {
                continue;
            }
            let Ok(config) = fs::read_to_string(path) else {
                continue;
            };
            for line in config.lines() {
                let line = line.trim();
                if let Some(value) = line.strip_prefix("module:") {
                    let value = value.trim();
                    let module = PathBuf::from(value);
                    if module.is_absolute() && module.is_file() {
                        found.insert(module);
                    }
                }
            }
        }
    }
}

pub fn modules() -> Vec<PathBuf> {
    let mut found = BTreeSet::new();
    for path in known_module_paths().map(PathBuf::from) {
        if path.is_absolute() && path.is_file() {
            found.insert(path);
        }
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        visit(&directory.join("pkcs11"), 4, true, &mut found);
    }
    if let Some(paths) = std::env::var_os("MACSIGNER_PKCS11_MODULES") {
        for path in std::env::split_paths(&paths) {
            if path.is_absolute() && path.is_file() {
                found.insert(path);
            }
        }
    }
    for (root, depth) in roots() {
        visit(&root, depth, false, &mut found);
    }
    #[cfg(not(windows))]
    registered_modules(&mut found);
    found.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_discovery_includes_both_system_architectures() {
        let windows = Path::new(r"C:\Windows");
        assert_eq!(
            windows_system_roots(windows),
            [
                (PathBuf::from(r"C:\Windows\System32"), 1),
                (PathBuf::from(r"C:\Windows\SysWOW64"), 1),
            ]
        );
    }

    #[test]
    fn portable_discovery_finds_unknown_vendor_names_without_loading_dependencies() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("macsigner-modules-{}-{unique}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let extension = if cfg!(windows) {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        let module = directory.join(format!("unknown-vendor.{extension}"));
        let dependency = directory.join(format!("dependency.{extension}"));
        fs::write(&module, b"fixture\0C_GetFunctionList\0").unwrap();
        fs::write(&dependency, b"fixture without PKCS11 entrypoint").unwrap();
        let mut found = BTreeSet::new();
        visit(&directory, 4, false, &mut found);
        assert!(found.is_empty());
        visit(&directory, 4, true, &mut found);
        assert_eq!(found.into_iter().collect::<Vec<_>>(), vec![module.clone()]);
        fs::remove_file(module).unwrap();
        fs::remove_file(dependency).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
