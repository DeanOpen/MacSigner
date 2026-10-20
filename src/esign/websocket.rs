use crate::{
    activity::{Call, Channel},
    esign::{bridge::Backend, native::MAX_MESSAGE, protocol},
};
use std::{
    io,
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tungstenite::{
    Message, accept_hdr_with_config,
    handshake::server::{Request, Response},
    protocol::WebSocketConfig,
};

// Exact Origin and Host checks keep browser requests tied to the configured site.
// Module paths and raw signing are deliberately CLI-only; the prepared-byte
// signing commands go to the backend, which asks the user (see `bridge`).
pub fn permitted(request: &Request, origins: &[String], port: u16) -> bool {
    let host = request.headers().get("host").and_then(|h| h.to_str().ok());
    let expected = [format!("localhost:{port}"), format!("127.0.0.1:{port}")];
    request.uri().path() == "/plugin/sign"
        && request.uri().query().is_none()
        && host.is_some_and(|h| expected.iter().any(|v| v.eq_ignore_ascii_case(h)))
        && request.headers().get_all("origin").iter().count() == 1
        && request
            .headers()
            .get("origin")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| origins.iter().any(|o| o == h))
}

pub const DEFAULT_ORIGIN: &str = "https://thuedientu.gdt.gov.vn";

pub fn validate_origin(value: &str) -> Result<(), String> {
    let uri: tungstenite::http::Uri = value.parse().map_err(|e| format!("{e}"))?;
    if !matches!(uri.scheme_str(), Some("https" | "chrome-extension"))
        || uri.authority().is_none()
        || uri.authority().is_some_and(|a| a.as_str().contains('@'))
        || value
            != format!(
                "{}://{}",
                uri.scheme_str().unwrap(),
                uri.authority().unwrap()
            )
    {
        return Err(format!(
            "origin must be an exact https://host[:port] or chrome-extension://id origin without a path: {value}"
        ));
    }
    Ok(())
}

pub fn bind(primary: u16, backup: Option<u16>) -> io::Result<TcpListener> {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, primary)) {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse && backup.is_some() => {
            TcpListener::bind((Ipv4Addr::LOCALHOST, backup.unwrap()))
        }
        result => result,
    }
}

// Tungstenite's callback ABI requires an unboxed HTTP error response.
#[allow(clippy::result_large_err)]
fn connection(
    stream: TcpStream,
    origins: Arc<Vec<String>>,
    port: u16,
    backend: Option<Arc<dyn Backend>>,
) -> Result<(), Box<dyn std::error::Error>> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    // The handshake callback must be 'static, so it reports the origin through a shared slot.
    let seen = Arc::new(std::sync::Mutex::new(String::new()));
    let slot = seen.clone();
    let handshake = Call::start(Channel::Esign, "Kết nối", "");
    let accepted = accept_hdr_with_config(
        stream,
        move |req: &Request, response: Response| {
            if let Ok(mut slot) = slot.lock() {
                *slot = req
                    .headers()
                    .get("origin")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
            }
            if permitted(req, &origins, port) {
                Ok(response)
            } else {
                Err(Response::builder()
                    .status(403)
                    .body(Some("request origin, host or path rejected".into()))
                    .unwrap())
            }
        },
        Some(config),
    );
    let origin = seen.lock().map(|s| s.clone()).unwrap_or_default();
    let mut ws = match accepted {
        Ok(ws) => ws,
        Err(e) => {
            // Only refused handshakes are worth a line; accepted ones show as commands.
            handshake.client(origin).finish(
                false,
                "403",
                format!("Từ chối kết nối (origin, host hoặc đường dẫn không hợp lệ): {e}"),
            );
            return Err(e.into());
        }
    };
    loop {
        match ws.read()? {
            Message::Text(text) => {
                let call = Call::start(Channel::Esign, "", origin.as_str());
                let handled = protocol::handle_with(text.as_bytes(), &origin, backend.as_deref());
                let ok = handled.reply["SUCCESS"] == "TRUE";
                call.action(handled.action)
                    .signing(handled.signing)
                    .certificate(handled.certificate)
                    .signer(handled.signer)
                    .finish(ok, if ok { "TRUE" } else { "FALSE" }, handled.message);
                ws.send(Message::Text(handled.reply.to_string().into()))?
            }
            Message::Close(_) => {
                let _ = ws.flush();
                return Ok(());
            }
            Message::Ping(_) => ws.flush()?,
            Message::Binary(_) => {
                ws.close(None)?;
                return Ok(());
            }
            _ => {}
        }
    }
}

pub fn serve(listener: TcpListener, origins: Vec<String>) -> io::Result<()> {
    eprintln!(
        "experimental eSigner WebSocket: ws://127.0.0.1:{}/plugin/sign",
        listener.local_addr()?.port()
    );
    serve_until(listener, origins, Arc::new(AtomicBool::new(false)), None)
}

// Polls a nonblocking listener so the desktop can stop the service without
// closing the process. Accepted connections are switched back to blocking mode.
// Without a backend every command, including signing, answers FALSE.
pub fn serve_until(
    listener: TcpListener,
    origins: Vec<String>,
    stop: Arc<AtomicBool>,
    backend: Option<Arc<dyn Backend>>,
) -> io::Result<()> {
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let origins = Arc::new(origins);
    let count = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::SeqCst) {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        stream.set_nonblocking(false)?;
        // The accept loop is the only incrementer; workers only decrement.
        if count.load(Ordering::SeqCst) >= 16 {
            continue;
        }
        count.fetch_add(1, Ordering::SeqCst);
        let (origins, count, backend) = (origins.clone(), count.clone(), backend.clone());
        std::thread::spawn(move || {
            struct Active(Arc<AtomicUsize>);
            impl Drop for Active {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
            let _active = Active(count);
            // Do not print payloads, cookies, certificate material or PINs.
            let _ = connection(stream, origins, port, backend);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &str, host: &str, origin: &str) -> Request {
        Request::builder()
            .uri(path)
            .header("host", host)
            .header("origin", origin)
            .body(())
            .unwrap()
    }

    #[test]
    fn websocket_rejects_other_origins_hosts_and_paths() {
        let origins = vec!["https://thuedientu.gdt.gov.vn".to_string()];
        assert!(permitted(
            &request("/plugin/sign", "127.0.0.1:8768", &origins[0]),
            &origins,
            8768
        ));
        for (path, host, origin) in [
            ("/plugin/other", "127.0.0.1:8768", origins[0].as_str()),
            ("/plugin/sign?x=1", "127.0.0.1:8768", origins[0].as_str()),
            ("/plugin/sign", "attacker.example:8768", origins[0].as_str()),
            ("/plugin/sign", "127.0.0.1:8768", "https://attacker.example"),
            (
                "/plugin/sign",
                "127.0.0.1:8768",
                "http://thuedientu.gdt.gov.vn",
            ),
        ] {
            assert!(!permitted(&request(path, host, origin), &origins, 8768));
        }
        let duplicate = Request::builder()
            .uri("/plugin/sign")
            .header("host", "localhost:8768")
            .header("origin", &origins[0])
            .header("origin", &origins[0])
            .body(())
            .unwrap();
        assert!(!permitted(&duplicate, &origins, 8768));
        let missing = Request::builder()
            .uri("/plugin/sign")
            .header("host", "localhost:8768")
            .body(())
            .unwrap();
        assert!(!permitted(&missing, &origins, 8768));
    }
}
