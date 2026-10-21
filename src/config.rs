// Per-module settings: only on/off and ports. Certificates are found\n// automatically; keys, PINs and passwords are never stored here.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EsignSettings {
    /// Start the WebSocket service when the application opens.
    pub enabled: bool,
    pub port: u16,
    /// Used only when `port` is occupied; `None` disables fallback.
    pub fallback_port: Option<u16>,
    pub origins: Vec<String>,
}

impl Default for EsignSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8768,
            fallback_port: Some(8769),
            origins: vec![crate::esign::websocket::DEFAULT_ORIGIN.into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CtsignSettings {
    /// Start the loopback HTTP API when the application opens.
    pub enabled: bool,
    pub port: u16,
}

impl Default for CtsignSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: crate::ctsign::DEFAULT_PORT,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub esign: EsignSettings,
    pub ctsign: CtsignSettings,
    pub vnpt: VnptSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VnptSettings {
    pub enabled: bool,
    pub port: u16,
}

impl Default for VnptSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: crate::vnpt::protocol::PORTS[0],
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.esign.port == 0 || self.ctsign.port == 0 || self.vnpt.port == 0 {
            return Err("Cổng phải từ 1 đến 65535".into());
        }
        if self.esign.fallback_port == Some(0) {
            return Err("Cổng dự phòng ESIGN phải từ 1 đến 65535".into());
        }
        if self.esign.origins.is_empty() {
            return Err("ESIGN cần ít nhất một origin được phép".into());
        }
        for origin in &self.esign.origins {
            crate::esign::websocket::validate_origin(origin)?;
        }
        let ports = [
            Some(self.esign.port),
            self.esign.fallback_port,
            Some(self.ctsign.port),
        ];
        if ports[2] == ports[0] || ports[2] == ports[1] {
            return Err("ESIGN và CTSIGN không được dùng chung cổng".into());
        }
        if ports.contains(&Some(self.vnpt.port)) {
            return Err("VNPT không được dùng chung cổng với ESIGN hoặc CTSIGN".into());
        }
        if self.vnpt.enabled {
            crate::vnpt::ensure_available()?;
        }

        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        // Write then rename so a crash never leaves a truncated settings file.
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

/// `MACSIGNER_CONFIG` overrides the per-user location.
pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MACSIGNER_CONFIG") {
        return PathBuf::from(path);
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("MacSigner")
        .join("settings.json")
}
