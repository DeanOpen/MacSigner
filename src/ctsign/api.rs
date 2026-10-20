use crate::{
    activity::{Call, Channel},
    ctsign::{
        model::{Response, SignHashRequest, SignPdfRequest},
        signing::SigningState,
    },
};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode, Uri, header},
    middleware,
    response::{IntoResponse, Response as HttpResponse},
    routing::any,
};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use tower_http::cors::{Any, CorsLayer};

pub const ADDRESS: &str = "127.0.0.1:32001";

pub fn router(state: SigningState) -> Router {
    Router::new()
        .fallback(any(dispatch))
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(Any),
        )
        .layer(middleware::map_response(
            |mut response: HttpResponse| async move {
                if response.headers().get(header::CONTENT_TYPE)
                    == Some(&HeaderValue::from_static("application/json"))
                {
                    response.headers_mut().insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json; charset=utf-8"),
                    );
                }
                if response.status() == StatusCode::NOT_FOUND {
                    response
                        .headers_mut()
                        .remove(header::ACCESS_CONTROL_ALLOW_ORIGIN);
                }
                response
            },
        ))
        .layer(middleware::from_fn(record_call))
}

const MAX_BODY: usize = 1024 * 1024;

/// Records each API call in the activity history: endpoint, origin, certificate
/// serial and the API status. Hashes, PDF data and images are not kept.
async fn record_call(request: Request, next: middleware::Next) -> HttpResponse {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let path = request.uri().path().trim_matches('/');
    let action = path
        .strip_prefix("api/")
        .or_else(|| path.strip_prefix("API/"))
        .unwrap_or(path)
        .to_owned();
    let lower = action.to_ascii_lowercase();
    let signing = lower == "hubclient/signhash" || lower == "hubclient/signpdf";
    let call = Call::start(
        Channel::Ctsign,
        format!("{} {action}", request.method()),
        origin,
    )
    .signing(signing);
    let (parts, body) = request.into_parts();
    let Ok(body) = axum::body::to_bytes(body, MAX_BODY).await else {
        call.finish(false, "413", "Yêu cầu vượt quá 1 MB");
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let input: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let text = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let target = match (
        text("filePath"),
        input.get("hash").and_then(Value::as_array),
    ) {
        (path, _) if !path.is_empty() => path,
        (_, Some(hashes)) => format!("{} hash {}", hashes.len(), text("requestId"))
            .trim()
            .to_owned(),
        _ => text("requestId"),
    };
    let call = call.certificate(text("serinumber")).target(target);

    let response = next.run(Request::from_parts(parts, Body::from(body))).await;
    let http = response.status();
    let (parts, body) = response.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX)
        .await
        .unwrap_or_default();
    let output: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let message = output
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    match output.get("status").and_then(Value::as_i64) {
        Some(200) => call.finish(true, "200", message),
        Some(0) => call.finish(false, "0", "Không có chứng thư nào"),
        Some(code) => call.finish(false, code.to_string(), message),
        None => call.finish(http.is_success(), http.as_u16().to_string(), ""),
    }
    HttpResponse::from_parts(parts, Body::from(body))
}

async fn dispatch(
    State(state): State<SigningState>,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> HttpResponse {
    if method != Method::GET && method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let parts = uri
        .path()
        .trim_matches('/')
        .split('/')
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if !(parts.len() == 3 || parts.len() == 4) || parts[0] != "api" {
        return StatusCode::NOT_FOUND.into_response();
    }
    match (parts[1].as_str(), parts[2].as_str()) {
        ("info", "getinfocert") => Json(json!("1")).into_response(),
        ("signhash", "version") => Json(json!("3333333333333333")).into_response(),
        ("hubclient", "getcert") => {
            let result = match state.certificates() {
                Ok(certs) if certs.is_empty() => Response {
                    status: 0,
                    data: None,
                    message: None,
                },
                Ok(certs) => Response::success(certs, "SUCCESS"),
                Err(e) => Response::error(500, e.to_string()),
            };
            Json(result).into_response()
        }
        ("hubclient", "signhash") => {
            if !state.api_signing_enabled.load(Ordering::SeqCst) {
                return Json(Response::<Value>::error(
                    403,
                    "Chưa cho phép ký hash qua API trong ứng dụng desktop",
                ))
                .into_response();
            }
            let request = match serde_json::from_slice::<SignHashRequest>(&body) {
                Ok(request) => request,
                Err(e) => {
                    return Json(Response::<Value>::error(
                        500,
                        format!("Invalid SignHash request: {e}"),
                    ))
                    .into_response();
                }
            };
            match tokio::task::spawn_blocking(move || state.sign_hash(request)).await {
                Ok(response) => Json(response).into_response(),
                Err(e) => Json(Response::<Value>::error(500, e.to_string())).into_response(),
            }
        }
        ("hubclient", "getfilelocal") => Json(Response::<String>::error(
            501,
            "Native file selection requires desktop build",
        ))
        .into_response(),
        ("hubclient", "signpdf") => {
            if !state.api_signing_enabled.load(Ordering::SeqCst) {
                return Json(Response::<Value>::error(
                    403,
                    "Chưa cho phép ký qua API trong ứng dụng desktop",
                ))
                .into_response();
            }
            let request = match serde_json::from_slice::<SignPdfRequest>(&body) {
                Ok(request) => request,
                Err(e) => {
                    return Json(Response::<Value>::error(
                        500,
                        format!("Lỗi không xác định:{e}"),
                    ))
                    .into_response();
                }
            };
            match tokio::task::spawn_blocking(move || state.sign_pdf(request)).await {
                Ok(response) => Json(response).into_response(),
                Err(e) => Json(Response::<Value>::error(500, e.to_string())).into_response(),
            }
        }
        ("hubclient", "signxml" | "signooxml") => Json(Response::<Value>::error(
            501,
            "Chức năng ký tài liệu chưa được triển khai; không có tài liệu nào được ký",
        ))
        .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
