pub mod diagnostics;
pub mod protocol;

pub const UNAVAILABLE_REASON: &str = "VNPT chưa sẵn sàng: chưa có máy chủ WSS với chứng chỉ TLS tin cậy và backend ký; xác thực thử thách của hãng chưa được hỗ trợ";

pub fn ensure_available() -> Result<(), String> {
    Err(UNAVAILABLE_REASON.into())
}
