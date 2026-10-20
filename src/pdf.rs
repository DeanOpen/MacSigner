// PDF signing (PAdES baseline B: ETSI.CAdES.detached, SHA-256) by incremental
// update, plus verification of the signatures a file carries.
use crate::cms;
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _, io::Write as _};

/// Bytes reserved for the CMS blob (hex-encoded in the file, so twice that).
const SIGNATURE_SPACE: usize = 24 * 1024;
const BYTE_RANGE_PLACEHOLDER: &str = "/ByteRange [0 0000000000 0000000000 0000000000]";

#[derive(Clone, Debug, PartialEq)]
pub enum Appearance {
    /// No visible stamp (Rect [0 0 0 0]).
    Invisible,
    /// Visible stamp on `page` (1-based) at `rect` = [x1, y1, x2, y2] in PDF points,
    /// origin bottom-left of the page's crop box.
    Visible {
        page: u32,
        rect: [f32; 4],
        text: String,
        /// PNG or JPEG bytes drawn on the left (or filling the box without text).
        image: Option<Vec<u8>>,
        /// Draw the image across the whole box behind the text (API type 3).
        image_background: bool,
    },
}

#[derive(Clone, Debug)]
pub struct SignOptions {
    pub appearance: Appearance,
    pub reason: String,
    pub location: String,
    pub field_name: Option<String>,
}

impl Default for SignOptions {
    fn default() -> Self {
        Self {
            appearance: Appearance::Invisible,
            reason: String::new(),
            location: String::new(),
            field_name: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PageInfo {
    /// Crop box [x1, y1, x2, y2].
    pub bounds: [f32; 4],
    pub rotate: i64,
}

pub fn page_count(pdf: &[u8]) -> Result<usize, String> {
    Ok(load(pdf)?.get_pages().len())
}

pub fn page_info(pdf: &[u8], page: u32) -> Result<PageInfo, String> {
    let doc = load(pdf)?;
    let id = *doc
        .get_pages()
        .get(&page)
        .ok_or_else(|| format!("không có trang {page}"))?;
    page_geometry(&doc, id)
}

fn load(pdf: &[u8]) -> Result<Document, String> {
    let doc = Document::load_mem(pdf).map_err(|e| format!("không đọc được PDF: {e}"))?;
    if doc.is_encrypted() {
        return Err("PDF có mật khẩu/mã hóa, chưa hỗ trợ ký".into());
    }
    Ok(doc)
}

fn inherited<'a>(doc: &'a Document, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..32 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return doc.dereference(value).ok().map(|(_, v)| v);
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn page_geometry(doc: &Document, id: ObjectId) -> Result<PageInfo, String> {
    let rect = |key: &[u8]| -> Option<[f32; 4]> {
        let array = inherited(doc, id, key)?.as_array().ok()?;
        let values: Vec<f32> = array.iter().filter_map(number).collect();
        (values.len() == 4).then(|| {
            [
                values[0].min(values[2]),
                values[1].min(values[3]),
                values[0].max(values[2]),
                values[1].max(values[3]),
            ]
        })
    };
    let bounds = rect(b"CropBox")
        .or_else(|| rect(b"MediaBox"))
        .unwrap_or([0.0, 0.0, 595.0, 842.0]);
    let rotate = inherited(doc, id, b"Rotate")
        .and_then(|r| r.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    Ok(PageInfo { bounds, rotate })
}

// ---- Serialisation -----------------------------------------------------------

fn write_name(out: &mut Vec<u8>, name: &[u8]) {
    out.push(b'/');
    for &b in name {
        if b.is_ascii_alphanumeric() || b"-_.+:;,@*'\"".contains(&b) {
            out.push(b);
        } else {
            let _ = write!(out, "#{b:02X}");
        }
    }
}

fn write_object(out: &mut Vec<u8>, object: &Object) {
    match object {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Object::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Object::Real(r) => {
            let _ = write!(out, "{}", fmt_num(*r));
        }
        Object::Name(n) => write_name(out, n),
        Object::String(bytes, format) => match format {
            StringFormat::Hexadecimal => {
                out.push(b'<');
                for b in bytes {
                    let _ = write!(out, "{b:02X}");
                }
                out.push(b'>');
            }
            StringFormat::Literal => out.extend_from_slice(&literal(bytes)),
        },
        Object::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_object(out, item);
            }
            out.push(b']');
        }
        Object::Dictionary(dict) => write_dict(out, dict),
        // Only dictionaries/arrays of existing objects are rewritten; streams are
        // never copied, so this arm is unreachable in practice.
        Object::Stream(stream) => write_dict(out, &stream.dict),
        Object::Reference((n, g)) => {
            let _ = write!(out, "{n} {g} R");
        }
    }
}

fn write_dict(out: &mut Vec<u8>, dict: &Dictionary) {
    out.extend_from_slice(b"<<");
    for (key, value) in dict.iter() {
        write_name(out, key);
        out.push(b' ');
        write_object(out, value);
    }
    out.extend_from_slice(b">>");
}

fn literal(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![b'('];
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\n' => out.extend_from_slice(b"\\n"),
            _ => out.push(b),
        }
    }
    out.push(b')');
    out
}

/// PDF text string: UTF-16BE with BOM, so Vietnamese survives in every viewer.
fn text_string(text: &str) -> Object {
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn fmt_num(value: f32) -> String {
    let s = format!("{value:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

fn decode_text(object: &Object) -> String {
    match object {
        Object::String(bytes, _) if bytes.starts_with(&[0xfe, 0xff]) => {
            let units: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        Object::String(bytes, _) => bytes.iter().map(|b| *b as char).collect(),
        _ => String::new(),
    }
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn previous_startxref(pdf: &[u8]) -> Result<usize, String> {
    let pos = find_last(pdf, b"startxref").ok_or("PDF thiếu startxref")?;
    let tail = std::str::from_utf8(&pdf[pos + 9..]).unwrap_or("");
    tail.split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| "startxref không hợp lệ".into())
}

// ---- Appearance --------------------------------------------------------------

/// System fonts with Vietnamese coverage, as the original uses times.ttf.
fn system_font() -> Option<Vec<u8>> {
    [
        "C:\\Windows\\Fonts\\times.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
        "/usr/share/fonts/liberation/LiberationSerif-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        "/usr/share/fonts/TTF/DejaVuSerif.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .find_map(|p| std::fs::read(p).ok())
}

/// Emits glyph outlines as PDF path operators, so no font has to be embedded.
struct PathWriter<'a> {
    out: &'a mut String,
    scale: f32,
    dx: f32,
    dy: f32,
    last: (f32, f32),
}

impl PathWriter<'_> {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        (self.dx + x * self.scale, self.dy + y * self.scale)
    }
}

impl ttf_parser::OutlineBuilder for PathWriter<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.p(x, y);
        let _ = write!(self.out, "{} {} m ", fmt_num(a), fmt_num(b));
        self.last = (x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (a, b) = self.p(x, y);
        let _ = write!(self.out, "{} {} l ", fmt_num(a), fmt_num(b));
        self.last = (x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.last;
        let c1 = (x0 + 2.0 / 3.0 * (x1 - x0), y0 + 2.0 / 3.0 * (y1 - y0));
        let c2 = (x + 2.0 / 3.0 * (x1 - x), y + 2.0 / 3.0 * (y1 - y));
        self.curve_to(c1.0, c1.1, c2.0, c2.1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b) = self.p(x1, y1);
        let (c, d) = self.p(x2, y2);
        let (e, f) = self.p(x, y);
        let _ = write!(
            self.out,
            "{} {} {} {} {} {} c ",
            fmt_num(a),
            fmt_num(b),
            fmt_num(c),
            fmt_num(d),
            fmt_num(e),
            fmt_num(f)
        );
        self.last = (x, y);
    }
    fn close(&mut self) {
        self.out.push_str("h ");
    }
}

fn text_width(face: &ttf_parser::Face<'_>, text: &str, size: f32) -> f32 {
    let scale = size / face.units_per_em() as f32;
    text.chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .unwrap_or(face.units_per_em() / 2) as f32
                * scale
        })
        .sum()
}

fn wrap(face: &ttf_parser::Face<'_>, text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if text_width(face, &candidate, size) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            }
        }
        lines.push(line);
    }
    lines
}

/// Draws `text` inside the box as filled glyph outlines; shrinks to fit.
fn draw_text(out: &mut String, font: &[u8], text: &str, x: f32, y: f32, w: f32, h: f32) {
    let Ok(face) = ttf_parser::Face::parse(font, 0) else {
        return;
    };
    let mut size = 10.0f32;
    let mut lines = wrap(&face, text, size, w);
    while size > 4.0 && lines.len() as f32 * size * 1.25 > h {
        size -= 0.5;
        lines = wrap(&face, text, size, w);
    }
    let scale = size / face.units_per_em() as f32;
    out.push_str("q 0 0 0 rg ");
    let mut baseline = y + h - size;
    for line in lines {
        let mut cursor = x;
        for c in line.chars() {
            let Some(glyph) = face.glyph_index(c) else {
                cursor += size / 2.0;
                continue;
            };
            let mut writer = PathWriter {
                out,
                scale,
                dx: cursor,
                dy: baseline,
                last: (0.0, 0.0),
            };
            face.outline_glyph(glyph, &mut writer);
            cursor += face.glyph_hor_advance(glyph).unwrap_or(0) as f32 * scale;
        }
        out.push_str("f ");
        baseline -= size * 1.25;
    }
    out.push_str("Q ");
}

struct EncodedImage {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

fn decode_image(bytes: &[u8]) -> Result<EncodedImage, String> {
    let img =
        image::load_from_memory(bytes).map_err(|e| format!("hình chữ ký không đọc được: {e}"))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    let mut alpha = Vec::with_capacity((width * height) as usize);
    for p in rgba.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p.0[3]);
    }
    let opaque = alpha.iter().all(|a| *a == 255);
    Ok(EncodedImage {
        width,
        height,
        rgb: deflate(&rgb),
        alpha: (!opaque).then(|| deflate(&alpha)),
    })
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = encoder.write_all(data);
    encoder.finish().unwrap_or_default()
}

// ---- Signing -----------------------------------------------------------------

struct Writer {
    buf: Vec<u8>,
    offsets: BTreeMap<u32, usize>,
}

impl Writer {
    fn begin(&mut self, id: u32) {
        self.offsets.insert(id, self.buf.len());
        let _ = writeln!(self.buf, "{id} 0 obj");
    }
    fn end(&mut self) {
        self.buf.extend_from_slice(b"\nendobj\n");
    }
    fn dict_object(&mut self, id: u32, object: &Object) {
        self.begin(id);
        write_object(&mut self.buf, object);
        self.end();
    }
    fn stream_object(&mut self, id: u32, dict: &mut Dictionary, data: &[u8]) {
        dict.set("Length", data.len() as i64);
        self.begin(id);
        write_dict(&mut self.buf, dict);
        self.buf.extend_from_slice(b"\nstream\n");
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream");
        self.end();
    }
}

fn pdf_date(now: chrono::DateTime<chrono::Local>) -> String {
    let offset = now.offset().local_minus_utc();
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.abs();
    format!(
        "D:{}{}{:02}'{:02}'",
        now.format("%Y%m%d%H%M%S"),
        sign,
        offset / 3600,
        (offset % 3600) / 60
    )
}

/// Signs `pdf`. `sign` receives DER signed attributes and must return an RSA
/// PKCS#1 v1.5 SHA-256 signature made with the key of `chain[0]`.
pub fn sign(
    pdf: &[u8],
    chain_leaf_first: &[Vec<u8>],
    options: &SignOptions,
    sign: impl FnOnce(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let doc = load(pdf)?;
    let root_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| "PDF thiếu /Root")?;
    let size = doc
        .trailer
        .get(b"Size")
        .and_then(Object::as_i64)
        .map_err(|_| "PDF thiếu /Size")? as u32;
    let size = size.max(doc.max_id + 1);
    let prev = previous_startxref(pdf)?;
    let classic_xref = pdf
        .get(prev..)
        .is_some_and(|tail| tail.trim_ascii_start().starts_with(b"xref"));
    let pages = doc.get_pages();
    let (page_no, rect) = match &options.appearance {
        Appearance::Invisible => (1, [0.0; 4]),
        Appearance::Visible { page, rect, .. } => (*page, *rect),
    };
    let page_id = *pages
        .get(&page_no)
        .ok_or_else(|| format!("PDF không có trang {page_no}"))?;

    // Existing form fields decide the new field's unique name.
    let mut catalog = doc
        .get_dictionary(root_id)
        .map_err(|e| e.to_string())?
        .clone();
    let (acro_id, mut acro) = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(id)) => (
            Some(*id),
            doc.get_dictionary(*id).map_err(|e| e.to_string())?.clone(),
        ),
        Ok(Object::Dictionary(d)) => (None, d.clone()),
        _ => (None, Dictionary::new()),
    };
    let mut fields = match acro.get(b"Fields") {
        Ok(obj) => doc
            .dereference(obj)
            .ok()
            .and_then(|(_, v)| v.as_array().ok().cloned())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let names: Vec<String> = fields
        .iter()
        .filter_map(|f| f.as_reference().ok())
        .filter_map(|id| doc.get_dictionary(id).ok())
        .filter_map(|d| d.get(b"T").ok().map(decode_text))
        .collect();
    let field_name = match &options.field_name {
        Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
        _ => (1..)
            .map(|i| format!("Signature{i}"))
            .find(|n| !names.contains(n))
            .unwrap(),
    };

    let sig_id = size;
    let widget_id = size + 1;
    let ap_id = size + 2;
    let mut next = size + 3;

    let mut writer = Writer {
        buf: pdf.to_vec(),
        offsets: BTreeMap::new(),
    };
    if !writer.buf.ends_with(b"\n") {
        writer.buf.push(b'\n');
    }

    // Signature dictionary with fixed-size placeholders filled in afterwards.
    let now = chrono::Local::now();
    let leaf_name = signer_name(&chain_leaf_first[0]);
    writer.begin(sig_id);
    writer
        .buf
        .extend_from_slice(b"<</Type /Sig /Filter /Adobe.PPKLite /SubFilter /ETSI.CAdES.detached ");
    let byte_range_at = writer.buf.len();
    writer
        .buf
        .extend_from_slice(BYTE_RANGE_PLACEHOLDER.as_bytes());
    writer.buf.extend_from_slice(b" /Contents ");
    let contents_start = writer.buf.len();
    writer.buf.push(b'<');
    writer
        .buf
        .extend(std::iter::repeat_n(b'0', SIGNATURE_SPACE * 2));
    writer.buf.push(b'>');
    let contents_end = writer.buf.len();
    let mut extra = Dictionary::new();
    extra.set(
        "M",
        Object::String(pdf_date(now).into_bytes(), StringFormat::Literal),
    );
    if !leaf_name.is_empty() {
        extra.set("Name", text_string(&leaf_name));
    }
    if !options.reason.is_empty() {
        extra.set("Reason", text_string(&options.reason));
    }
    if !options.location.is_empty() {
        extra.set("Location", text_string(&options.location));
    }
    for (key, value) in extra.iter() {
        writer.buf.push(b' ');
        write_name(&mut writer.buf, key);
        writer.buf.push(b' ');
        write_object(&mut writer.buf, value);
    }
    writer.buf.extend_from_slice(b">>");
    writer.end();

    // Widget annotation that is also the signature field.
    let mut widget = Dictionary::new();
    widget.set("Type", Object::Name(b"Annot".to_vec()));
    widget.set("Subtype", Object::Name(b"Widget".to_vec()));
    widget.set("FT", Object::Name(b"Sig".to_vec()));
    widget.set("T", text_string(&field_name));
    widget.set("V", Object::Reference((sig_id, 0)));
    widget.set("F", 132i64);
    widget.set("P", Object::Reference(page_id));
    widget.set(
        "Rect",
        Object::Array(rect.iter().map(|v| Object::Real(*v)).collect()),
    );
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference((ap_id, 0)));
    widget.set("AP", Object::Dictionary(ap));
    writer.dict_object(widget_id, &Object::Dictionary(widget));

    // Appearance stream.
    let (w, h) = ((rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs());
    let mut content = String::new();
    let mut resources = Dictionary::new();
    if let Appearance::Visible {
        text,
        image,
        image_background,
        ..
    } = &options.appearance
    {
        let mut text_x = 2.0;
        if let Some(bytes) = image {
            let img = decode_image(bytes)?;
            let mut idict = Dictionary::new();
            idict.set("Type", Object::Name(b"XObject".to_vec()));
            idict.set("Subtype", Object::Name(b"Image".to_vec()));
            idict.set("Width", img.width as i64);
            idict.set("Height", img.height as i64);
            idict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            idict.set("BitsPerComponent", 8i64);
            idict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
            if let Some(alpha) = &img.alpha {
                let smask_id = next;
                next += 1;
                let mut sdict = Dictionary::new();
                sdict.set("Type", Object::Name(b"XObject".to_vec()));
                sdict.set("Subtype", Object::Name(b"Image".to_vec()));
                sdict.set("Width", img.width as i64);
                sdict.set("Height", img.height as i64);
                sdict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
                sdict.set("BitsPerComponent", 8i64);
                sdict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
                writer.stream_object(smask_id, &mut sdict, alpha);
                idict.set("SMask", Object::Reference((smask_id, 0)));
            }
            let image_id = next;
            next += 1;
            writer.stream_object(image_id, &mut idict, &img.rgb);
            let mut xobjects = Dictionary::new();
            xobjects.set("Im1", Object::Reference((image_id, 0)));
            resources.set("XObject", Object::Dictionary(xobjects));
            // Image on the left part (or the whole box without text), aspect kept.
            let area_w = if text.trim().is_empty() || *image_background {
                w
            } else {
                w * 0.4
            };
            let ratio = (area_w / img.width as f32).min(h / img.height as f32);
            let (iw, ih) = (img.width as f32 * ratio, img.height as f32 * ratio);
            let _ = write!(
                content,
                "q {} 0 0 {} {} {} cm /Im1 Do Q ",
                fmt_num(iw),
                fmt_num(ih),
                fmt_num((area_w - iw) / 2.0),
                fmt_num((h - ih) / 2.0)
            );
            text_x = if *image_background { 2.0 } else { area_w + 4.0 };
        }
        if !text.trim().is_empty() {
            // "Signed by" bracket: a blue rule with short ticks left of the text.
            let (bx, top, bottom) = (text_x + 1.0, h - 3.0, 3.0);
            let _ = write!(
                content,
                "q 0.059 0.384 0.996 RG 1.4 w 2 J {x} {t} m {x} {b} l S {x} {t} m {x2} {t} l S {x} {b} m {x2} {b} l S Q ",
                x = fmt_num(bx),
                t = fmt_num(top),
                b = fmt_num(bottom),
                x2 = fmt_num(bx + 7.0),
            );
            let text_x = text_x + 6.0;
            match system_font() {
                Some(font) => draw_text(
                    &mut content,
                    &font,
                    text,
                    text_x,
                    4.0,
                    (w - text_x - 2.0).max(10.0),
                    (h - 8.0).max(6.0),
                ),
                None => return Err("không tìm thấy font hệ thống có tiếng Việt".into()),
            }
        }
    }
    let mut apdict = Dictionary::new();
    apdict.set("Type", Object::Name(b"XObject".to_vec()));
    apdict.set("Subtype", Object::Name(b"Form".to_vec()));
    apdict.set(
        "BBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(w),
            Object::Real(h),
        ]),
    );
    apdict.set("Resources", Object::Dictionary(resources));
    apdict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    writer.stream_object(ap_id, &mut apdict, &deflate(content.as_bytes()));

    // Page /Annots gains the widget.
    let mut page = doc
        .get_dictionary(page_id)
        .map_err(|e| e.to_string())?
        .clone();
    match page.get(b"Annots") {
        Ok(Object::Reference(id)) => {
            let id = *id;
            let mut annots = doc
                .get_object(id)
                .and_then(Object::as_array)
                .cloned()
                .unwrap_or_default();
            annots.push(Object::Reference((widget_id, 0)));
            writer.dict_object(id.0, &Object::Array(annots));
        }
        Ok(Object::Array(a)) => {
            let mut annots = a.clone();
            annots.push(Object::Reference((widget_id, 0)));
            page.set("Annots", Object::Array(annots));
            writer.dict_object(page_id.0, &Object::Dictionary(page));
        }
        _ => {
            page.set(
                "Annots",
                Object::Array(vec![Object::Reference((widget_id, 0))]),
            );
            writer.dict_object(page_id.0, &Object::Dictionary(page));
        }
    }

    // AcroForm lists the field; SigFlags 3 = SignaturesExist | AppendOnly.
    fields.push(Object::Reference((widget_id, 0)));
    acro.set("Fields", Object::Array(fields));
    acro.set("SigFlags", 3i64);
    acro.remove(b"NeedAppearances");
    match acro_id {
        Some(id) => writer.dict_object(id.0, &Object::Dictionary(acro)),
        None => {
            catalog.set("AcroForm", Object::Dictionary(acro));
            writer.dict_object(root_id.0, &Object::Dictionary(catalog));
        }
    }

    // Cross-reference section in the same style as the original file.
    let mut trailer = Dictionary::new();
    trailer.set("Root", Object::Reference(root_id));
    for key in [b"Info".as_slice(), b"ID"] {
        if let Ok(value) = doc.trailer.get(key) {
            trailer.set(key.to_vec(), value.clone());
        }
    }
    trailer.set("Prev", prev as i64);
    if classic_xref {
        let xref_at = writer.buf.len();
        writer.buf.extend_from_slice(b"xref\n");
        for (id, offset) in &writer.offsets {
            let _ = write!(writer.buf, "{id} 1\n{offset:010} 00000 n\r\n");
        }
        trailer.set("Size", next as i64);
        writer.buf.extend_from_slice(b"trailer\n");
        write_dict(&mut writer.buf, &trailer);
        let _ = write!(writer.buf, "\nstartxref\n{xref_at}\n%%EOF\n");
    } else {
        let xref_id = next;
        next += 1;
        let xref_at = writer.buf.len();
        writer.offsets.insert(xref_id, xref_at);
        let mut index = Vec::new();
        let mut data = Vec::new();
        for (id, offset) in &writer.offsets {
            index.push(Object::Integer(*id as i64));
            index.push(Object::Integer(1));
            data.push(1u8);
            data.extend_from_slice(&(*offset as u32).to_be_bytes());
            data.extend_from_slice(&0u16.to_be_bytes());
        }
        trailer.set("Type", Object::Name(b"XRef".to_vec()));
        trailer.set("Size", next as i64);
        trailer.set("W", Object::Array(vec![1.into(), 4.into(), 2.into()]));
        trailer.set("Index", Object::Array(index));
        let _ = writeln!(writer.buf, "{xref_id} 0 obj");
        trailer.set("Length", data.len() as i64);
        write_dict(&mut writer.buf, &trailer);
        writer.buf.extend_from_slice(b"\nstream\n");
        writer.buf.extend_from_slice(&data);
        writer.buf.extend_from_slice(b"\nendstream\nendobj\n");
        let _ = write!(writer.buf, "startxref\n{xref_at}\n%%EOF\n");
    }

    // Fill ByteRange, hash everything except /Contents, then embed the CMS.
    let mut buf = writer.buf;
    let total = buf.len();
    let range = format!(
        "/ByteRange [0 {contents_start} {contents_end} {}]",
        total - contents_end
    );
    if range.len() > BYTE_RANGE_PLACEHOLDER.len() {
        return Err("PDF quá lớn".into());
    }
    let padded = format!("{range:<width$}", width = BYTE_RANGE_PLACEHOLDER.len());
    buf[byte_range_at..byte_range_at + padded.len()].copy_from_slice(padded.as_bytes());
    let mut hasher = Sha256::new();
    hasher.update(&buf[..contents_start]);
    hasher.update(&buf[contents_end..]);
    let digest = hasher.finalize();
    let cms = cms::sign_detached(&digest, chain_leaf_first, sign)?;
    if cms.len() > SIGNATURE_SPACE {
        return Err("chữ ký vượt quá vùng dành sẵn".into());
    }
    let mut hex = String::with_capacity(cms.len() * 2);
    for b in &cms {
        let _ = write!(hex, "{b:02X}");
    }
    buf[contents_start + 1..contents_start + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(buf)
}

fn signer_name(der: &[u8]) -> String {
    x509_parser::parse_x509_certificate(der)
        .ok()
        .and_then(|(_, c)| {
            c.subject()
                .iter_common_name()
                .next()
                .and_then(|cn| cn.as_str().ok().map(String::from))
        })
        .unwrap_or_default()
}

/// Default stamp text, matching the original API's type 5.
pub fn default_text(leaf: &[u8]) -> String {
    format!(
        "Người ký: {}\nNgày ký: {}",
        signer_name(leaf),
        chrono::Local::now().format("%d/%m/%Y")
    )
}

// ---- Verification ------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SignatureReport {
    pub signer: String,
    pub issuer: String,
    pub serial: String,
    pub signing_time: String,
    /// The signed byte ranges reach the end of the file (no later changes).
    pub covers_whole_file: bool,
    pub digest_ok: bool,
    pub signature_ok: bool,
    pub certificate_valid_now: bool,
    pub error: Option<String>,
}

impl SignatureReport {
    pub fn valid(&self) -> bool {
        self.digest_ok && self.signature_ok && self.error.is_none()
    }
}

fn parse_byte_range(pdf: &[u8], at: usize) -> Option<[usize; 4]> {
    let open = at + pdf[at..].iter().position(|b| *b == b'[')?;
    let close = open + pdf[open..].iter().position(|b| *b == b']')?;
    let numbers: Vec<usize> = std::str::from_utf8(&pdf[open + 1..close])
        .ok()?
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    (numbers.len() == 4).then(|| [numbers[0], numbers[1], numbers[2], numbers[3]])
}

fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect()
}

pub fn verify(pdf: &[u8]) -> Vec<SignatureReport> {
    let mut reports = Vec::new();
    for at in find_all(pdf, b"/ByteRange") {
        let Some([a, b, c, d]) = parse_byte_range(pdf, at) else {
            continue;
        };
        if a != 0 || b >= c || c.checked_add(d).is_none_or(|end| end > pdf.len()) {
            continue;
        }
        let date = pdf_field_near(pdf, at, b"/M");
        reports.push(check_one(pdf, [b, c, d], date));
    }
    reports
}

fn pdf_field_near(pdf: &[u8], at: usize, key: &[u8]) -> String {
    let start = at.saturating_sub(SIGNATURE_SPACE * 2 + 4096);
    let end = (at + SIGNATURE_SPACE * 2 + 4096).min(pdf.len());
    let window = &pdf[start..end];
    let Some(pos) = find_all(window, key).into_iter().find(|p| {
        window
            .get(p + key.len())
            .is_some_and(|c| *c == b' ' || *c == b'(')
    }) else {
        return String::new();
    };
    let rest = &window[pos..];
    let Some(open) = rest.iter().position(|b| *b == b'(') else {
        return String::new();
    };
    let Some(close) = rest[open..].iter().position(|b| *b == b')') else {
        return String::new();
    };
    let raw = String::from_utf8_lossy(&rest[open + 1..open + close]).to_string();
    // D:YYYYMMDDHHmmSS → dd/MM/yyyy HH:mm:ss
    let digits: String = raw.chars().skip(2).take(14).collect();
    if digits.len() == 14 {
        format!(
            "{}/{}/{} {}:{}:{}",
            &digits[6..8],
            &digits[4..6],
            &digits[0..4],
            &digits[8..10],
            &digits[10..12],
            &digits[12..14]
        )
    } else {
        raw
    }
}

fn check_one(pdf: &[u8], [b, c, d]: [usize; 3], signing_time: String) -> SignatureReport {
    let mut report = SignatureReport {
        signer: String::new(),
        issuer: String::new(),
        serial: String::new(),
        signing_time,
        covers_whole_file: c + d == pdf.len(),
        digest_ok: false,
        signature_ok: false,
        certificate_valid_now: false,
        error: None,
    };
    let result = (|| -> Result<(), String> {
        let hex = pdf.get(b + 1..c - 1).ok_or("vùng /Contents không hợp lệ")?;
        let hex: Vec<u8> = hex
            .iter()
            .copied()
            .filter(|x| !x.is_ascii_whitespace())
            .collect();
        let mut der = Vec::with_capacity(hex.len() / 2);
        for pair in hex.chunks(2) {
            let s = std::str::from_utf8(pair).map_err(|_| "hex lỗi")?;
            der.push(u8::from_str_radix(s, 16).map_err(|_| "hex lỗi")?);
        }
        let len = cms::der_length(&der)?;
        let parsed = cms::parse(&der[..len])?;
        let cert = parsed
            .certificates
            .iter()
            .find(|cert| {
                x509_parser::parse_x509_certificate(cert)
                    .is_ok_and(|(_, c)| c.tbs_certificate.raw_serial() == parsed.signer_serial)
            })
            .or(parsed.certificates.first())
            .ok_or("chữ ký không kèm chứng thư")?;
        let (_, x509) = x509_parser::parse_x509_certificate(cert).map_err(|e| e.to_string())?;
        report.signer = x509
            .subject()
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap_or("")
            .to_owned();
        report.issuer = x509
            .issuer()
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .unwrap_or("")
            .to_owned();
        report.serial = hex::encode_upper(x509.raw_serial());
        report.certificate_valid_now = x509.validity().is_valid();
        if parsed.digest_oid != cms::OID_SHA256 {
            return Err("chỉ kiểm tra được chữ ký SHA-256".into());
        }
        let mut hasher = Sha256::new();
        hasher.update(&pdf[..b]);
        hasher.update(&pdf[c..c + d]);
        report.digest_ok = hasher.finalize().as_slice() == parsed.message_digest.as_slice();
        use rsa::{
            pkcs1v15::{Signature, VerifyingKey},
            signature::Verifier,
        };
        let key = crate::esign::certificate::rsa_public_key(cert).map_err(|e| e.to_string())?;
        let signature =
            Signature::try_from(parsed.signature.as_slice()).map_err(|e| e.to_string())?;
        report.signature_ok = VerifyingKey::<Sha256>::new(key)
            .verify(&parsed.signed_attributes, &signature)
            .is_ok();
        Ok(())
    })();
    report.error = result.err();
    report
}
