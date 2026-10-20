// Main window modelled on CTSigningHub ("Phần mềm Ký số điện tử"): menu, toolbar,
// PDF viewer, "Thông tin ký số" and "Cấu hình" dialogs. ESIGN tools, services and
// self-tests are reachable from the menus.
use crate::{
    config::{self, Settings},
    ctsign::{
        model::{CertificateInfo, HashInfo, SignHashRequest},
        signing::{self, SigningState, Source},
    },
    esign::{discovery, online::Algorithm, reader},
    pdf,
    services::{CtsignService, EsignService, Status},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{BufReader, Read},
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver},
    },
    time::Duration,
};
use zeroize::{Zeroize, Zeroizing};

mod config_ui;
mod pdf_view;
mod sign_ui;
mod tools;

const INK: egui::Color32 = egui::Color32::from_rgb(24, 37, 57);
const MUTED: egui::Color32 = egui::Color32::from_rgb(91, 105, 123);
const BRAND: egui::Color32 = egui::Color32::from_rgb(17, 95, 163);
const CYAN: egui::Color32 = egui::Color32::from_rgb(0, 177, 197);
const GREEN: egui::Color32 = egui::Color32::from_rgb(34, 139, 34);
const RED: egui::Color32 = egui::Color32::from_rgb(200, 40, 40);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfigTab {
    Templates,
    TrustedCa,
    Timestamp,
    Update,
    Services,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct SignatureTemplate {
    name: String,
    /// 0 image and text, 1 image only, 2 text only.
    appearance: usize,
    organization: bool,
    label: bool,
    email: bool,
    unit: bool,
    position: String,
    signing_time: bool,
    image_path: String,
}

impl Default for SignatureTemplate {
    fn default() -> Self {
        Self {
            name: "Mẫu mặc định".into(),
            appearance: 0,
            organization: false,
            label: true,
            email: false,
            unit: false,
            position: String::new(),
            signing_time: true,
            image_path: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    templates: Vec<SignatureTemplate>,
    default_template: usize,
    timestamp_enabled: bool,
    timestamp_url: String,
    auto_update: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            templates: vec![SignatureTemplate::default()],
            default_template: 0,
            timestamp_enabled: false,
            timestamp_url: String::new(),
            auto_update: false,
        }
    }
}

impl Preferences {
    /// Stored next to MacSigner's settings.json.
    fn path() -> PathBuf {
        config::default_path().with_file_name("preferences.json")
    }

    fn load() -> Self {
        let mut preferences: Self = std::fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if preferences.templates.is_empty() {
            preferences.templates.push(SignatureTemplate::default());
        }
        preferences.default_template = preferences
            .default_template
            .min(preferences.templates.len() - 1);
        preferences
    }

    fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// Subject attributes shown in stamps and dialogs.
#[derive(Clone, Default)]
struct CertFields {
    common_name: String,
    organization: String,
    unit: String,
    email: String,
    tax_code: String,
}

impl CertFields {
    fn from_der(der: &[u8]) -> Self {
        let Ok((_, cert)) = x509_parser::parse_x509_certificate(der) else {
            return Self::default();
        };
        let first =
            |iter: &mut dyn Iterator<Item = &x509_parser::x509::AttributeTypeAndValue<'_>>| {
                iter.next()
                    .and_then(|a| a.as_str().ok())
                    .unwrap_or("")
                    .to_owned()
            };
        let subject = cert.subject();
        let text = subject.to_string();
        let tax_code = text
            .split(',')
            .find_map(|part| {
                part.trim()
                    .split_once("MST:")
                    .map(|(_, v)| v.trim().to_owned())
            })
            .unwrap_or_default();
        Self {
            common_name: first(&mut subject.iter_common_name()),
            organization: first(&mut subject.iter_organization()),
            unit: first(&mut subject.iter_organizational_unit()),
            email: first(&mut subject.iter_email()),
            tax_code,
        }
    }
}

#[derive(Clone)]
enum EntrySource {
    /// Usable now: Windows store, P12/PEM file, or a token already logged in.
    Loaded(Source),
    /// Found on a USB token through PKCS#11; needs the PIN before signing.
    Token {
        module: PathBuf,
        slot: u64,
        key_id: Vec<u8>,
    },
}

#[derive(Clone)]
struct CertEntry {
    info: CertificateInfo,
    fields: CertFields,
    der: Vec<u8>,
    source: EntrySource,
}

impl CertEntry {
    fn source_label(&self) -> &'static str {
        match self.source {
            EntrySource::Loaded(Source::WindowsStore) => "Kho Windows",
            EntrySource::Loaded(Source::Token) => "USB token (đã đăng nhập)",
            EntrySource::Loaded(Source::File) => "Tệp P12/PEM",
            EntrySource::Token { .. } => "USB token",
        }
    }
}

struct TokenScan {
    readers: String,
    certificates: Vec<(PathBuf, u64, Vec<u8>, Vec<u8>)>,
    problems: usize,
}

fn scan_tokens() -> TokenScan {
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
                "Chưa thấy USB token trong đầu đọc.".into()
            } else {
                format!("Đã nhận USB token: {}", inserted.join(", "))
            }
        }
        Err(err) => format!("Không đọc được đầu đọc thẻ: {err}"),
    };
    let mut scan = TokenScan {
        readers,
        certificates: Vec::new(),
        problems: 0,
    };
    if let Ok(list) = discovery::certificates() {
        for cert in list["certificates"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let (Some(module), Some(slot), Some(id), Some(der)) = (
                cert["module"].as_str(),
                cert["slot"].as_u64(),
                cert["id_hex"].as_str(),
                cert["certificate_der_base64"].as_str(),
            ) else {
                continue;
            };
            if let (Ok(id), Ok(der)) = (crate::esign::pkcs11::decode_hex(id), STANDARD.decode(der))
            {
                scan.certificates
                    .push((PathBuf::from(module), slot, id, der));
            }
        }
        scan.problems = list["token_errors"].as_array().map_or(0, Vec::len)
            + list["module_errors"].as_array().map_or(0, Vec::len);
    }
    scan
}

struct VerifyResult {
    file: String,
    reports: Vec<pdf::SignatureReport>,
}

/// `initial` is a PDF passed on the command line (e.g. from "Open with").
pub fn run(initial: Option<PathBuf>) -> eframe::Result {
    eframe::run_native(
        "Phần mềm Ký số điện tử · MacSigner",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1320.0, 820.0])
                .with_min_inner_size([960.0, 600.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "noto-sans".into(),
                egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSans.ttf"))
                    .into(),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "noto-sans".into());
            }
            cc.egui_ctx.set_fonts(fonts);
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::light();
            style.visuals.widgets.active.bg_fill = BRAND;
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            cc.egui_ctx.set_style(style);
            let mut app = Desktop::new(cc.egui_ctx.clone());
            if let Some(path) = initial {
                app.open_path(path);
            }
            // Opens a dialog at start-up (manual UI checks and screenshots).
            match std::env::var("MACSIGNER_SHOW").as_deref() {
                Ok("sign") => {
                    app.sign_dialog_open = true;
                    app.signature_position = Some((0.55, 0.82));
                }
                Ok("config") => app.config_open = true,
                Ok("services") => {
                    app.config_open = true;
                    app.config_tab = ConfigTab::Services;
                }
                Ok("tools") => app.tools.open = true,
                Ok("offline") => app.tools.offline_open = true,
                _ => {}
            }
            Ok(Box::new(app))
        }),
    )
}

struct Desktop {
    state: SigningState,
    settings_path: PathBuf,
    settings: Settings,
    esign: EsignService,
    ctsign: CtsignService,
    entries: Vec<CertEntry>,
    token_certs: Vec<(PathBuf, u64, Vec<u8>, Vec<u8>)>,
    reader_status: String,
    scan_rx: Option<Receiver<TokenScan>>,
    selected: usize,
    pin: Zeroizing<String>,
    file: Option<PathBuf>,
    digest: String,
    status: String,
    log: Vec<String>,
    import_open: bool,
    import_type: usize,
    import_path: String,
    key_path: String,
    password: Zeroizing<String>,
    log_open: bool,
    config_open: bool,
    config_tab: ConfigTab,
    preferences: Preferences,
    template_selected: usize,
    template_draft: SignatureTemplate,
    ca_details: Option<usize>,
    pdf_pages: usize,
    pdf_page: usize,
    zoom: f32,
    pdf_texture: Option<egui::TextureHandle>,
    pdf_render_pending: bool,
    sign_dialog_open: bool,
    output_path: String,
    appearance_mode: usize,
    note: String,
    signer_details_open: bool,
    signature_position: Option<(f32, f32)>,
    sign_template_selected: usize,
    template_preview_key: String,
    template_preview_texture: Option<egui::TextureHandle>,
    verify_result: Option<VerifyResult>,
    about_open: bool,
    tools: tools::ToolsState,
}

impl Desktop {
    fn new(ctx: egui::Context) -> Self {
        let settings_path = config::default_path();
        let settings = Settings::load(&settings_path).unwrap_or_default();
        let state = SigningState::default();
        let ctsign = CtsignService::new(state.clone()).expect("tokio runtime");
        let mut app = Self {
            state,
            settings_path,
            settings,
            esign: EsignService::default(),
            ctsign,
            entries: Vec::new(),
            token_certs: Vec::new(),
            reader_status: "Đang tìm USB token…".into(),
            scan_rx: None,
            selected: 0,
            pin: Zeroizing::new(String::new()),
            file: None,
            digest: String::new(),
            status: String::new(),
            log: Vec::new(),
            import_open: false,
            import_type: 0,
            import_path: String::new(),
            key_path: String::new(),
            password: Zeroizing::new(String::new()),
            log_open: false,
            config_open: false,
            config_tab: ConfigTab::Templates,
            preferences: Preferences::load(),
            template_selected: 0,
            template_draft: SignatureTemplate::default(),
            ca_details: None,
            pdf_pages: 0,
            pdf_page: 0,
            zoom: 0.75,
            pdf_texture: None,
            pdf_render_pending: false,
            sign_dialog_open: false,
            output_path: String::new(),
            appearance_mode: 0,
            note: String::new(),
            signer_details_open: false,
            signature_position: None,
            sign_template_selected: 0,
            template_preview_key: String::new(),
            template_preview_texture: None,
            verify_result: None,
            about_open: false,
            tools: tools::ToolsState::default(),
        };
        app.sign_template_selected = app.preferences.default_template;
        if let Some(template) = app.preferences.templates.first() {
            app.template_draft = template.clone();
        }
        if app.settings.esign.enabled {
            app.start_esign();
        }
        if app.settings.ctsign.enabled {
            app.start_ctsign();
        }
        app.refresh(ctx);
        app
    }

    fn record(&mut self, message: impl Into<String>) {
        let message = message.into();
        let time = chrono::Local::now().format("%H:%M:%S");
        self.status = message.clone();
        self.log.push(format!("[{time}] {message}"));
    }

    fn start_esign(&mut self) {
        match self.esign.start(&self.settings.esign) {
            Ok(address) => self.record(format!("ESIGN đang chạy: ws://{address}/plugin/sign")),
            Err(e) => self.record(format!("ESIGN: {e}")),
        }
    }

    fn start_ctsign(&mut self) {
        match self.ctsign.start(&self.settings.ctsign) {
            Ok(address) => self.record(format!("CTSIGN API đang chạy: http://{address}")),
            Err(e) => self.record(format!("CTSIGN: {e}")),
        }
    }

    /// Windows store certificates load immediately (no PIN); USB tokens are
    /// probed in the background through PKCS#11 discovery.
    fn refresh(&mut self, ctx: egui::Context) {
        #[cfg(windows)]
        match crate::certstore::personal() {
            Ok(certs) => {
                let identities: Vec<_> = certs
                    .into_iter()
                    .filter_map(|c| signing::Identity::windows_store(c).ok())
                    .collect();
                if !identities.is_empty() {
                    let _ = self.state.add(identities);
                }
            }
            Err(err) => self.record(format!("Lỗi đọc kho chứng thư Windows: {err}")),
        }
        self.rebuild_entries();
        if self.scan_rx.is_none() {
            let (tx, rx) = mpsc::channel();
            self.scan_rx = Some(rx);
            self.reader_status = "Đang tìm USB token…".into();
            std::thread::spawn(move || {
                let _ = tx.send(scan_tokens());
                ctx.request_repaint();
            });
        }
    }

    fn poll_scan(&mut self) {
        let Some(rx) = &self.scan_rx else { return };
        let Ok(scan) = rx.try_recv() else { return };
        self.scan_rx = None;
        self.reader_status = scan.readers;
        self.token_certs = scan.certificates;
        self.rebuild_entries();
        let message = format!(
            "Tìm thấy {} chứng thư.{}",
            self.entries.len(),
            if scan.problems > 0 {
                format!(
                    " {} lỗi đọc token/driver; xem lệnh esign token-diagnostics.",
                    scan.problems
                )
            } else {
                String::new()
            }
        );
        self.record(message);
    }

    /// Loaded identities first; token certificates not yet loaded follow.
    fn rebuild_entries(&mut self) {
        let mut entries = Vec::new();
        if let Ok(identities) = self.state.identities.lock() {
            for identity in identities.iter() {
                entries.push(CertEntry {
                    info: identity.info.clone(),
                    fields: CertFields::from_der(identity.der()),
                    der: identity.der().to_vec(),
                    source: EntrySource::Loaded(identity.source()),
                });
            }
        }
        for (module, slot, key_id, der) in &self.token_certs {
            let Ok(info) = crate::ctsign::certificate::describe(der) else {
                continue;
            };
            if entries.iter().any(|entry: &CertEntry| entry.der == *der) {
                continue;
            }
            entries.push(CertEntry {
                info,
                fields: CertFields::from_der(der),
                der: der.clone(),
                source: EntrySource::Token {
                    module: module.clone(),
                    slot: *slot,
                    key_id: key_id.clone(),
                },
            });
        }
        self.entries = entries;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    /// Makes the selected certificate usable, logging in to the token if needed.
    fn ensure_selected_loaded(&mut self) -> anyhow::Result<CertEntry> {
        let entry = self
            .entries
            .get(self.selected)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Chưa chọn chứng thư số"))?;
        if let EntrySource::Token {
            module,
            slot,
            key_id,
        } = &entry.source
        {
            let pin = (!self.pin.is_empty()).then(|| Zeroizing::new(self.pin.as_str().to_owned()));
            let identity = signing::open_token(
                module,
                *slot,
                key_id,
                pin.as_ref().map(|value| value.as_str()),
            );
            self.pin.zeroize();
            self.state.add(vec![identity?])?;
            let serial = entry.info.serinumber.clone();
            self.rebuild_entries();
            if let Some(index) = self
                .entries
                .iter()
                .position(|e| e.info.serinumber == serial)
            {
                self.selected = index;
            }
        }
        Ok(entry)
    }

    fn open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
        else {
            return;
        };
        self.open_path(path);
    }

    fn open_path(&mut self, path: PathBuf) {
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            self.record("Chỉ chọn tệp PDF cho luồng ký tài liệu.");
            return;
        }
        let page_count = match pdf_view::page_count(&path) {
            Ok(count) if count > 0 => count,
            Ok(_) => {
                self.record("PDF không có trang nào.");
                return;
            }
            Err(err) => {
                self.record(format!("Không mở được PDF: {err}"));
                return;
            }
        };
        let result = (|| -> anyhow::Result<String> {
            let mut file = BufReader::new(std::fs::File::open(&path)?);
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
            Ok(STANDARD.encode(hash.finalize()))
        })();
        match result {
            Ok(digest) => {
                let output_name = format!(
                    "{}-signed.pdf",
                    path.file_stem()
                        .and_then(|name| name.to_str())
                        .unwrap_or("document")
                );
                self.output_path = path
                    .with_file_name(output_name)
                    .to_string_lossy()
                    .into_owned();
                self.record(format!("Tài liệu đã chọn: {}", path.display()));
                self.file = Some(path);
                self.digest = digest;
                self.pdf_pages = page_count;
                self.pdf_page = 0;
                self.signature_position = None;
                self.pdf_texture = None;
                self.pdf_render_pending = true;
            }
            Err(err) => self.record(format!("Không đọc được tệp: {err}")),
        }
    }

    fn sample(&mut self) {
        self.file = None;
        self.pdf_pages = 0;
        self.pdf_texture = None;
        self.digest = STANDARD.encode(Sha256::digest(b"MacSigner USB token signing test"));
        self.record("Đã tạo hash dữ liệu thử; chưa ký hoặc gửi tài liệu nào.");
    }

    fn import(&mut self) {
        let result = if self.import_type == 0 {
            signing::load_pkcs12(std::path::Path::new(&self.import_path), &self.password)
        } else {
            signing::load_pem(
                std::path::Path::new(&self.import_path),
                std::path::Path::new(&self.key_path),
            )
        }
        .and_then(|identities| self.state.add(identities));
        self.password.zeroize();
        match result {
            Ok(count) => {
                self.rebuild_entries();
                self.record(format!(
                    "Đã nạp chứng thư. Hiện có {count} chứng thư sẵn sàng ký."
                ));
                self.import_open = false;
            }
            Err(err) => self.record(format!("Không nạp được chứng thư: {err}")),
        }
    }

    fn sign(&mut self) {
        if let Some(input) = self.file.clone() {
            if self.output_path.trim().is_empty() {
                self.record("Chọn đường dẫn lưu PDF đã ký.");
                return;
            }
            let output = PathBuf::from(self.output_path.trim());
            if !output
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
            {
                self.record("Tệp đầu ra phải có đuôi .pdf.");
                return;
            }
            if output == input {
                self.record("Chọn tên tệp đầu ra khác tệp PDF gốc.");
                return;
            }
            let result = (|| -> anyhow::Result<usize> {
                let entry = self.ensure_selected_loaded()?;
                let original = std::fs::read(&input)?;
                let appearance = self.signature_appearance(&entry, &original)?;
                let options = pdf::SignOptions {
                    appearance,
                    ..Default::default()
                };
                let signed = self
                    .state
                    .with_identity(&entry.info.serinumber, |identity| {
                        pdf::sign(
                            &original,
                            std::slice::from_ref(&entry.der),
                            &options,
                            |data| {
                                identity
                                    .sign(data, Algorithm::Sha256Rsa)
                                    .map_err(|e| e.to_string())
                            },
                        )
                        .map_err(|e| anyhow::anyhow!(e))
                    })?
                    .ok_or_else(|| anyhow::anyhow!("Chứng thư không còn khả dụng"))?;
                let reports = pdf::verify(&signed);
                anyhow::ensure!(
                    reports
                        .last()
                        .is_some_and(|r| r.valid() && r.covers_whole_file),
                    "Chữ ký vừa tạo không vượt qua bước tự kiểm tra"
                );
                std::fs::write(&output, signed)?;
                Ok(reports.len())
            })();
            match result {
                Ok(count) => {
                    self.sign_dialog_open = false;
                    self.signature_position = None;
                    self.open_path(output.clone());
                    self.record(format!(
                        "Ký số thành công: {} (tài liệu có {count} chữ ký)",
                        output.display()
                    ));
                }
                Err(err) => self.record(format!("Không ký được PDF: {err}")),
            }
            return;
        }
        let entry = match self.ensure_selected_loaded() {
            Ok(entry) => entry,
            Err(err) => {
                self.record(format!("{err}"));
                return;
            }
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("macsigner-signature.sha256.json")
            .save_file()
        else {
            return;
        };
        let response = self.state.sign_hash(SignHashRequest {
            request_id: Some("desktop-test".into()),
            signature: None,
            serinumber: entry.info.serinumber,
            hash: vec![HashInfo {
                ma: None,
                digest: self.digest.clone(),
                signature: None,
            }],
        });
        if response.status != 200 {
            self.record(
                response
                    .message
                    .unwrap_or_else(|| "Ký không thành công.".into()),
            );
            return;
        }
        match serde_json::to_vec_pretty(&response)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(Into::into))
        {
            Ok(()) => self.record(format!("Đã ký và lưu kết quả: {}", path.display())),
            Err(err) => self.record(format!("Không lưu được chữ ký: {err}")),
        }
    }

    fn verify_current_pdf(&mut self) {
        let path = match &self.file {
            Some(path) => path.clone(),
            None => match rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
            {
                Some(path) => path,
                None => return,
            },
        };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let reports = pdf::verify(&bytes);
                let valid = reports.iter().filter(|r| r.valid()).count();
                self.record(format!(
                    "Kiểm tra chữ ký: {valid}/{} chữ ký hợp lệ về mật mã",
                    reports.len()
                ));
                self.verify_result = Some(VerifyResult {
                    file: path.display().to_string(),
                    reports,
                });
            }
            Err(err) => self.record(format!("Không đọc được tệp: {err}")),
        }
    }

    fn verify_ui(&self, ui: &mut egui::Ui, result: &VerifyResult) {
        ui.label(egui::RichText::new(&result.file).color(MUTED));
        if result.reports.is_empty() {
            ui.label("Tài liệu không có chữ ký số.");
            return;
        }
        for (i, r) in result.reports.iter().enumerate() {
            ui.group(|ui| {
                ui.set_min_width(560.0);
                let (text, color) = if r.valid() {
                    ("Chữ ký hợp lệ", GREEN)
                } else {
                    ("Chữ ký không hợp lệ", RED)
                };
                ui.label(
                    egui::RichText::new(format!("Chữ ký {}: {text}", i + 1))
                        .strong()
                        .color(color),
                );
                ui.label(format!("Người ký: {}", r.signer));
                ui.label(format!("Đơn vị cấp phát: {}", r.issuer));
                ui.label(format!("Số sê-ri: {}", r.serial));
                if !r.signing_time.is_empty() {
                    ui.label(format!("Thời gian ký: {}", r.signing_time));
                }
                ui.label(format!(
                    "Toàn vẹn dữ liệu: {}",
                    if r.digest_ok {
                        "đúng"
                    } else {
                        "đã bị thay đổi"
                    }
                ));
                ui.label(if r.covers_whole_file {
                    "Phạm vi: toàn bộ tài liệu"
                } else {
                    "Phạm vi: tài liệu được bổ sung sau khi ký (ví dụ có chữ ký khác)"
                });
                ui.label(format!(
                    "Chứng thư: {}",
                    if r.certificate_valid_now {
                        "còn hiệu lực"
                    } else {
                        "hết hiệu lực hoặc chưa có hiệu lực"
                    }
                ));
                if let Some(err) = &r.error {
                    ui.label(egui::RichText::new(err).color(RED));
                }
            });
        }
        ui.label(
            egui::RichText::new("Chưa kiểm tra chuỗi CA tin cậy và trạng thái thu hồi (OCSP/CRL).")
                .color(MUTED),
        );
    }
}

fn path_row(ui: &mut egui::Ui, label: &str, path: &mut String) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add_sized([340.0, 28.0], egui::TextEdit::singleline(path));
        if ui.button("Chọn tệp…").clicked()
            && let Some(selected) = rfd::FileDialog::new().pick_file()
        {
            *path = selected.to_string_lossy().into_owned();
        }
    });
}

fn service_dot(ui: &mut egui::Ui, status: &Status, name: &str) {
    let (color, tip) = match status {
        Status::Running(address) => (GREEN, format!("Đang chạy tại {address}")),
        Status::Stopped => (MUTED, "Đã tắt".to_owned()),
        Status::Failed(err) => (RED, err.clone()),
    };
    status_dot(ui, color);
    ui.label(egui::RichText::new(name).color(color))
        .on_hover_text(tip);
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs(1));
        self.poll_scan();
        // Opening a PDF by dropping it on the window.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()))
        {
            self.open_path(path);
        }
        if self.pdf_render_pending {
            self.pdf_render_pending = false;
            if let Some(path) = &self.file {
                // Render at physical pixel density so text stays sharp on HiDPI screens.
                match pdf_view::render_page(path, self.pdf_page, self.zoom * ctx.pixels_per_point())
                {
                    Ok(image) => {
                        self.pdf_texture =
                            Some(ctx.load_texture("pdf-page", image, egui::TextureOptions::LINEAR))
                    }
                    Err(err) => self.record(format!("Không hiển thị được trang PDF: {err}")),
                }
            }
        }
        let esign_status = self.esign.status();
        let ctsign_status = self.ctsign.status();
        egui::TopBottomPanel::top("header")
            .exact_height(83.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(248, 249, 250))
                    .inner_margin(egui::Margin::symmetric(16, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.menu_button("Hệ thống", |ui| {
                        if ui.button("Cấu hình").clicked() {
                            self.config_open = true;
                            ui.close();
                        }
                        if ui.button("Dịch vụ ESIGN / CTSIGN").clicked() {
                            self.config_open = true;
                            self.config_tab = ConfigTab::Services;
                            ui.close();
                        }
                        if ui.button("Kiểm thử / Ký thử").clicked() {
                            self.tools.open = true;
                            ui.close();
                        }
                        if ui.button("Nhật ký phiên").clicked() {
                            self.log_open = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Thoát").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Chức năng", |ui| {
                        if ui.button("Mở PDF…").clicked() {
                            self.open_file();
                            ui.close();
                        }
                        if ui.button("Ký số…").clicked() {
                            self.sign_dialog_open = true;
                            ui.close();
                        }
                        if ui.button("Kiểm tra chữ ký").clicked() {
                            self.verify_current_pdf();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Ký tệp offline (eSigner)…").clicked() {
                            self.tools.offline_open = true;
                            ui.close();
                        }
                        if ui.button("Nhập chứng thư P12/PFX…").clicked() {
                            self.import_open = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Trợ giúp", |ui| {
                        if ui.button("Thông tin").clicked() {
                            self.about_open = true;
                            ui.close();
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("📂 Mở").clicked() {
                        self.open_file();
                    }
                    if ui.button("⚙ Cấu hình").clicked() {
                        self.config_open = true;
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.pdf_pages > 0 && self.pdf_page > 0,
                            egui::Button::new("⏮"),
                        )
                        .clicked()
                    {
                        self.pdf_page = 0;
                        self.signature_position = None;
                        self.pdf_render_pending = true;
                    }
                    if ui
                        .add_enabled(
                            self.pdf_pages > 0 && self.pdf_page > 0,
                            egui::Button::new("◀"),
                        )
                        .clicked()
                    {
                        self.pdf_page = self.pdf_page.saturating_sub(1);
                        self.signature_position = None;
                        self.pdf_render_pending = true;
                    }
                    ui.label(format!(
                        "{} / {}",
                        if self.pdf_pages == 0 {
                            0
                        } else {
                            self.pdf_page + 1
                        },
                        self.pdf_pages
                    ));
                    if ui
                        .add_enabled(self.pdf_page + 1 < self.pdf_pages, egui::Button::new("▶"))
                        .clicked()
                    {
                        self.pdf_page += 1;
                        self.signature_position = None;
                        self.pdf_render_pending = true;
                    }
                    if ui
                        .add_enabled(self.pdf_page + 1 < self.pdf_pages, egui::Button::new("⏭"))
                        .clicked()
                    {
                        self.pdf_page = self.pdf_pages - 1;
                        self.signature_position = None;
                        self.pdf_render_pending = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("📝 Kiểm tra chữ ký").clicked() {
                            self.verify_current_pdf();
                        }
                        if ui
                            .add(
                                egui::Button::new("Ký số")
                                    .fill(egui::Color32::from_rgb(198, 235, 196)),
                            )
                            .clicked()
                        {
                            self.sign_dialog_open = true;
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("footer")
            .exact_height(43.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(240, 245, 249))
                    .inner_margin(egui::Margin::symmetric(20, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.file.is_some() {
                        if ui.button("−").clicked() {
                            self.zoom = (self.zoom - 0.1).max(0.25);
                            self.pdf_render_pending = true;
                        }
                        ui.label(format!("{:.0}%", self.zoom * 100.0));
                        if ui.button("+").clicked() {
                            self.zoom = (self.zoom + 0.1).min(3.0);
                            self.pdf_render_pending = true;
                        }
                        ui.separator();
                    }
                    service_dot(ui, &esign_status, "ESIGN");
                    service_dot(ui, &ctsign_status, "CTSIGN");
                    ui.separator();
                    ui.label(egui::RichText::new(&self.status).color(MUTED));
                });
            });
        if self.file.is_none() {
            egui::SidePanel::left("certificates")
                .exact_width(340.0)
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(247, 249, 252))
                        .inner_margin(egui::Margin::same(20)),
                )
                .show(ctx, |ui| {
                    ui.heading(egui::RichText::new("Chứng thư của bạn").color(INK));
                    ui.label(
                        egui::RichText::new("Tự nhận từ hệ điều hành và USB token").color(MUTED),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.scan_rx.is_none(), egui::Button::new("↻ Tìm lại"))
                            .clicked()
                        {
                            self.refresh(ui.ctx().clone());
                        }
                        if ui.button("Nhập P12/PFX…").clicked() {
                            self.import_open = true;
                        }
                    });
                    if self.scan_rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Đang đọc USB token…");
                        });
                    } else {
                        ui.small(&self.reader_status);
                    }
                    ui.separator();
                    if self.entries.is_empty() {
                        ui.label("Chưa có chứng thư khả dụng. Cắm USB token rồi bấm Tìm lại.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height() - 90.0)
                        .show(ui, |ui| {
                            for index in 0..self.entries.len() {
                                let entry = &self.entries[index];
                                let (name, serial, until, source) = (
                                    entry.fields.common_name.clone(),
                                    entry.info.serinumber.clone(),
                                    entry.info.valid_to.clone(),
                                    entry.source_label(),
                                );
                                ui.group(|ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.selectable_value(
                                        &mut self.selected,
                                        index,
                                        egui::RichText::new(name).strong(),
                                    );
                                    ui.small(format!("Nguồn: {source}"));
                                    ui.small(format!("Serial: {serial}"));
                                    ui.small(format!("Hiệu lực đến: {until}"));
                                });
                                ui.add_space(5.0);
                            }
                        });
                    ui.separator();
                    let mut api_enabled = self.state.api_signing_enabled.load(Ordering::SeqCst);
                    if ui
                        .checkbox(&mut api_enabled, "Cho phép ký qua API nội bộ (CTSIGN)")
                        .changed()
                    {
                        self.state
                            .api_signing_enabled
                            .store(api_enabled, Ordering::SeqCst);
                    }
                    match &ctsign_status {
                        Status::Running(address) => ui.small(format!("http://{address}")),
                        _ => ui.small("API CTSIGN đang tắt (Hệ thống → Dịch vụ)"),
                    };
                });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(if self.file.is_some() {
                        egui::Color32::from_rgb(235, 236, 238)
                    } else {
                        egui::Color32::WHITE
                    })
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ctx, |ui| {
                if self.file.is_some() {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            if let Some(texture) = &self.pdf_texture {
                                let response = ui.add(
                                    egui::Image::from_texture(texture)
                                        .fit_to_exact_size(texture.size_vec2() / ui.ctx().pixels_per_point())
                                        .sense(egui::Sense::click()),
                                );
                                if response.clicked()
                                    && let Some(pointer) = response.interact_pointer_pos()
                                {
                                    let relative = pointer - response.rect.min;
                                    self.signature_position = Some((
                                        (relative.x / response.rect.width()).clamp(0.0, 1.0),
                                        (relative.y / response.rect.height()).clamp(0.0, 1.0),
                                    ));
                                }
                                if let Some((x, y)) = self.signature_position {
                                    let box_size = egui::vec2(
                                        sign_ui::BOX_WIDTH * self.zoom,
                                        sign_ui::BOX_HEIGHT * self.zoom,
                                    );
                                    let corner = egui::pos2(
                                        (response.rect.left() + x * response.rect.width())
                                            .min(response.rect.right() - box_size.x),
                                        (response.rect.top() + y * response.rect.height())
                                            .min(response.rect.bottom() - box_size.y),
                                    );
                                    let mark = egui::Rect::from_min_size(corner, box_size);
                                    ui.painter().rect_filled(
                                        mark,
                                        0.0,
                                        egui::Color32::from_rgba_unmultiplied(0, 177, 197, 30),
                                    );
                                    ui.painter().rect_stroke(
                                        mark,
                                        0.0,
                                        egui::Stroke::new(2.0_f32, CYAN),
                                        egui::StrokeKind::Outside,
                                    );
                                }
                            } else {
                                ui.label("Đang hiển thị PDF…");
                            }
                        });
                    });
                    return;
                }
                ui.heading(
                    egui::RichText::new("Phần mềm Ký số điện tử")
                        .size(25.0)
                        .color(INK),
                );
                ui.label(
                    egui::RichText::new(
                        "Mở hoặc kéo thả tệp PDF vào đây, bấm vào trang để đặt vị trí chữ ký, rồi chọn Ký số.",
                    )
                    .color(MUTED),
                );
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new("📂 Chọn PDF…")
                                .fill(egui::Color32::from_rgb(223, 239, 251))
                                .min_size(egui::vec2(160.0, 36.0)),
                        )
                        .clicked()
                    {
                        self.open_file();
                    }
                    if ui.button("📝 Kiểm tra chữ ký PDF…").clicked() {
                        self.verify_current_pdf();
                    }
                });
                ui.add_space(24.0);
                ui.separator();
                ui.label(egui::RichText::new("Ký hash thử").strong().color(INK));
                ui.horizontal(|ui| {
                    if ui.button("Tạo dữ liệu thử").clicked() {
                        self.sample();
                    }
                    let ready = !self.digest.is_empty() && !self.entries.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Ký hash thử (JSON)…"))
                        .clicked()
                    {
                        self.sign();
                    }
                });
                ui.add_sized(
                    [ui.available_width(), 56.0],
                    egui::TextEdit::multiline(&mut self.digest).hint_text("SHA-256 (base64)"),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(
                        "PDF được ký nhúng PAdES (ETSI.CAdES.detached, SHA-256). XML tờ khai và Office chưa hỗ trợ.",
                    )
                    .color(MUTED),
                );
            });
        let mut import_open = self.import_open;
        egui::Window::new("Nhập chứng thư từ tệp")
            .open(&mut import_open)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ComboBox::from_label("Loại tệp")
                    .selected_text(if self.import_type == 0 {
                        "P12 / PFX"
                    } else {
                        "PEM / DER + khóa PEM"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.import_type, 0, "P12 / PFX");
                        ui.selectable_value(&mut self.import_type, 1, "PEM / DER + khóa PEM");
                    });
                path_row(ui, "Chứng thư", &mut self.import_path);
                if self.import_type == 0 {
                    ui.label("Mật khẩu tệp P12/PFX");
                    ui.add(egui::TextEdit::singleline(&mut *self.password).password(true));
                } else {
                    path_row(ui, "Khóa PEM", &mut self.key_path);
                }
                if ui.button("Nhập chứng thư").clicked() {
                    self.import();
                }
            });
        self.import_open = import_open;
        let mut sign_dialog_open = self.sign_dialog_open;
        egui::Window::new("Thông tin ký số")
            .open(&mut sign_dialog_open)
            .collapsible(false)
            .resizable(true)
            .min_width(560.0)
            .default_size([600.0, 640.0])
            // Right-aligned and movable: the page behind stays clickable for
            // placing the stamp, as in the original.
            .pivot(egui::Align2::RIGHT_TOP)
            .default_pos(ctx.content_rect().right_top() + egui::vec2(-16.0, 96.0))
            .show(ctx, |ui| self.sign_ui(ui));
        self.sign_dialog_open &= sign_dialog_open;
        let mut signer_details_open = self.signer_details_open;
        egui::Window::new("Thông tin chứng thư số")
            .open(&mut signer_details_open)
            .collapsible(false)
            .show(ctx, |ui| self.signer_details_ui(ui));
        self.signer_details_open = signer_details_open;
        let mut config_open = self.config_open;
        egui::Window::new("Cấu hình")
            .open(&mut config_open)
            .collapsible(false)
            .resizable(true)
            .min_width(700.0)
            .min_height(520.0)
            .default_size([860.0, 560.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| self.config_ui(ui));
        self.config_open = config_open;
        if let Some(index) = self.ca_details {
            let mut details_open = true;
            egui::Window::new("Chứng chỉ CA tham chiếu")
                .open(&mut details_open)
                .collapsible(false)
                .show(ctx, |ui| self.ca_details_ui(ui, index));
            if !details_open {
                self.ca_details = None;
            }
        }
        if let Some(result) = self.verify_result.take() {
            let mut open = true;
            egui::Window::new("Kiểm tra chữ ký")
                .open(&mut open)
                .collapsible(false)
                .default_width(600.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(560.0)
                        .show(ui, |ui| self.verify_ui(ui, &result))
                });
            if open {
                self.verify_result = Some(result);
            }
        }
        self.tools_windows(ctx);
        egui::Window::new("Thông tin")
            .open(&mut self.about_open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.heading("MacSigner");
                ui.label(format!("Phiên bản {}", env!("CARGO_PKG_VERSION")));
                ui.label("Gộp CTSigningHub (ký PDF, API ký hash) và eSigner (plugin thuế điện tử) trong một ứng dụng Rust.");
                ui.label(egui::RichText::new("Không phải bản phát hành chính thức của Cục Thuế hoặc nhà cung cấp chứng thư.").color(MUTED));
            });
        egui::Window::new("Nhật ký phiên")
            .open(&mut self.log_open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for entry in &self.log {
                            ui.label(entry);
                        }
                    });
            });
    }
}

/// A small filled circle (the "●" glyph is missing from the UI font).
fn status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}
