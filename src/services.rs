// Start/stop control for the two loopback services, shared by the desktop UI,
// the headless `serve` command and the integration tests.
use crate::{
    config::{CtsignSettings, EsignSettings},
    ctsign::{api, signing::SigningState},
    esign::{bridge::Backend, websocket},
};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Running(SocketAddr),
    Failed(String),
}

impl Status {
    pub fn is_running(&self) -> bool {
        matches!(self, Status::Running(_))
    }
}

struct EsignRunning {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<std::io::Result<()>>,
    address: SocketAddr,
}

#[derive(Default)]
pub struct EsignService {
    running: Option<EsignRunning>,
    failure: Option<String>,
    backend: Option<Arc<dyn Backend>>,
}

impl EsignService {
    /// Signs browser requests with `backend`, which must ask the user to approve
    /// each one. Without it the signing commands answer FALSE.
    pub fn with_backend(backend: Arc<dyn Backend>) -> Self {
        Self {
            running: None,
            failure: None,
            backend: Some(backend),
        }
    }

    pub fn start(&mut self, settings: &EsignSettings) -> Result<SocketAddr, String> {
        self.stop();
        for origin in &settings.origins {
            websocket::validate_origin(origin)?;
        }
        if settings.origins.is_empty() {
            return Err("ESIGN cần ít nhất một origin được phép".into());
        }
        let listener = websocket::bind(settings.port, settings.fallback_port)
            .map_err(|e| format!("ESIGN không mở được cổng {}: {e}", settings.port))?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let origins = settings.origins.clone();
        let flag = stop.clone();
        let backend = self.backend.clone();
        let thread = std::thread::Builder::new()
            .name("esign-websocket".into())
            .spawn(move || websocket::serve_until(listener, origins, flag, backend))
            .map_err(|e| e.to_string())?;
        self.failure = None;
        self.running = Some(EsignRunning {
            stop,
            thread,
            address,
        });
        Ok(address)
    }

    pub fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop.store(true, Ordering::SeqCst);
            if let Ok(Err(e)) = running.thread.join() {
                self.failure = Some(e.to_string());
            }
        }
    }

    pub fn status(&mut self) -> Status {
        if self
            .running
            .as_ref()
            .is_some_and(|r| r.thread.is_finished())
        {
            let running = self.running.take().unwrap();
            self.failure = Some(match running.thread.join() {
                Ok(Ok(())) => "Dịch vụ đã dừng".into(),
                Ok(Err(e)) => e.to_string(),
                Err(_) => "Luồng dịch vụ bị lỗi".into(),
            });
        }
        match (&self.running, &self.failure) {
            (Some(r), _) => Status::Running(r.address),
            (None, Some(e)) => Status::Failed(e.clone()),
            (None, None) => Status::Stopped,
        }
    }
}

impl Drop for EsignService {
    fn drop(&mut self) {
        self.stop();
    }
}

struct CtsignRunning {
    shutdown: tokio::sync::oneshot::Sender<()>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    address: SocketAddr,
}

pub struct CtsignService {
    pub state: SigningState,
    runtime: tokio::runtime::Runtime,
    running: Option<CtsignRunning>,
    failure: Option<String>,
}

impl CtsignService {
    pub fn new(state: SigningState) -> std::io::Result<Self> {
        Ok(Self {
            state,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
            running: None,
            failure: None,
        })
    }

    pub fn start(&mut self, settings: &CtsignSettings) -> Result<SocketAddr, String> {
        self.stop();
        let address = format!("127.0.0.1:{}", settings.port);
        let listener = self
            .runtime
            .block_on(tokio::net::TcpListener::bind(address.as_str()))
            .map_err(|e| {
                // Windows reports a port held by http.sys (e.g. the original
                // CTSigningHub) as PermissionDenied (10013) rather than AddrInUse.
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
                ) {
                    format!("Cổng {} đang bị ứng dụng khác giữ (có thể CTSigningHub gốc). Đổi cổng trong Cài đặt hoặc đóng ứng dụng đó", settings.port)
                } else {
                    format!("CTSIGN không mở được {address}: {e}")
                }
            })?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let (shutdown, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = self.runtime.spawn(
            axum::serve(listener, api::router(self.state.clone()))
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .into_future(),
        );
        self.failure = None;
        self.running = Some(CtsignRunning {
            shutdown,
            server,
            address,
        });
        Ok(address)
    }

    pub fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            let _ = running.shutdown.send(());
            match self.runtime.block_on(running.server) {
                Ok(Err(e)) => self.failure = Some(e.to_string()),
                Err(e) => self.failure = Some(e.to_string()),
                Ok(Ok(())) => {}
            }
        }
    }

    pub fn status(&mut self) -> Status {
        if self
            .running
            .as_ref()
            .is_some_and(|r| r.server.is_finished())
        {
            let running = self.running.take().unwrap();
            self.failure = Some(match self.runtime.block_on(running.server) {
                Ok(Ok(())) => "Dịch vụ đã dừng".into(),
                Ok(Err(e)) => e.to_string(),
                Err(e) => e.to_string(),
            });
        }
        match (&self.running, &self.failure) {
            (Some(r), _) => Status::Running(r.address),
            (None, Some(e)) => Status::Failed(e.clone()),
            (None, None) => Status::Stopped,
        }
    }
}

impl Drop for CtsignService {
    fn drop(&mut self) {
        self.stop();
    }
}
