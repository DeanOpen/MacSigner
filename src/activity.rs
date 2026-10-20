// Activity history: PDF signing and every call to the ESIGN (WebSocket) and
// CTSIGN (HTTP) services. Entries describe what happened, who asked and the
// outcome; payloads, hashes, PINs and certificate material are never recorded.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::Mutex,
    time::Instant,
};

/// Entries kept in memory and, after compaction, on disk.
const KEEP: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Signing started from the desktop UI.
    App,
    /// eSigner-compatible WebSocket plugin.
    Esign,
    /// CTSigningHub-compatible HTTP API.
    Ctsign,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Local time, RFC 3339.
    pub time: String,
    pub channel: Channel,
    /// Command or endpoint, e.g. "Ký PDF", "HubClient/signPdf", "CER_001".
    pub action: String,
    /// Who asked: web origin, or empty for the desktop UI.
    pub client: String,
    /// Document path or request id.
    pub target: String,
    /// Certificate serial.
    pub certificate: String,
    /// Signer name, when known.
    pub signer: String,
    /// True when this call produced (or tried to produce) a signature.
    pub signing: bool,
    pub ok: bool,
    /// Protocol status: HTTP/API status code, or TRUE/FALSE for ESIGN.
    pub status: String,
    pub message: String,
    pub duration_ms: u64,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            id: 0,
            time: String::new(),
            channel: Channel::App,
            action: String::new(),
            client: String::new(),
            target: String::new(),
            certificate: String::new(),
            signer: String::new(),
            signing: false,
            ok: false,
            status: String::new(),
            message: String::new(),
            duration_ms: 0,
        }
    }
}

struct Log {
    path: Option<PathBuf>,
    entries: VecDeque<Entry>,
    next_id: u64,
    /// Lines in the file, to know when to compact it.
    lines: usize,
}

static LOG: Mutex<Log> = Mutex::new(Log {
    path: None,
    entries: VecDeque::new(),
    next_id: 1,
    lines: 0,
});

fn log() -> std::sync::MutexGuard<'static, Log> {
    LOG.lock().unwrap_or_else(|e| e.into_inner())
}

/// Persist entries to a JSON Lines file and load what it already holds.
/// Without this call the history lives in memory only (CLI, tests).
pub fn open(path: PathBuf) {
    let mut entries = VecDeque::new();
    let mut lines = 0;
    if let Ok(file) = std::fs::File::open(&path) {
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            lines += 1;
            if let Ok(entry) = serde_json::from_str::<Entry>(&line) {
                if entries.len() == KEEP {
                    entries.pop_front();
                }
                entries.push_back(entry);
            }
        }
    }
    let mut log = log();
    log.next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    log.entries = entries;
    log.lines = lines;
    log.path = Some(path);
}

fn write_all(log: &Log) {
    let Some(path) = &log.path else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut body = Vec::new();
    for entry in &log.entries {
        if serde_json::to_writer(&mut body, entry).is_ok() {
            body.push(b'\n');
        }
    }
    let _ = std::fs::write(path, body);
}

pub fn record(mut entry: Entry) {
    entry.time = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let mut log = log();
    entry.id = log.next_id;
    log.next_id += 1;
    if log.entries.len() == KEEP {
        log.entries.pop_front();
    }
    log.entries.push_back(entry.clone());
    if log.lines >= KEEP * 2 {
        write_all(&log);
        log.lines = log.entries.len();
    } else if let Some(path) = log.path.clone() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| {
                let mut line = serde_json::to_vec(&entry).map_err(std::io::Error::other)?;
                line.push(b'\n');
                f.write_all(&line)
            });
        if appended.is_ok() {
            log.lines += 1;
        }
    }
}

/// Newest first.
pub fn entries() -> Vec<Entry> {
    log().entries.iter().rev().cloned().collect()
}

pub fn clear() {
    let mut log = log();
    log.entries.clear();
    log.lines = 0;
    write_all(&log);
}

/// Measures one call from start to finish and records it.
pub struct Call {
    started: Instant,
    entry: Entry,
}

impl Call {
    pub fn start(channel: Channel, action: impl Into<String>, client: impl Into<String>) -> Self {
        Self {
            started: Instant::now(),
            entry: Entry {
                channel,
                action: clip(action.into()),
                client: clip(client.into()),
                ..Entry::default()
            },
        }
    }

    pub fn signing(mut self, signing: bool) -> Self {
        self.entry.signing = signing;
        self
    }

    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.entry.action = clip(action.into());
        self
    }

    pub fn client(mut self, client: impl Into<String>) -> Self {
        self.entry.client = clip(client.into());
        self
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.entry.target = clip(target.into());
        self
    }

    pub fn certificate(mut self, certificate: impl Into<String>) -> Self {
        self.entry.certificate = clip(certificate.into());
        self
    }

    pub fn signer(mut self, signer: impl Into<String>) -> Self {
        self.entry.signer = clip(signer.into());
        self
    }

    pub fn finish(mut self, ok: bool, status: impl Into<String>, message: impl Into<String>) {
        self.entry.ok = ok;
        self.entry.status = clip(status.into());
        self.entry.message = clip(message.into());
        self.entry.duration_ms = self.started.elapsed().as_millis() as u64;
        record(self.entry);
    }
}

/// Request fields come from untrusted clients; keep them short.
fn clip(mut value: String) -> String {
    const MAX: usize = 300;
    if value.len() > MAX {
        let mut end = MAX;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
        value.push('…');
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_newest_first_and_persists() {
        let path =
            std::env::temp_dir().join(format!("macsigner-activity-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        open(path.clone());
        Call::start(Channel::Ctsign, "HubClient/signHash", "https://example.vn")
            .signing(true)
            .certificate("ABC")
            .finish(true, "200", "SUCCESS");
        Call::start(Channel::Esign, "x".repeat(1000), "").finish(false, "FALSE", "");
        let list = entries();
        assert_eq!(list[0].channel, Channel::Esign);
        assert!(list[0].action.chars().count() < 400);
        assert_eq!(list[1].certificate, "ABC");
        open(path.clone());
        assert_eq!(entries().len(), 2);
        clear();
        assert!(entries().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
