// IPC commands for the React UI. Slow work (token probing, signing, which may
// show a vendor PIN dialog, and verification) runs on blocking threads.
use crate::{
    AppState, TokenCert,
    stamp::{self, CertFields, SignatureTemplate},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use macsigner::{
    activity::{self, Call, Channel},
    config::Settings,
    ctsign::{
        certificate,
        signing::{self, Source},
    },
    esign::{discovery, online::Algorithm, pkcs11, reader},
    pdf, selftest,
    services::Status,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(err)?
}

// ---- App -----------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    platform: String,
    config_path: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        config_path: state.settings_path.display().to_string(),
    }
}

/// A PDF passed on the command line ("Open with"), handed out once.
#[tauri::command]
pub fn take_initial_file(state: State<'_, AppState>) -> Option<String> {
    state.initial_file.lock().ok()?.take()
}

// ---- Certificates ------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CertDto {
    /// Certificate serial (hex), used as the identifier everywhere.
    pub(crate) id: String,
    fields: CertFields,
    subject: String,
    issuer: String,
    issuer_name: String,
    valid_from: String,
    valid_to: String,
    valid_now: bool,
    /// "store" | "file" | "token-session" | "token"
    source: String,
    needs_pin: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertList {
    certificates: Vec<CertDto>,
    reader_status: String,
}

fn issuer_cn(der: &[u8]) -> String {
    x509_parser::parse_x509_certificate(der)
        .ok()
        .and_then(|(_, c)| {
            c.issuer()
                .iter_common_name()
                .next()
                .and_then(|a| a.as_str().ok().map(String::from))
        })
        .unwrap_or_default()
}

fn dto(der: &[u8], source: &str, needs_pin: bool) -> Option<CertDto> {
    let info = certificate::describe(der).ok()?;
    let valid_now = x509_parser::parse_x509_certificate(der)
        .map(|(_, c)| c.validity().is_valid())
        .unwrap_or(false);
    Some(CertDto {
        id: info.serinumber.clone(),
        fields: CertFields::from_der(der),
        subject: info.subject,
        issuer: info.issuer,
        issuer_name: issuer_cn(der),
        valid_from: info.valid_from,
        valid_to: info.valid_to,
        valid_now,
        source: source.into(),
        needs_pin,
    })
}

pub(crate) fn same_serial(a: &str, b: &str) -> bool {
    certificate::normalize_serial(a).ok() == certificate::normalize_serial(b).ok()
}

fn current_list(state: &AppState) -> CertList {
    let mut certificates = Vec::new();
    let tokens = state.tokens.lock().map(|t| t.clone()).unwrap_or_default();
    if let Ok(identities) = state.signing.identities.lock() {
        for identity in identities.iter() {
            let source = match identity.source() {
                Source::WindowsStore => "store",
                Source::File => "file",
                Source::Token => "token-session",
            };
            // A store certificate that is also on a plugged token is signed on the
            // token directly (see ensure_loaded), so MacSigner asks for the PIN.
            let on_token = identity.source() == Source::WindowsStore
                && tokens.iter().any(|t| t.der == identity.der());
            if let Some(d) = dto(identity.der(), source, on_token) {
                certificates.push(d);
            }
        }
    }
    for token in tokens {
        if let Some(d) = dto(&token.der, "token", true)
            && !certificates.iter().any(|c| same_serial(&c.id, &d.id))
        {
            certificates.push(d);
        }
    }
    CertList {
        certificates,
        reader_status: state.reader_status.lock().map(|s| s.clone()).unwrap_or_default(),
    }
}

fn load_store(state: &AppState) {
    #[cfg(windows)]
    if let Ok(certs) = macsigner::certstore::personal() {
        let identities: Vec<_> = certs
            .into_iter()
            .filter_map(|c| signing::Identity::windows_store(c).ok())
            .collect();
        if !identities.is_empty() {
            let _ = state.signing.add(identities);
        }
    }
    #[cfg(not(windows))]
    let _ = state;
}

fn scan_tokens(state: &AppState) {
    let readers = match reader::readers() {
        Ok(list) => {
            let inserted: Vec<String> = list
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .filter(|r| r["card_present"].as_bool() == Some(true))
                .filter_map(|r| r["reader"].as_str().map(String::from))
                .collect();
            if inserted.is_empty() {
                "Chưa thấy USB token trong đầu đọc".to_owned()
            } else {
                format!("Đã nhận USB token: {}", inserted.join(", "))
            }
        }
        Err(e) => format!("Không đọc được đầu đọc thẻ: {e}"),
    };
    let mut found = Vec::new();
    if let Ok(list) = discovery::certificates() {
        for cert in list["certificates"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let (Some(module), Some(slot), Some(id), Some(der)) = (
                cert["module"].as_str(),
                cert["slot"].as_u64(),
                cert["id_hex"].as_str(),
                cert["certificate_der_base64"].as_str(),
            ) else {
                continue;
            };
            if let (Ok(key_id), Ok(der)) = (pkcs11::decode_hex(id), STANDARD.decode(der)) {
                found.push(TokenCert {
                    module: PathBuf::from(module),
                    slot,
                    key_id,
                    der,
                });
            }
        }
    }
    if let Ok(mut tokens) = state.tokens.lock() {
        *tokens = found;
    }
    let devices = discovery::tokens()["tokens"].as_array().cloned().unwrap_or_default();
    if let Ok(mut d) = state.token_devices.lock() {
        *d = devices;
    }
    if let Ok(mut status) = state.reader_status.lock() {
        *status = readers;
    }
}

/// Certificates offered for a browser signing request: those `accepts` allows
/// (valid now, matching the serial / MST the page asked for). Tokens are
/// scanned only when nothing loaded so far qualifies.
pub(crate) fn browser_candidates(state: &AppState, accepts: impl Fn(&[u8]) -> bool) -> Vec<CertDto> {
    let allowed = |state: &AppState| -> Vec<String> {
        let mut ders: Vec<Vec<u8>> = state
            .signing
            .identities
            .lock()
            .map(|ids| ids.iter().map(|i| i.der().to_vec()).collect())
            .unwrap_or_default();
        if let Ok(tokens) = state.tokens.lock() {
            ders.extend(tokens.iter().map(|t| t.der.clone()));
        }
        ders.iter()
            .filter(|der| accepts(der))
            .filter_map(|der| certificate::describe(der).ok().map(|i| i.serinumber))
            .collect()
    };
    load_store(state);
    let mut serials = allowed(state);
    if serials.is_empty() {
        scan_tokens(state);
        serials = allowed(state);
    }
    current_list(state)
        .certificates
        .into_iter()
        .filter(|c| serials.iter().any(|s| same_serial(s, &c.id)))
        .collect()
}

/// Windows store (instant, no PIN) plus, when `rescan`, PKCS#11 token discovery.
#[tauri::command]
pub async fn list_certificates(app: AppHandle, rescan: bool) -> CmdResult<CertList> {
    blocking(app, move |state| {
        load_store(state);
        if rescan {
            scan_tokens(state);
        }
        Ok(current_list(state))
    })
    .await
}

#[tauri::command]
pub async fn import_certificate(
    app: AppHandle,
    path: String,
    key_path: Option<String>,
    password: Option<String>,
) -> CmdResult<CertList> {
    blocking(app, move |state| {
        let identities = match key_path.filter(|k| !k.is_empty()) {
            Some(key) => signing::load_pem(Path::new(&path), Path::new(&key)),
            None => signing::load_pkcs12(Path::new(&path), password.as_deref().unwrap_or("")),
        }
        .map_err(err)?;
        state.signing.add(identities).map_err(err)?;
        Ok(current_list(state))
    })
    .await
}

/// Makes the certificate usable, logging in to its token first if needed.
///
/// A Windows-store certificate that also sits on a plugged token is signed on the
/// token with the PIN typed in MacSigner. The store route goes through the vendor
/// CSP, which picks a reader by itself and with two tokens plugged can prompt for
/// (and fail on) the wrong one.
pub(crate) fn ensure_loaded(state: &AppState, serial: &str, pin: Option<&str>) -> CmdResult<Vec<u8>> {
    let pin = pin.filter(|p| !p.is_empty());
    let loaded = state
        .signing
        .with_identity(serial, |i| Ok((i.der().to_vec(), i.source())))
        .ok()
        .flatten();
    let on_token = |der: &[u8]| {
        state
            .tokens
            .lock()
            .map(|t| t.iter().any(|t| t.der == der))
            .unwrap_or(false)
    };
    match loaded {
        Some((der, Source::WindowsStore)) if on_token(&der) => {
            if pin.is_none() {
                return Err("Nhập mã PIN token để ký".into());
            }
        }
        Some((der, _)) => return Ok(der),
        None => {}
    }
    let token = state
        .tokens
        .lock()
        .map_err(err)?
        .iter()
        .find(|t| {
            certificate::describe(&t.der)
                .map(|i| same_serial(&i.serinumber, serial))
                .unwrap_or(false)
        })
        .cloned()
        .ok_or("Không tìm thấy chứng thư đã chọn — hãy cắm token và bấm Tìm lại")?;
    let identity = signing::open_token(&token.module, token.slot, &token.key_id, pin).map_err(err)?;
    let der = identity.der().to_vec();
    state.signing.add(vec![identity]).map_err(err)?;
    Ok(der)
}

// ---- PDF -------------------------------------------------------------------

#[tauri::command]
pub async fn read_file(path: String) -> CmdResult<tauri::ipc::Response> {
    let target = path.clone();
    tauri::async_runtime::spawn_blocking(move || std::fs::read(&target))
        .await
        .map_err(err)?
        .map(tauri::ipc::Response::new)
        .map_err(|e| format!("Không đọc được {path}: {e}"))
}

#[tauri::command]
pub fn default_output(path: String) -> String {
    let p = Path::new(&path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
    p.with_file_name(format!("{stem}-signed.pdf"))
        .to_string_lossy()
        .into_owned()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    input: String,
    output: String,
    cert_id: String,
    pin: Option<String>,
    /// 1-based page.
    page: u32,
    x: f32,
    y: f32,
    /// 0 signature, 1 note, 2 invisible.
    mode: usize,
    template: SignatureTemplate,
    note: String,
    reason: Option<String>,
    location: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignResult {
    output: String,
    signatures: usize,
}

#[tauri::command]
pub async fn sign_pdf(app: AppHandle, request: SignRequest) -> CmdResult<SignResult> {
    let call = Call::start(Channel::App, "Ký PDF", "")
        .signing(true)
        .target(request.input.clone())
        .certificate(request.cert_id.clone());
    let result = blocking(app, move |state| sign_pdf_now(state, request)).await;
    match &result {
        Ok((signer, r)) => call
            .signer(signer.as_str())
            .finish(true, "OK", format!("Đã lưu {}", r.output)),
        Err(e) => call.finish(false, "Lỗi", e.as_str()),
    }
    result.map(|(_, r)| r)
}

/// Signs and saves; returns the signer (name and serial) for the history.
fn sign_pdf_now(state: &AppState, request: SignRequest) -> CmdResult<(String, SignResult)> {
    {
        let output = PathBuf::from(request.output.trim());
        if !output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        {
            return Err("Tệp lưu phải có đuôi .pdf".into());
        }
        if output == Path::new(&request.input) {
            return Err("Chọn tên tệp lưu khác tệp gốc".into());
        }
        let der = ensure_loaded(state, &request.cert_id, request.pin.as_deref())?;
        let original = std::fs::read(&request.input).map_err(err)?;
        let fields = CertFields::from_der(&der);
        let appearance = stamp::appearance(
            &original,
            request.page,
            request.x,
            request.y,
            request.mode,
            &request.template,
            &fields,
            &request.note,
        )?;
        let options = pdf::SignOptions {
            appearance,
            reason: request.reason.unwrap_or_default(),
            location: request.location.unwrap_or_default(),
            field_name: None,
        };
        let signed = state
            .signing
            .with_identity(&request.cert_id, |identity| {
                pdf::sign(&original, std::slice::from_ref(&der), &options, |data| {
                    identity
                        .sign(data, Algorithm::Sha256Rsa)
                        .map_err(|e| e.to_string())
                })
                .map_err(|e| anyhow::anyhow!(e))
            })
            .map_err(err)?
            .ok_or("Chứng thư không còn khả dụng")?;
        let reports = pdf::verify(&signed);
        if !reports.last().is_some_and(|r| r.valid() && r.covers_whole_file) {
            return Err("Chữ ký vừa tạo không vượt qua bước tự kiểm tra; tệp chưa được lưu".into());
        }
        std::fs::write(&output, signed).map_err(err)?;
        Ok((
            fields.common_name,
            SignResult {
                output: output.display().to_string(),
                signatures: reports.len(),
            },
        ))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDto {
    signer: String,
    issuer: String,
    serial: String,
    signing_time: String,
    covers_whole_file: bool,
    digest_ok: bool,
    signature_ok: bool,
    certificate_valid_now: bool,
    valid: bool,
    error: Option<String>,
}

#[tauri::command]
pub async fn verify_pdf(path: String) -> CmdResult<Vec<ReportDto>> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(err)?;
        Ok(pdf::verify(&bytes)
            .into_iter()
            .map(|r| ReportDto {
                valid: r.valid(),
                signer: r.signer,
                issuer: r.issuer,
                serial: r.serial,
                signing_time: r.signing_time,
                covers_whole_file: r.covers_whole_file,
                digest_ok: r.digest_ok,
                signature_ok: r.signature_ok,
                certificate_valid_now: r.certificate_valid_now,
                error: r.error,
            })
            .collect())
    })
    .await
    .map_err(err)?
}

// ---- Settings & services ------------------------------------------------------

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.lock().map_err(err)?.clone())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    settings.save(&state.settings_path)?;
    *state.settings.lock().map_err(err)? = settings;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDto {
    running: bool,
    address: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicesDto {
    esign: ServiceDto,
    ctsign: ServiceDto,
    api_signing: bool,
}

fn service_dto(status: Status) -> ServiceDto {
    match status {
        Status::Running(a) => ServiceDto {
            running: true,
            address: Some(a.to_string()),
            error: None,
        },
        Status::Stopped => ServiceDto {
            running: false,
            address: None,
            error: None,
        },
        Status::Failed(e) => ServiceDto {
            running: false,
            address: None,
            error: Some(e),
        },
    }
}

fn services(state: &AppState) -> CmdResult<ServicesDto> {
    Ok(ServicesDto {
        esign: service_dto(state.esign.lock().map_err(err)?.status()),
        ctsign: service_dto(state.ctsign.lock().map_err(err)?.status()),
        api_signing: state.signing.api_signing_enabled.load(Ordering::SeqCst),
    })
}

#[tauri::command]
pub fn services_status(state: State<'_, AppState>) -> CmdResult<ServicesDto> {
    services(&state)
}

#[tauri::command]
pub fn set_service(state: State<'_, AppState>, name: String, running: bool) -> CmdResult<ServicesDto> {
    let settings = state.settings.lock().map_err(err)?.clone();
    match (name.as_str(), running) {
        ("esign", true) => {
            state.esign.lock().map_err(err)?.start(&settings.esign)?;
        }
        ("esign", false) => state.esign.lock().map_err(err)?.stop(),
        ("ctsign", true) => {
            state.ctsign.lock().map_err(err)?.start(&settings.ctsign)?;
        }
        ("ctsign", false) => state.ctsign.lock().map_err(err)?.stop(),
        _ => return Err(format!("Dịch vụ không hợp lệ: {name}")),
    }
    services(&state)
}

#[tauri::command]
pub fn set_api_signing(state: State<'_, AppState>, enabled: bool) -> CmdResult<ServicesDto> {
    state
        .signing
        .api_signing_enabled
        .store(enabled, Ordering::SeqCst);
    services(&state)
}

// ---- Signature templates and PDF preferences ---------------------------------

#[derive(Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    templates: Vec<SignatureTemplate>,
    default_template: usize,
    timestamp_enabled: bool,
    timestamp_url: String,
    reason: String,
    location: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            templates: vec![SignatureTemplate::default()],
            default_template: 0,
            timestamp_enabled: false,
            timestamp_url: String::new(),
            reason: String::new(),
            location: String::new(),
        }
    }
}

fn preferences_path(state: &AppState) -> PathBuf {
    state.settings_path.with_file_name("app-preferences.json")
}

#[tauri::command]
pub fn get_preferences(state: State<'_, AppState>) -> Preferences {
    let mut prefs: Preferences = std::fs::read(preferences_path(&state))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if prefs.templates.is_empty() {
        prefs.templates.push(SignatureTemplate::default());
    }
    prefs.default_template = prefs.default_template.min(prefs.templates.len() - 1);
    prefs
}

#[tauri::command]
pub fn save_preferences(state: State<'_, AppState>, preferences: Preferences) -> CmdResult<()> {
    if preferences.timestamp_enabled
        && !["https://", "http://"]
            .iter()
            .any(|p| preferences.timestamp_url.starts_with(p))
    {
        return Err("Địa chỉ TSA phải bắt đầu bằng https:// hoặc http://".into());
    }
    let path = preferences_path(&state);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(&preferences).map_err(err)?).map_err(err)
}

// ---- Activity history ------------------------------------------------------------

#[tauri::command]
pub fn activity_list() -> Vec<activity::Entry> {
    activity::entries()
}

#[tauri::command]
pub fn activity_clear() {
    activity::clear();
}

// ---- Self-test and test signing ----------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDto {
    module: String,
    name: String,
    outcome: String,
    detail: String,
}

#[tauri::command]
pub async fn run_selftest(app: AppHandle, module: String) -> CmdResult<Vec<CheckDto>> {
    blocking(app, move |state| {
        let settings = state.settings.lock().map_err(err)?.clone();
        let mut checks = Vec::new();
        if module != "ctsign" {
            let live = selftest::live_address(&state.esign.lock().map_err(err)?.status());
            checks.extend(selftest::esign(&settings.esign, live));
        }
        if module != "esign" {
            let live = selftest::live_address(&state.ctsign.lock().map_err(err)?.status());
            checks.extend(selftest::ctsign(&state.signing, live));
        }
        Ok(checks
            .into_iter()
            .map(|c| CheckDto {
                module: c.module.into(),
                name: c.name,
                outcome: match c.outcome {
                    selftest::Outcome::Pass => "pass",
                    selftest::Outcome::Fail => "fail",
                    selftest::Outcome::Skip => "skip",
                }
                .into(),
                detail: c.detail,
            })
            .collect())
    })
    .await
}

/// Test signing. Without `cert_id` the TEST ONLY sample certificate is used.
#[tauri::command]
pub async fn sample_sign(
    app: AppHandle,
    module: String,
    text: String,
    cert_id: Option<String>,
    pin: Option<String>,
) -> CmdResult<String> {
    blocking(app, move |state| {
        let data = text.into_bytes();
        let value = match (module.as_str(), cert_id) {
            ("esign", None) => selftest::esign_sample_sign(&data, "ONLINE_XML_CER_001")?,
            ("ctsign", None) => {
                let sample = selftest::ctsign_sample_state()?;
                let serial = sample
                    .certificates()
                    .map_err(err)?
                    .first()
                    .map(|c| c.serinumber.clone())
                    .unwrap_or_default();
                selftest::ctsign_sample_sign(&sample, &serial, &data)
            }
            ("esign", Some(serial)) => {
                let der = ensure_loaded(state, &serial, pin.as_deref())?;
                let request = serde_json::json!({
                    "CODE": "ONLINE_XML_CER_001", "BASE64_VALUE": STANDARD.encode(&data),
                    "SERIAL_NUMBER": "", "CHECK_CERT_SERIAL": "false", "CHECK_TIN": "false",
                    "TIN_VALID": "", "DATE_SIGNING": "", "TIMEZONE_SIGNING": "", "PIN_CODE": "",
                    "PKCS_11_LIBRARY_NAME_LIST": ""
                });
                state
                    .signing
                    .with_identity(&serial, |identity| {
                        macsigner::esign::legacy::sign_request(&request, &der, true, |payloads, alg| {
                            payloads
                                .iter()
                                .map(|p| identity.sign(p, alg).map_err(|e| e.to_string().into()))
                                .collect()
                        })
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                    })
                    .map_err(err)?
                    .ok_or("Chứng thư không còn khả dụng")?
            }
            ("ctsign", Some(serial)) => {
                ensure_loaded(state, &serial, pin.as_deref())?;
                selftest::ctsign_sample_sign(&state.signing, &serial, &data)
            }
            _ => return Err(format!("Mô-đun không hợp lệ: {module}")),
        };
        serde_json::to_string_pretty(&value).map_err(err)
    })
    .await
}

/// eSigner "Ký tệp offline": what can be done with the chosen file.
#[tauri::command]
pub fn offline_status(path: String) -> String {
    let p = Path::new(path.trim());
    if path.trim().is_empty() {
        return "Chọn tệp tờ khai XML, XLSX hoặc DOCX.".into();
    }
    if !p.is_file() {
        return "Tệp không tồn tại hoặc không đọc được.".into();
    }
    match p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "xml" => "Đã chọn XML. Ký XMLDSig tờ khai chưa được triển khai.".into(),
        "xlsx" | "docx" => "Đã chọn OOXML. Ký gói OOXML chưa được triển khai.".into(),
        _ => "Định dạng này không thuộc cửa sổ ký tệp offline của eSigner.".into(),
    }
}

// ---- Token devices ------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenDevice {
    label: String,
    manufacturer: String,
    model: String,
    serial: String,
    slot: u64,
    pin_locked: bool,
    pin_final_try: bool,
    pin_count_low: bool,
    certificates: Vec<CertDto>,
}

/// USB tokens from the last scan, each with the certificates it holds. A
/// certificate also present in the Windows store is reported with source "store".
#[tauri::command]
pub fn token_devices(state: State<'_, AppState>) -> CmdResult<Vec<TokenDevice>> {
    let devices = state.token_devices.lock().map_err(err)?.clone();
    let tokens = state.tokens.lock().map_err(err)?.clone();
    let loaded = current_list(&state).certificates;
    let mut out = Vec::new();
    for d in devices {
        let module = d["module"].as_str().unwrap_or("").to_owned();
        let slot = d["slot"].as_u64().unwrap_or(0);
        let certificates = tokens
            .iter()
            .filter(|t| t.module.to_string_lossy() == module && t.slot == slot)
            .filter_map(|t| dto(&t.der, "token", true))
            .map(|mut c| {
                if let Some(l) = loaded.iter().find(|l| same_serial(&l.id, &c.id)) {
                    c.source = l.source.clone();
                    c.needs_pin = l.needs_pin;
                }
                c
            })
            .collect();
        let text = |k: &str| d[k].as_str().unwrap_or("").trim_end_matches('\0').trim().to_owned();
        out.push(TokenDevice {
            label: text("label"),
            manufacturer: text("manufacturer"),
            model: text("model"),
            serial: text("serial"),
            slot,
            pin_locked: d["user_pin_locked"].as_bool().unwrap_or(false),
            pin_final_try: d["user_pin_final_try"].as_bool().unwrap_or(false),
            pin_count_low: d["user_pin_count_low"].as_bool().unwrap_or(false),
            certificates,
        });
    }
    Ok(out)
}