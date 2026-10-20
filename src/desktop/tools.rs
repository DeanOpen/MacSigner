// "Kiểm thử / Ký thử" and eSigner's "Ký tệp offline" windows.
use super::{Desktop, GREEN, MUTED, RED};
use crate::selftest::{self, Check, Outcome};
use base64::{Engine, engine::general_purpose::STANDARD};
use eframe::egui;
use std::path::Path;

pub(super) struct ToolsState {
    pub open: bool,
    pub offline_open: bool,
    checks: Vec<Check>,
    sample_text: String,
    result: String,
    offline_path: String,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            open: false,
            offline_open: false,
            checks: Vec::new(),
            sample_text: "Nội dung ký thử — MacSigner".into(),
            result: String::new(),
            offline_path: String::new(),
        }
    }
}

impl Desktop {
    pub(super) fn tools_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.tools.open;
        egui::Window::new("Kiểm thử / Ký thử")
            .open(&mut open)
            .collapsible(false)
            .default_size([760.0, 560.0])
            .show(ctx, |ui| self.tools_ui(ui));
        self.tools.open = open;
        let mut offline = self.tools.offline_open;
        egui::Window::new(
            "Ký tệp offline (Chỉ hỗ trợ ký các tệp tờ khai định dạng xml hoặc tệp xlsx, docx)",
        )
        .open(&mut offline)
        .collapsible(false)
        .resizable(false)
        .default_width(760.0)
        .show(ctx, |ui| self.offline_ui(ui));
        self.tools.offline_open = offline;
    }

    fn run_checks(&mut self, esign: bool, ctsign: bool) {
        self.tools.checks.clear();
        if esign {
            let live = selftest::live_address(&self.esign.status());
            self.tools
                .checks
                .extend(selftest::esign(&self.settings.esign, live));
        }
        if ctsign {
            let live = selftest::live_address(&self.ctsign.status());
            self.tools
                .checks
                .extend(selftest::ctsign(&self.state, live));
        }
        let failed = self
            .tools
            .checks
            .iter()
            .filter(|c| c.outcome == Outcome::Fail)
            .count();
        self.record(format!(
            "Tự kiểm tra: {} mục, {failed} lỗi",
            self.tools.checks.len()
        ));
    }

    fn sign_with_selected(&mut self, esign_flow: bool) {
        let result = (|| -> anyhow::Result<serde_json::Value> {
            let entry = self.ensure_selected_loaded()?;
            let data = self.tools.sample_text.as_bytes().to_vec();
            if esign_flow {
                // eSigner's ONLINE_XML_CER_001 over the prepared bytes.
                let request = serde_json::json!({
                    "CODE": "ONLINE_XML_CER_001", "BASE64_VALUE": STANDARD.encode(&data),
                    "SERIAL_NUMBER": "", "CHECK_CERT_SERIAL": "false", "CHECK_TIN": "false",
                    "TIN_VALID": "", "DATE_SIGNING": "", "TIMEZONE_SIGNING": "", "PIN_CODE": "",
                    "PKCS_11_LIBRARY_NAME_LIST": ""
                });
                let der = entry.der.clone();
                self.state
                    .with_identity(&entry.info.serinumber, |identity| {
                        crate::esign::legacy::sign_request(&request, &der, true, |payloads, alg| {
                            payloads
                                .iter()
                                .map(|p| identity.sign(p, alg).map_err(|e| e.to_string().into()))
                                .collect()
                        })
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                    })?
                    .ok_or_else(|| anyhow::anyhow!("Chứng thư không còn khả dụng"))
            } else {
                Ok(selftest::ctsign_sample_sign(
                    &self.state,
                    &entry.info.serinumber,
                    &data,
                ))
            }
        })();
        match result {
            Ok(value) => {
                self.tools.result = serde_json::to_string_pretty(&value).unwrap_or_default();
                self.record("Ký thử bằng chứng thư đang chọn: thành công, chữ ký đã được xác minh");
            }
            Err(err) => self.record(format!("Ký thử thất bại: {err}")),
        }
    }

    fn tools_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Ký thử");
        ui.label("Nội dung cần ký");
        ui.add(
            egui::TextEdit::multiline(&mut self.tools.sample_text)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("ESIGN · chứng thư mẫu").clicked() {
                match selftest::esign_sample_sign(
                    self.tools.sample_text.as_bytes(),
                    "ONLINE_XML_CER_001",
                ) {
                    Ok(v) => {
                        self.tools.result = serde_json::to_string_pretty(&v).unwrap_or_default();
                        self.record("ESIGN: ký thử bằng chứng thư mẫu thành công");
                    }
                    Err(e) => self.record(format!("ESIGN: {e}")),
                }
            }
            if ui.button("CTSIGN · chứng thư mẫu").clicked() {
                match selftest::ctsign_sample_state() {
                    Ok(state) => {
                        let serial = state
                            .certificates()
                            .ok()
                            .and_then(|c| c.first().map(|c| c.serinumber.clone()))
                            .unwrap_or_default();
                        let v = selftest::ctsign_sample_sign(
                            &state,
                            &serial,
                            self.tools.sample_text.as_bytes(),
                        );
                        self.tools.result = serde_json::to_string_pretty(&v).unwrap_or_default();
                        self.record("CTSIGN: ký thử bằng chứng thư mẫu thành công");
                    }
                    Err(e) => self.record(format!("CTSIGN: {e}")),
                }
            }
            ui.separator();
            let name = self
                .entries
                .get(self.selected)
                .map_or("chưa chọn".to_owned(), |e| e.fields.common_name.clone());
            if ui
                .add_enabled(
                    !self.entries.is_empty(),
                    egui::Button::new("ESIGN · chứng thư đang chọn"),
                )
                .on_hover_text(&name)
                .clicked()
            {
                self.sign_with_selected(true);
            }
            if ui
                .add_enabled(
                    !self.entries.is_empty(),
                    egui::Button::new("CTSIGN · chứng thư đang chọn"),
                )
                .on_hover_text(&name)
                .clicked()
            {
                self.sign_with_selected(false);
            }
        });
        ui.small("Chứng thư mẫu là cặp khóa TEST ONLY, không cần token. Chứng thư đang chọn có thể yêu cầu nhập PIN của token.");
        if !self.tools.result.is_empty() {
            let mut copy = self.tools.result.clone();
            egui::ScrollArea::vertical()
                .id_salt("tool-result")
                .max_height(150.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut copy)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                });
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("Tự kiểm tra");
            if ui.button("▶ Cả hai").clicked() {
                self.run_checks(true, true);
            }
            if ui.button("ESIGN").clicked() {
                self.run_checks(true, false);
            }
            if ui.button("CTSIGN").clicked() {
                self.run_checks(false, true);
            }
        });
        ui.small("Gọi dịch vụ đang chạy bằng client thật; dịch vụ đang tắt thì dùng bản tạm trên cổng ngẫu nhiên. Vector chữ ký so với bản gốc Java/.NET.");
        egui::ScrollArea::vertical()
            .id_salt("checks")
            .show(ui, |ui| {
                egui::Grid::new("checks")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for check in &self.tools.checks {
                            let (text, color) = match check.outcome {
                                Outcome::Pass => ("ĐẠT", GREEN),
                                Outcome::Fail => ("LỖI", RED),
                                Outcome::Skip => ("BỎ QUA", MUTED),
                            };
                            ui.label(egui::RichText::new(text).strong().color(color));
                            ui.label(check.module);
                            ui.label(&check.name);
                            ui.add(
                                egui::Label::new(egui::RichText::new(&check.detail).small()).wrap(),
                            );
                            ui.end_row();
                        }
                    });
            });
    }

    fn offline_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Tệp file ký").strong());
            ui.add(egui::TextEdit::singleline(&mut self.tools.offline_path).desired_width(470.0));
            if ui.button("📂 Browse...").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Tờ khai", &["xml", "xlsx", "docx"])
                    .pick_file()
            {
                self.tools.offline_path = path.to_string_lossy().into_owned();
            }
        });
        let path = Path::new(self.tools.offline_path.trim());
        let status = if self.tools.offline_path.trim().is_empty() {
            "Chọn tệp tờ khai XML, XLSX hoặc DOCX.".to_owned()
        } else if !path.is_file() {
            "Tệp không tồn tại hoặc không đọc được.".to_owned()
        } else {
            match path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "xml" => "Đã chọn XML. Ký XMLDSig tờ khai chưa được triển khai.".to_owned(),
                "xlsx" | "docx" => "Đã chọn OOXML. Ký gói OOXML chưa được triển khai.".to_owned(),
                _ => "Định dạng này không thuộc cửa sổ ký tệp offline của eSigner.".to_owned(),
            }
        };
        ui.label(egui::RichText::new(status).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled(
                false,
                egui::Button::new("📋 Ký điện tử").min_size(egui::vec2(130.0, 36.0)),
            )
            .on_disabled_hover_text("Ký XML/OOXML tương thích eSigner Java chưa hoàn tất.");
        });
        ui.separator();
        ui.label(egui::RichText::new("USB token và chứng thư").strong());
        ui.label(&self.reader_status);
        ui.label(format!("{} chứng thư khả dụng", self.entries.len()));
    }
}
