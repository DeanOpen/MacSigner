use super::{CYAN, CertFields, ConfigTab, Desktop, GREEN, MUTED, RED, SignatureTemplate, sign_ui};
use crate::services::Status;
use eframe::egui;
use std::path::Path;
use std::time::UNIX_EPOCH;

const ROOTS: [(&str, &str); 4] = [
    ("MIC National Root CA", "16/05/2040"),
    ("Vietnam National Root CA", "15/04/2039"),
    ("Cơ quan chứng thực số chuyên dùng Chính phủ", "05/03/2030"),
    ("RootCA chuyên dùng Chính phủ G2", "04/08/2048"),
];

impl Desktop {
    fn save_preferences(&mut self) {
        match self.preferences.save() {
            Ok(()) => self.record("Đã lưu cấu hình trên máy này."),
            Err(err) => self.record(format!("Không lưu được cấu hình: {err}")),
        }
    }

    pub(super) fn config_ui(&mut self, ui: &mut egui::Ui) {
        ui.add(egui::Separator::default().spacing(3.0));
        ui.horizontal(|ui| {
            for (tab, label) in [
                (ConfigTab::Templates, "Mẫu chữ ký PDF"),
                (ConfigTab::TrustedCa, "Chứng chỉ tin cậy"),
                (ConfigTab::Timestamp, "Dấu thời gian"),
                (ConfigTab::Update, "Cập nhật"),
                (ConfigTab::Services, "Dịch vụ"),
            ] {
                if ui.selectable_label(self.config_tab == tab, label).clicked() {
                    self.config_tab = tab;
                }
            }
        });
        ui.add(egui::Separator::default().spacing(3.0));
        ui.add_space(8.0);
        egui::ScrollArea::both().show(ui, |ui| match self.config_tab {
            ConfigTab::Templates => self.template_tab(ui),
            ConfigTab::TrustedCa => self.trusted_ca_tab(ui),
            ConfigTab::Timestamp => self.timestamp_tab(ui),
            ConfigTab::Update => self.update_tab(ui),
            ConfigTab::Services => self.services_tab(ui),
        });
    }

    fn template_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Mẫu chữ ký PDF");
            egui::ComboBox::from_id_salt("template_select")
                .selected_text(
                    self.preferences
                        .templates
                        .get(self.template_selected)
                        .map_or("Tạo mẫu mới", |template| &template.name),
                )
                .show_ui(ui, |ui| {
                    for (index, template) in self.preferences.templates.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.template_selected, index, &template.name)
                            .clicked()
                        {
                            self.template_draft = template.clone();
                        }
                    }
                });
            if ui.button("Tạo mẫu mới").clicked() {
                self.template_selected = self.preferences.templates.len();
                self.template_draft = SignatureTemplate {
                    name: String::new(),
                    ..SignatureTemplate::default()
                };
            }
        });
        ui.separator();
        egui::Grid::new("template_options")
            .num_columns(2)
            .spacing([22.0, 12.0])
            .show(ui, |ui| {
                ui.label("Tên mẫu");
                ui.add_sized(
                    [360.0, 28.0],
                    egui::TextEdit::singleline(&mut self.template_draft.name),
                );
                ui.end_row();
                ui.label("Kiểu hiển thị");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.template_draft.appearance, 0, "Ảnh và chữ");
                    ui.radio_value(&mut self.template_draft.appearance, 1, "Chỉ ảnh");
                    ui.radio_value(&mut self.template_draft.appearance, 2, "Chỉ chữ");
                });
                ui.end_row();
                ui.label("Thông tin hiển thị");
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.template_draft.label, "Người ký");
                    ui.checkbox(&mut self.template_draft.organization, "Tổ chức");
                    ui.checkbox(&mut self.template_draft.email, "Email");
                    ui.checkbox(&mut self.template_draft.unit, "Đơn vị");
                    ui.checkbox(&mut self.template_draft.signing_time, "Thời gian ký");
                });
                ui.end_row();
                ui.label("Chức vụ");
                ui.add_sized(
                    [360.0, 28.0],
                    egui::TextEdit::singleline(&mut self.template_draft.position),
                );
                ui.end_row();
                ui.label("Ảnh chữ ký");
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [360.0, 28.0],
                        egui::TextEdit::singleline(&mut self.template_draft.image_path),
                    );
                    if ui.button("Chọn…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Ảnh", &["png", "jpg", "jpeg"])
                            .pick_file()
                    {
                        self.template_draft.image_path = path.to_string_lossy().into_owned();
                    }
                });
                ui.end_row();
            });
        ui.add_space(10.0);
        if self.preferences.default_template == self.template_selected {
            ui.label(egui::RichText::new("(Mẫu mặc định)").color(CYAN));
        }
        let template = self.template_draft.clone();
        self.template_preview(ui, &template, None);
        ui.label(
            egui::RichText::new(
                "Mẫu được áp dụng khi chọn ‘Hiển thị chữ ký’ trong hộp thoại ký PDF.",
            )
            .color(MUTED),
        );
        ui.horizontal(|ui| {
            if ui.button("Lưu mẫu").clicked() {
                let name = self.template_draft.name.trim();
                if name.is_empty() {
                    self.record("Tên mẫu không được để trống.");
                } else if self.template_draft.appearance != 2
                    && !self.template_draft.image_path.is_empty()
                    && !Path::new(&self.template_draft.image_path).is_file()
                {
                    self.record("Không tìm thấy ảnh chữ ký đã chọn.");
                } else {
                    self.template_draft.name = name.to_owned();
                    if self.template_selected < self.preferences.templates.len() {
                        self.preferences.templates[self.template_selected] =
                            self.template_draft.clone();
                    } else {
                        self.preferences.templates.push(self.template_draft.clone());
                        self.template_selected = self.preferences.templates.len() - 1;
                    }
                    self.save_preferences();
                }
            }
            if ui
                .add_enabled(
                    self.template_selected < self.preferences.templates.len(),
                    egui::Button::new("Đặt mặc định"),
                )
                .clicked()
            {
                self.preferences.default_template = self.template_selected;
                self.save_preferences();
            }
            if ui
                .add_enabled(
                    self.preferences.templates.len() > 1
                        && self.template_selected < self.preferences.templates.len(),
                    egui::Button::new("Xóa mẫu"),
                )
                .clicked()
            {
                self.preferences.templates.remove(self.template_selected);
                self.template_selected = self
                    .template_selected
                    .min(self.preferences.templates.len() - 1);
                self.preferences.default_template = self
                    .preferences
                    .default_template
                    .min(self.preferences.templates.len() - 1);
                self.sign_template_selected = self
                    .sign_template_selected
                    .min(self.preferences.templates.len() - 1);
                self.template_draft = self.preferences.templates[self.template_selected].clone();
                self.save_preferences();
            }
        });
    }

    pub(super) fn template_preview(
        &mut self,
        ui: &mut egui::Ui,
        template: &SignatureTemplate,
        signer: Option<&CertFields>,
    ) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Xem trước chữ ký trên PDF").strong());
            ui.horizontal(|ui| {
                if template.appearance != 2 {
                    if template.image_path.is_empty() {
                        ui.label(egui::RichText::new("Chưa chọn ảnh chữ ký").color(MUTED));
                    } else {
                        match self.preview_texture(ui.ctx(), &template.image_path) {
                            Ok(texture) => {
                                ui.add(
                                    egui::Image::new(texture).max_size(egui::vec2(190.0, 110.0)),
                                );
                            }
                            Err(err) => {
                                ui.label(
                                    egui::RichText::new(format!("Không mở được ảnh: {err}"))
                                        .color(egui::Color32::RED),
                                );
                            }
                        }
                    }
                }
                if template.appearance != 1 {
                    let placeholder = CertFields {
                        common_name: "[theo chứng thư]".into(),
                        organization: "[theo chứng thư]".into(),
                        unit: "[theo chứng thư]".into(),
                        email: "[theo chứng thư]".into(),
                        tax_code: String::new(),
                    };
                    let text = sign_ui::stamp_text(template, signer.unwrap_or(&placeholder), 0, "");
                    ui.vertical(|ui| {
                        for line in text.lines() {
                            ui.label(line);
                        }
                    });
                }
            });
        });
    }
    fn preview_texture(
        &mut self,
        ctx: &egui::Context,
        path: &str,
    ) -> anyhow::Result<&egui::TextureHandle> {
        let metadata = std::fs::metadata(path)?;
        let modified = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos();
        let key = format!("{path}:{}:{modified}", metadata.len());
        if self.template_preview_key != key || self.template_preview_texture.is_none() {
            let bytes = std::fs::read(path)?;
            let decoded = image::load_from_memory(&bytes)?.to_rgba8();
            anyhow::ensure!(
                decoded.width() <= 5000 && decoded.height() <= 5000,
                "Ảnh quá lớn để xem trước"
            );
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [decoded.width() as usize, decoded.height() as usize],
                decoded.as_raw(),
            );
            self.template_preview_texture = Some(ctx.load_texture(
                "signature-template-preview",
                color,
                egui::TextureOptions::LINEAR,
            ));
            self.template_preview_key = key;
        }
        self.template_preview_texture
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Không tạo được ảnh xem trước"))
    }

    fn trusted_ca_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Danh mục CA tham chiếu hiển thị trong ứng dụng gốc");
        ui.label(
            egui::RichText::new(
                "Chưa dùng danh mục này để xác minh chuỗi chứng thư hoặc trạng thái thu hồi.",
            )
            .color(MUTED),
        );
        ui.add_space(10.0);
        egui::Grid::new("trusted_ca_table")
            .striped(true)
            .spacing([16.0, 12.0])
            .show(ui, |ui| {
                for title in [
                    "STT",
                    "Tổ chức/Đơn vị",
                    "Được cấp bởi",
                    "Ngày hết hạn",
                    "Hành động",
                ] {
                    ui.label(egui::RichText::new(title).strong().color(CYAN));
                }
                ui.end_row();
                for (index, (name, expiry)) in ROOTS.iter().enumerate() {
                    ui.label((index + 1).to_string());
                    ui.label(*name);
                    ui.label(*name);
                    ui.label(*expiry);
                    if ui.link("Xem").clicked() {
                        self.ca_details = Some(index);
                    }
                    ui.end_row();
                }
            });
    }

    fn timestamp_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Dịch vụ dấu thời gian");
        ui.checkbox(
            &mut self.preferences.timestamp_enabled,
            "Sử dụng dấu thời gian khi ký",
        );
        ui.horizontal(|ui| {
            ui.label("Địa chỉ máy chủ TSA");
            ui.add_enabled(
                self.preferences.timestamp_enabled,
                egui::TextEdit::singleline(&mut self.preferences.timestamp_url)
                    .desired_width(430.0)
                    .hint_text("https://tsa.example.org"),
            );
        });
        ui.label(
            egui::RichText::new(
                "Địa chỉ chỉ được lưu; bản hiện tại chưa gửi yêu cầu RFC 3161 khi ký.",
            )
            .color(MUTED),
        );
        if ui.button("Lưu cấu hình").clicked() {
            if self.preferences.timestamp_enabled
                && !["https://", "http://"]
                    .iter()
                    .any(|prefix| self.preferences.timestamp_url.starts_with(prefix))
            {
                self.record("Địa chỉ TSA phải bắt đầu bằng https:// hoặc http://.");
            } else {
                self.save_preferences();
            }
        }
    }

    fn update_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cập nhật phần mềm");
        ui.label(format!(
            "Phiên bản Rust hiện tại: {}",
            env!("CARGO_PKG_VERSION")
        ));
        ui.label("Ngày phát hành: chưa có bản phát hành chính thức");
        ui.add_space(12.0);
        ui.add_enabled(
            false,
            egui::Checkbox::new(
                &mut self.preferences.auto_update,
                "Tự động kiểm tra cập nhật",
            ),
        );
        ui.label(egui::RichText::new("Tính năng này cần máy chủ phát hành và kiểm tra chữ ký gói cập nhật; hiện chưa khả dụng.").color(MUTED));
    }

    fn services_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Bật/tắt từng dịch vụ. Dịch vụ chỉ lắng nghe trên 127.0.0.1.");
        ui.add_space(6.0);
        let esign_status = self.esign.status();
        let ctsign_status = self.ctsign.status();
        let mut changed = false;
        egui::Grid::new("services")
            .num_columns(5)
            .spacing([18.0, 14.0])
            .striped(true)
            .show(ui, |ui| {
                for title in ["Dịch vụ", "Trạng thái", "Tự bật khi mở", "Cổng", ""] {
                    ui.label(egui::RichText::new(title).strong().color(CYAN));
                }
                ui.end_row();

                ui.label("ESIGN — plugin thuế điện tử (WebSocket)");
                status_label(ui, &esign_status);
                changed |= ui.checkbox(&mut self.settings.esign.enabled, "").changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut self.settings.esign.port).range(1..=65535))
                    .changed();
                if esign_status.is_running() {
                    if ui.button("Tắt").clicked() {
                        self.esign.stop();
                        self.record("ESIGN đã dừng");
                    }
                } else if ui.button("Bật").clicked() {
                    self.start_esign();
                }
                ui.end_row();

                ui.label("CTSIGN — API ký hash / ký PDF (HTTP)");
                status_label(ui, &ctsign_status);
                changed |= ui.checkbox(&mut self.settings.ctsign.enabled, "").changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut self.settings.ctsign.port).range(1..=65535))
                    .changed();
                if ctsign_status.is_running() {
                    if ui.button("Tắt").clicked() {
                        self.ctsign.stop();
                        self.record("CTSIGN đã dừng");
                    }
                } else if ui.button("Bật").clicked() {
                    self.start_ctsign();
                }
                ui.end_row();
            });
        if changed {
            match self.settings.save(&self.settings_path) {
                Ok(()) => self.record("Đã lưu cài đặt dịch vụ (cổng mới áp dụng khi bật lại)."),
                Err(e) => self.record(format!("Cài đặt không hợp lệ: {e}")),
            }
        }
        ui.add_space(10.0);
        let mut api = self
            .state
            .api_signing_enabled
            .load(std::sync::atomic::Ordering::SeqCst);
        if ui
            .checkbox(
                &mut api,
                "Cho phép trang web ký qua API CTSIGN (SignHash, signpdf)",
            )
            .changed()
        {
            self.state
                .api_signing_enabled
                .store(api, std::sync::atomic::Ordering::SeqCst);
        }
        ui.label(
            egui::RichText::new(
                "CTSigningHub gốc dùng cổng 32001; nếu bản gốc đang chạy, đổi cổng CTSIGN khác. \
                 Lệnh trình duyệt ESIGN hiện luôn trả SUCCESS:\"FALSE\".",
            )
            .color(MUTED),
        );
    }

    pub(super) fn ca_details_ui(&self, ui: &mut egui::Ui, index: usize) {
        ui.label(ROOTS[index].0);
        ui.label(format!(
            "Ngày hết hạn theo giao diện gốc: {}",
            ROOTS[index].1
        ));
        ui.label("Chưa có chứng thư DER để xem fingerprint hoặc xác minh chuỗi tin cậy.");
    }
}

fn status_label(ui: &mut egui::Ui, status: &Status) {
    ui.horizontal(|ui| match status {
        Status::Running(address) => {
            super::status_dot(ui, GREEN);
            ui.label(egui::RichText::new(address.to_string()).color(GREEN));
        }
        Status::Stopped => {
            super::status_dot(ui, MUTED);
            ui.label(egui::RichText::new("Đã tắt").color(MUTED));
        }
        Status::Failed(e) => {
            super::status_dot(ui, RED);
            ui.label(egui::RichText::new(e).color(RED));
        }
    });
}
