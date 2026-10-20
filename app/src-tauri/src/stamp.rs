// Signature stamp: template, certificate fields and placement on the page.
use macsigner::pdf::Appearance;
use serde::{Deserialize, Serialize};

/// Same shape as the stored signature templates ("Mẫu chữ ký PDF").
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SignatureTemplate {
    pub name: String,
    /// 0 image and text, 1 image only, 2 text only.
    pub appearance: usize,
    pub organization: bool,
    pub label: bool,
    pub email: bool,
    pub unit: bool,
    pub position: String,
    pub signing_time: bool,
    pub image_path: String,
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

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertFields {
    pub common_name: String,
    pub organization: String,
    pub unit: String,
    pub email: String,
    pub tax_code: String,
}

impl CertFields {
    pub fn from_der(der: &[u8]) -> Self {
        let Ok((_, cert)) = x509_parser::parse_x509_certificate(der) else {
            return Self::default();
        };
        let subject = cert.subject();
        let first = |iter: &mut dyn Iterator<Item = &x509_parser::x509::AttributeTypeAndValue<'_>>| {
            iter.next()
                .and_then(|a| a.as_str().ok())
                .unwrap_or("")
                .to_owned()
        };
        let tax_code = subject
            .to_string()
            .split(',')
            .find_map(|p| p.trim().split_once("MST:").map(|(_, v)| v.trim().to_owned()))
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

/// Stamp text lines. `mode`: 0 signature, 1 note, 2 invisible.
pub fn text(template: &SignatureTemplate, fields: &CertFields, mode: usize, note: &str) -> String {
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
        lines.push(format!("Ngày ký: {}", chrono::Local::now().format("%d/%m/%Y %H:%M")));
    }
    if mode == 1 && !note.trim().is_empty() {
        lines.push(note.trim().to_owned());
    }
    lines.join("\n")
}

/// Stamp size in PDF points.
pub const BOX_WIDTH: f32 = 230.0;
pub const BOX_HEIGHT: f32 = 95.0;

/// `x`/`y`: top-left corner of the stamp as a fraction of the displayed page.
#[allow(clippy::too_many_arguments)]
pub fn appearance(
    pdf: &[u8],
    page: u32,
    x: f32,
    y: f32,
    mode: usize,
    template: &SignatureTemplate,
    fields: &CertFields,
    note: &str,
) -> Result<Appearance, String> {
    if mode == 2 {
        return Ok(Appearance::Invisible);
    }
    let info = macsigner::pdf::page_info(pdf, page)?;
    let [bx0, by0, bx1, by1] = info.bounds;
    let (pw, ph) = (bx1 - bx0, by1 - by0);
    let (w, h) = (BOX_WIDTH.min(pw), BOX_HEIGHT.min(ph));
    let left = (x * pw).clamp(0.0, (pw - w).max(0.0));
    let top = (y * ph).clamp(0.0, (ph - h).max(0.0));
    let rect = [bx0 + left, by1 - top - h, bx0 + left + w, by1 - top];
    let mut stamp = text(template, fields, mode, note);
    let image = if mode == 0 && template.appearance != 2 && !template.image_path.is_empty() {
        Some(
            std::fs::read(&template.image_path)
                .map_err(|e| format!("Không đọc được ảnh chữ ký {}: {e}", template.image_path))?,
        )
    } else {
        None
    };
    if mode == 0 && template.appearance == 1 && image.is_some() {
        stamp.clear();
    }
    Ok(Appearance::Visible {
        page,
        rect,
        text: stamp,
        image,
        image_background: false,
    })
}
