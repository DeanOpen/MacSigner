use serde::Serialize;
use std::{
    io::ErrorKind,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    time::Duration,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortStatus {
    pub port: u16,
    pub state: &'static str,
    pub detail: String,
}

fn inspect(port: u16) -> PortStatus {
    let address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let (state, detail) = match TcpStream::connect_timeout(&address, Duration::from_millis(150)) {
        Ok(_) => (
            "open",
            "Có dịch vụ TCP; chưa xác nhận là VNPT hoặc TLS hợp lệ".into(),
        ),
        Err(error) if error.kind() == ErrorKind::ConnectionRefused => (
            "closed",
            "Không có dịch vụ nhận kết nối tại thời điểm kiểm tra; không bảo đảm cổng có thể bind"
                .into(),
        ),
        Err(error) => ("unknown", format!("Không kết luận được: {error}")),
    };
    PortStatus {
        port,
        state,
        detail,
    }
}

pub fn ports(configured: u16) -> Result<Vec<PortStatus>, String> {
    if configured == 0 {
        return Err("Cổng phải từ 1 đến 65535".into());
    }
    let mut ports = vec![configured];
    for port in super::protocol::PORTS {
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    Ok(ports.into_iter().map(inspect).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn detects_a_listener_without_claiming_vendor_identity() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let status = inspect(listener.local_addr().unwrap().port());
        assert_eq!(status.state, "open");
        assert!(status.detail.contains("chưa xác nhận"));
    }

    #[test]
    fn rejects_invalid_ports_and_never_enables_an_unimplemented_service() {
        assert!(ports(0).is_err());
        assert!(super::super::ensure_available().is_err());
    }
}
