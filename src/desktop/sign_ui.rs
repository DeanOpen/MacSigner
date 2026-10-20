use super::{
    CertEntry, CertFields, Desktop, EntrySource, GREEN, INK, MUTED, RED, SignatureTemplate,
};
use crate::pdf::Appearance;
use eframe::egui;

/// Stamp size in PDF points (the original API's default box is 200 x 100).
pub(super) const BOX_WIDTH: f32 = 230.0;
pub(super) const BOX_HEIGHT: f32 = 95.0;

/// Text lines of the stamp, from the template and the certificate's own fields.
pub(super) fn stamp_text(
    template: &SignatureTemplate,
    fields: &CertFields,
    mode: usize,
    note: &str,
) -> String {
    let mut lines = Vec::new();
    if template.label || mode == 1 {
        lines.push(format!("Người ký: {}", fields.common_name));
    }
    if template.organization && !fields.organization.is_empty() {
        lines.push(format!("Tổ chức: {}", fields.organization));
    }
    if template.unit && !fields.unit.is_empty() {
        lines.push(format!("Đơn vị: {}", fields.unit));
    }
    if template.email && !fields.email.is_empty() {
        lines.push(format!("Email: {}", fields.email));
    }
    if !template.position.is_empty() {
        lines.push(format!("Chức vụ: {}", template.position));
    }
    if template.signing_time || mode == 1 {
        lines.push(format!(
            "Ngày ký: {}",
            chrono::Local::now().format("%d/%m/%Y %H:%M")
        ));
    }
    if mode == 1 && !note.trim().is_empty() {
        lines.push(note.trim().to_owned());
    }
    lines.join("\n")
}

impl Desktop {
    /// Builds the PDF appearance from the dialog: page, clicked position (top-left
    /// fraction of the rendered page) and template.
    pub(super) fn signature_appearance(
        &self,
        entry: &CertEntry,
        pdf_bytes: &[u8],
    ) -> anyhow::Result<Appearance> {
        if self.appearance_mode == 2 {
            return Ok(Appearance::Invisible);
        }
        let (x, y) = self
            .signature_position
            .ok_or_else(|| anyhow::anyhow!("Nhấp vào trang PDF để đặt vị trí chữ ký"))?;
        let page = self.pdf_page as u32 + 1;
        let info = crate::pdf::page_info(pdf_bytes, page).map_err(|e| anyhow::anyhow!(e))?;
        let [bx0, by0, bx1, by1] = info.bounds;
        let (page_w, page_h) = (bx1 - bx0, by1 - by0);
        let width = BOX_WIDTH.min(page_w);
        let height = BOX_HEIGHT.min(page_h);
        let left = (x * page_w).clamp(0.0, (page_w - width).max(0.0));
        let top = (y * page_h).clamp(0.0, (page_h - height).max(0.0));
        let rect = [
            bx0 + left,
            by1 - top - height,
            bx0 + left + width,
            by1 - top,
        ];
        let template = self
            .preferences
            .templates
            .get(self.sign_template_selected)
            .cloned()
            .unwrap_or_default();
        let mut text = stamp_text(&template, &entry.fields, self.appearance_mode, &self.note);
        let image = if self.appearance_mode == 0
            && template.appearance != 2
            && !template.image_path.is_empty()
        {
            Some(std::fs::read(&template.image_path).map_err(|e| {
                anyhow::anyhow!("Không đọc được ảnh chữ ký {}: {e}", template.image_path)
            })?)
        } else {
            None
        };
        if template.appearance == 1 && image.is_some() && self.appearance_mode == 0 {
            text.clear();
        }
        Ok(Appearance::Visible {
            page,
            rect,
            text,
            image,
            image_background: false,
        })
    }

    pub(super) fn sign_ui(&mut self, ui: &mut egui::Ui) {
        if self.file.is_none() {
            ui.label("Mở tài liệu PDF trước khi ký.");
            if ui.button("Chọn PDF…").clicked() {
                self.open_file();
            }
            return;
        }
        egui::ScrollArea::vertical()
            .max_height(660.0)
            .show(ui, |ui| {
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Thông tin người ký *")
                            .strong()
                            .color(INK),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Chọn chứng thư số:");
                        let selected_name = self
                            .entries
                            .get(self.selected)
                            .map_or("Chưa có chứng thư".to_owned(), |e| e.fields.common_name.clone());
                        egui::ComboBox::from_id_salt("sign_cert_select")
                            .selected_text(selected_name)
                            .width(380.0)
                            .show_ui(ui, |ui| {
                                for (index, entry) in self.entries.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected,
                                        index,
                                        format!(
                                            "{} · …{}",
                                            entry.fields.common_name,
                                            &entry.info.serinumber
                                                [entry.info.serinumber.len().saturating_sub(6)..]
                                        ),
                                    );
                                }
                            });
                        if ui.button("↻").on_hover_text("Tìm lại chứng thư").clicked() {
                            self.refresh(ui.ctx().clone());
                        }
                        if ui.link("Xem chứng thư").clicked() {
                            self.signer_details_open = true;
                        }
                    });
                    ui.add_space(6.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new("Thông tin chứng thư số").strong());
                        if let Some(entry) = self.entries.get(self.selected) {
                            ui.label(format!("Chủ sở hữu: {}", entry.fields.common_name));
                            if !entry.fields.tax_code.is_empty() {
                                ui.label(format!("Mã số thuế: {}", entry.fields.tax_code));
                            }
                            ui.label(format!("Đơn vị cấp phát: {}", common_name(&entry.info.issuer)));
                            ui.label(format!(
                                "Thời gian hiệu lực: {} đến {}",
                                entry.info.valid_from, entry.info.valid_to
                            ));
                            let valid = x509_parser::parse_x509_certificate(&entry.der)
                                .is_ok_and(|(_, c)| c.validity().is_valid());
                            if valid {
                                ui.label(egui::RichText::new("Tình trạng: Chứng thư số có hiệu lực").color(GREEN));
                            } else {
                                ui.label(egui::RichText::new("Tình trạng: Chứng thư số hết hiệu lực").color(RED));
                            }
                            ui.small(format!("Nguồn: {}", entry.source_label()));
                        } else {
                            ui.label("Chưa tìm thấy chứng thư có khóa ký. Cắm USB token rồi bấm ↻.");
                        }
                    });
                    if matches!(
                        self.entries.get(self.selected).map(|e| &e.source),
                        Some(EntrySource::Token { .. })
                    ) {
                        ui.horizontal(|ui| {
                            ui.label("Mã PIN token:");
                            ui.add(
                                egui::TextEdit::singleline(&mut *self.pin)
                                    .password(true)
                                    .desired_width(160.0),
                            );
                            ui.small("Để trống nếu token có bàn phím PIN riêng.");
                        });
                    }
                });
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Hình thức hiển thị *")
                            .strong()
                            .color(INK),
                    );
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.appearance_mode, 0, "Hiển thị chữ ký");
                        ui.radio_value(&mut self.appearance_mode, 1, "Hiển thị ghi chú");
                        ui.radio_value(&mut self.appearance_mode, 2, "Chữ ký vô hình");
                    });
                    if self.appearance_mode == 1 {
                        ui.horizontal(|ui| {
                            ui.label("Ghi chú");
                            ui.add(egui::TextEdit::singleline(&mut self.note).desired_width(420.0));
                        });
                    }
                    if self.appearance_mode != 2 {
                        if self.signature_position.is_some() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Đã chọn vị trí trên trang {}",
                                    self.pdf_page + 1
                                ))
                                .color(GREEN),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(
                                    "Nhấp vào vị trí trên trang PDF (phía sau hộp thoại) để đặt chữ ký.",
                                )
                                .color(RED),
                            );
                        }
                    }
                });
                if self.appearance_mode == 0 {
                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(egui::RichText::new("Hiển thị chữ ký").strong().color(INK));
                        ui.horizontal(|ui| {
                            ui.label("Mẫu");
                            let name = self
                                .preferences
                                .templates
                                .get(self.sign_template_selected)
                                .map_or("Mẫu mặc định".to_owned(), |t| t.name.clone());
                            egui::ComboBox::from_id_salt("sign_template")
                                .selected_text(name)
                                .width(300.0)
                                .show_ui(ui, |ui| {
                                    for (index, template) in
                                        self.preferences.templates.iter().enumerate()
                                    {
                                        ui.selectable_value(
                                            &mut self.sign_template_selected,
                                            index,
                                            &template.name,
                                        );
                                    }
                                });
                            if ui.link("Quản lý mẫu chữ ký").clicked() {
                                self.config_open = true;
                                self.config_tab = super::ConfigTab::Templates;
                            }
                        });
                        let template = self
                            .preferences
                            .templates
                            .get(self.sign_template_selected)
                            .cloned()
                            .unwrap_or_default();
                        let fields = self.entries.get(self.selected).map(|e| e.fields.clone());
                        self.template_preview(ui, &template, fields.as_ref());
                    });
                }
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("Đường dẫn lưu file *")
                            .strong()
                            .color(INK),
                    );
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [520.0, 28.0],
                            egui::TextEdit::singleline(&mut self.output_path),
                        );
                        if ui.button("Chọn").clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("PDF", &["pdf"])
                                .set_file_name(
                                    std::path::Path::new(&self.output_path)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("signed.pdf"),
                                )
                                .save_file()
                        {
                            self.output_path = path.to_string_lossy().into_owned();
                        }
                    });
                });
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("Hủy bỏ").min_size(egui::vec2(110.0, 32.0)))
                        .clicked()
                    {
                        self.sign_dialog_open = false;
                    }
                    let ready = !self.entries.is_empty()
                        && (self.appearance_mode == 2 || self.signature_position.is_some());
                    if ui
                        .add_enabled(
                            ready,
                            egui::Button::new("Ký số")
                                .fill(egui::Color32::from_rgb(199, 236, 199))
                                .min_size(egui::vec2(110.0, 32.0)),
                        )
                        .clicked()
                    {
                        self.sign();
                    }
                });
                ui.label(egui::RichText::new(&self.status).color(MUTED));
            });
    }

    pub(super) fn signer_details_ui(&self, ui: &mut egui::Ui) {
        if let Some(entry) = self.entries.get(self.selected) {
            ui.label(format!("Chủ sở hữu: {}", entry.info.subject));
            ui.label(format!("Đơn vị cấp phát: {}", entry.info.issuer));
            ui.label(format!("Số sê-ri: {}", entry.info.serinumber));
            ui.label(format!(
                "Hiệu lực: {} đến {}",
                entry.info.valid_from, entry.info.valid_to
            ));
            ui.label(format!("Nguồn: {}", entry.source_label()));
            ui.label(
                egui::RichText::new("Chưa xác minh chuỗi tin cậy và trạng thái thu hồi.")
                    .color(MUTED),
            );
        }
    }
}

pub(super) fn common_name(dn: &str) -> &str {
    dn.split(',')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("CN="))
        .unwrap_or(dn)
}
