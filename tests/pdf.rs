// PDF signing round trips with the TEST ONLY software certificate.
use macsigner::pdf::{self, Appearance, SignOptions};
use rsa::signature::Signer;
use rsa::{
    RsaPrivateKey, pkcs1v15::SigningKey, pkcs8::DecodePrivateKey, signature::SignatureEncoding,
};

const CERT: &[u8] = include_bytes!("fixtures/ctsign/certificate.der");

fn key() -> RsaPrivateKey {
    RsaPrivateKey::from_pkcs8_pem(include_str!("fixtures/ctsign/private-key.fixture")).unwrap()
}

fn signer(data: &[u8]) -> Result<Vec<u8>, String> {
    Ok(SigningKey::<sha2::Sha256>::new(key()).sign(data).to_vec())
}

/// Two-page PDF with a classic cross-reference table.
fn classic_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 /MediaBox [0 0 595 842] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".into(),
        "<< /Length 44 >>\nstream\nBT /F1 24 Tf 72 720 Td (Trang 1) Tj ET    \nendstream".into(),
        "<< /Type /Page /Parent 2 0 R /Annots [] >>".into(),
    ];
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f\r\n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n\r\n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// One-page PDF whose cross-reference is a stream (PDF 1.5+), AcroForm indirect.
fn xref_stream_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Annots 5 0 R >>",
        "<< /Fields [] >>",
        "[]",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref_id = objects.len() + 1;
    let xref_at = out.len();
    let mut data = vec![0u8, 0, 0, 0, 0, 0xff, 0xff];
    for o in offsets.iter().chain(std::iter::once(&xref_at)) {
        data.push(1);
        data.extend_from_slice(&(*o as u32).to_be_bytes());
        data.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(
        format!(
            "{xref_id} 0 obj\n<< /Type /XRef /Size {} /W [1 4 2] /Root 1 0 R /Length {} >>\nstream\n",
            xref_id + 1,
            data.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&data);
    out.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
    out
}

fn visible(page: u32) -> SignOptions {
    SignOptions {
        appearance: Appearance::Visible {
            page,
            rect: [50.0, 50.0, 250.0, 120.0],
            text: "Người ký: CÔNG TY TNHH THỬ NGHIỆM\nNgày ký: 20/10/2026".into(),
            image: None,
            image_background: false,
        },
        reason: "Ký thử".into(),
        location: "Hồ Chí Minh".into(),
        field_name: None,
    }
}

fn assert_valid(signed: &[u8], expected: usize) {
    let reports = pdf::verify(signed);
    assert_eq!(reports.len(), expected);
    for r in &reports {
        assert!(r.valid(), "{r:?}");
        assert_eq!(r.signer, "CTSign TEST ONLY");
    }
    assert!(reports.last().unwrap().covers_whole_file);
    // Structural check by two independent readers.
    let doc = lopdf::Document::load_mem(signed).unwrap();
    assert!(!doc.get_pages().is_empty());
    let parsed = hayro::hayro_syntax::Pdf::new(signed.to_vec()).expect("hayro parses signed PDF");
    assert!(!parsed.pages().is_empty());
}

#[test]
fn signs_classic_pdf_visible_and_invisible_and_counter_signs() {
    let original = classic_pdf();
    assert_eq!(pdf::page_count(&original).unwrap(), 2);
    let once = pdf::sign(&original, &[CERT.to_vec()], &visible(2), signer).unwrap();
    assert!(
        once.starts_with(&original),
        "incremental update keeps the original bytes"
    );
    assert_valid(&once, 1);
    let twice = pdf::sign(&once, &[CERT.to_vec()], &SignOptions::default(), signer).unwrap();
    assert_valid(&twice, 2);
    let reports = pdf::verify(&twice);
    assert!(!reports[0].covers_whole_file);
    let doc = lopdf::Document::load_mem(&twice).unwrap();
    let catalog = doc.catalog().unwrap();
    let acro = catalog.get(b"AcroForm").unwrap().as_dict().unwrap();
    assert_eq!(acro.get(b"Fields").unwrap().as_array().unwrap().len(), 2);
}

#[test]
fn signs_xref_stream_pdf_with_indirect_acroform_and_annots() {
    let original = xref_stream_pdf();
    let signed = pdf::sign(&original, &[CERT.to_vec()], &visible(1), signer).unwrap();
    assert_valid(&signed, 1);
    let doc = lopdf::Document::load_mem(&signed).unwrap();
    let fields = doc
        .get_object((4, 0))
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Fields")
        .unwrap();
    assert_eq!(fields.as_array().unwrap().len(), 1);
    assert_eq!(doc.get_object((5, 0)).unwrap().as_array().unwrap().len(), 1);
}

#[test]
fn signature_with_image_stamp() {
    let mut png = Vec::new();
    let img = image::RgbaImage::from_fn(40, 40, |x, y| {
        image::Rgba([200, 20, 20, if (x + y) % 2 == 0 { 255 } else { 0 }])
    });
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut options = visible(1);
    if let Appearance::Visible { image, .. } = &mut options.appearance {
        *image = Some(png);
    }
    let signed = pdf::sign(&classic_pdf(), &[CERT.to_vec()], &options, signer).unwrap();
    assert_valid(&signed, 1);
}

#[test]
fn tampering_and_wrong_key_are_detected() {
    let signed = pdf::sign(
        &classic_pdf(),
        &[CERT.to_vec()],
        &SignOptions::default(),
        signer,
    )
    .unwrap();
    let mut tampered = signed.clone();
    let pos = tampered.windows(7).position(|w| w == b"Trang 1").unwrap();
    tampered[pos + 6] = b'2';
    let report = &pdf::verify(&tampered)[0];
    assert!(!report.digest_ok);
    assert!(!report.valid());

    let other = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
    let forged = pdf::sign(
        &classic_pdf(),
        &[CERT.to_vec()],
        &SignOptions::default(),
        |d| Ok(SigningKey::<sha2::Sha256>::new(other).sign(d).to_vec()),
    )
    .unwrap();
    let report = &pdf::verify(&forged)[0];
    assert!(report.digest_ok && !report.signature_ok);
}

#[test]
fn rejects_missing_page_and_garbage() {
    assert!(pdf::sign(&classic_pdf(), &[CERT.to_vec()], &visible(9), signer).is_err());
    assert!(
        pdf::sign(
            b"not a pdf",
            &[CERT.to_vec()],
            &SignOptions::default(),
            signer
        )
        .is_err()
    );
    assert!(pdf::verify(b"%PDF-1.4 nothing").is_empty());
}

#[test]
fn writes_sample_for_external_check() {
    // MACSIGNER_PDF_OUT=dir writes signed samples for independent verifiers.
    let Ok(dir) = std::env::var("MACSIGNER_PDF_OUT") else {
        return;
    };
    let signed = pdf::sign(&classic_pdf(), &[CERT.to_vec()], &visible(1), signer).unwrap();
    std::fs::write(format!("{dir}/signed-classic.pdf"), &signed).unwrap();
    let signed = pdf::sign(&xref_stream_pdf(), &[CERT.to_vec()], &visible(1), signer).unwrap();
    std::fs::write(format!("{dir}/signed-xrefstream.pdf"), &signed).unwrap();
}
