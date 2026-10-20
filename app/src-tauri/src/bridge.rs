// Approval for browser signing requests. The ESIGN WebSocket hands each
// CER_001 / ONLINE_XML_CER_001 / ONLINE_SUBMIT_SIGNING_001 request to
// `BrowserSigning`, which shows a prompt in the app window ("esign-sign-request")
// and waits for `esign_answer`: the certificate the user chose and, for a
// token, the PIN. Nothing is signed without that answer. The PIN is used for
// this request's login only and is never logged or returned to the page.
use crate::{AppState, commands};
use macsigner::esign::bridge::{Backend, Key, KeyRequest, Zeroizing};
use serde::Serialize;
use std::{
    collections::HashMap,
    error::Error,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

/// How long a prompt waits for the user before the request fails.
const WAIT: Duration = Duration::from_secs(120);

struct Answer {
    cert_id: Option<String>,
    pin: Option<Zeroizing<String>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    id: u64,
    code: String,
    origin: String,
    count: usize,
    sha1: bool,
    serials: Vec<String>,
    tin: Option<String>,
    /// The page sent a PIN_CODE; the user may leave the PIN field empty.
    pin_provided: bool,
    certificates: Vec<commands::CertDto>,
    timeout_secs: u64,
}

#[derive(Default)]
pub struct BrowserSigning {
    app: OnceLock<AppHandle>,
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, mpsc::Sender<Answer>>>,
    /// The prompt on screen, for a window that loads after it was sent.
    current: Mutex<Option<Prompt>>,
    /// One prompt at a time; further requests wait their turn.
    turn: Mutex<()>,
}

impl BrowserSigning {
    pub fn attach(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    /// A prompt is on screen waiting for the user.
    pub fn busy(&self) -> bool {
        self.current.lock().map(|c| c.is_some()).unwrap_or(false)
    }

    fn bring_to_front(app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            let _ = window.request_user_attention(Some(tauri::UserAttentionType::Critical));
        }
    }

    fn ask(&self, app: &AppHandle, prompt: Prompt) -> Result<Answer, Box<dyn Error>> {
        let id = prompt.id;
        let (sender, receiver) = mpsc::channel();
        self.waiting.lock().map_err(|e| e.to_string())?.insert(id, sender);
        if let Ok(mut current) = self.current.lock() {
            *current = Some(prompt.clone());
        }
        Self::bring_to_front(app);
        let answer = app
            .emit("esign-sign-request", prompt)
            .map_err(|e| e.to_string())
            .and_then(|()| receiver.recv_timeout(WAIT).map_err(|e| e.to_string()));
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.remove(&id);
        }
        if let Ok(mut current) = self.current.lock() {
            *current = None;
        }
        answer.map_err(|_| {
            let _ = app.emit("esign-sign-closed", id);
            "Hết thời gian chờ người dùng xác nhận trong MacSigner".into()
        })
    }
}

impl Backend for BrowserSigning {
    fn with_key(
        &self,
        request: &KeyRequest,
        sign: &mut dyn FnMut(&dyn Key) -> Result<(), Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        let app = self.app.get().ok_or("Ứng dụng MacSigner chưa sẵn sàng")?;
        let _turn = self.turn.lock().map_err(|e| e.to_string())?;
        let state = app.state::<AppState>();
        let certificates = commands::browser_candidates(&state, |der| request.accepts(der));
        if certificates.is_empty() {
            return Err(
                "Không có chứng thư còn hiệu lực khớp yêu cầu (serial/MST); hãy cắm token rồi thử lại"
                    .into(),
            );
        }
        let answer = self.ask(
            app,
            Prompt {
                id: self.next.fetch_add(1, Ordering::SeqCst) + 1,
                code: request.code.clone(),
                origin: request.origin.clone(),
                count: request.count,
                sha1: request.uses_sha1(),
                serials: request.serials.clone(),
                tin: request.tin.clone(),
                pin_provided: request.pin.is_some(),
                certificates: certificates.clone(),
                timeout_secs: WAIT.as_secs(),
            },
        )?;
        let cert_id = answer.cert_id.ok_or("Người dùng đã từ chối yêu cầu ký")?;
        if !certificates.iter().any(|c| commands::same_serial(&c.id, &cert_id)) {
            return Err("Chứng thư đã chọn không thuộc danh sách được phép".into());
        }
        let pin = answer.pin.or_else(|| request.pin.clone());
        commands::ensure_loaded(&state, &cert_id, pin.as_ref().map(|p| p.as_str()))?;
        drop(pin);
        state
            .signing
            .with_identity(&cert_id, |identity| {
                sign(identity).map_err(|e| anyhow::anyhow!(e.to_string()))
            })
            .map_err(|e| e.to_string())?
            .ok_or("Chứng thư không còn khả dụng")?;
        Ok(())
    }
}

/// The prompt waiting for an answer, if any (the window may have missed the event).
#[tauri::command]
pub fn esign_pending(signing: State<'_, std::sync::Arc<BrowserSigning>>) -> Option<Prompt> {
    signing.current.lock().ok()?.clone()
}

/// The user's answer to a prompt: a certificate id to sign, or none to decline.
#[tauri::command]
pub fn esign_answer(
    signing: State<'_, std::sync::Arc<BrowserSigning>>,
    id: u64,
    cert_id: Option<String>,
    pin: Option<String>,
) -> Result<(), String> {
    let sender = signing
        .waiting
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&id)
        .ok_or("Yêu cầu ký đã hết hạn")?;
    sender
        .send(Answer {
            cert_id: cert_id.filter(|c| !c.is_empty()),
            pin: pin.filter(|p| !p.is_empty()).map(Zeroizing::new),
        })
        .map_err(|_| "Yêu cầu ký đã hết hạn".to_string())
}
