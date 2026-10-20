use anyhow::{Context, Result, ensure};
use eframe::egui;
use hayro::{
    PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
    hayro_syntax::Pdf, render, vello_cpu::color::palette::css::WHITE,
};
use std::path::Path;

pub(super) fn page_count(path: &Path) -> Result<usize> {
    let bytes = std::fs::read(path)?;
    ensure!(bytes.starts_with(b"%PDF-"), "Tệp không có định dạng PDF");
    let pdf =
        Pdf::new(bytes).map_err(|err| anyhow::anyhow!("Không phân tích được PDF: {err:?}"))?;
    Ok(pdf.pages().len())
}

pub(super) fn render_page(path: &Path, page_index: usize, zoom: f32) -> Result<egui::ColorImage> {
    let bytes =
        std::fs::read(path).with_context(|| format!("Không đọc được {}", path.display()))?;
    let pdf =
        Pdf::new(bytes).map_err(|err| anyhow::anyhow!("Không phân tích được PDF: {err:?}"))?;
    let page = pdf
        .pages()
        .get(page_index)
        .context("Số trang PDF ngoài phạm vi")?;
    let scale = zoom.clamp(0.25, 8.0);
    let pixmap = render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    ensure!(
        pixmap.width() > 0 && pixmap.height() > 0,
        "Không kết xuất được trang PDF"
    );
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data_as_u8_slice(),
    ))
}
