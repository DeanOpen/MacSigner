// Page rasterisation for the built-in PDF viewer (pure Rust, no PDFium).
use hayro::{
    PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
    hayro_syntax::Pdf, render, vello_cpu::color::palette::css::WHITE,
};

pub struct Document {
    pdf: Pdf,
}

pub struct Rendered {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA8, as egui textures expect.
    pub rgba: Vec<u8>,
}

impl Document {
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        let pdf = Pdf::new(bytes).map_err(|e| format!("không mở được PDF: {e:?}"))?;
        Ok(Self { pdf })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Page size in points after rotation, as displayed.
    pub fn page_size(&self, index: usize) -> Option<(f32, f32)> {
        self.pdf.pages().get(index).map(|p| p.render_dimensions())
    }

    pub fn render(&self, index: usize, scale: f32) -> Option<Rendered> {
        let pages = self.pdf.pages();
        let page = pages.get(index)?;
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
        Some(Rendered {
            width: pixmap.width() as u32,
            height: pixmap.height() as u32,
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }
}
