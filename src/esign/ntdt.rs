//! NTDT_CER_001 and NTDT_CER_002.
//!
//! eSigner 1.0.9 (`XMLSigner.signXMLFileXPath` / `XMLMultiSigner`) does not
//! return a raw RSA signature. It appends an XMLDSig signature to each XML
//! document and returns that document, base64-encoded. `NTDT_SIGN_CODE`
//! selects which documents are in `BASE64_VALUE` and which XPath is signed:
//!
//! - `01` đăng ký: `bankXmls@gdtXml` (banks may be empty or comma-separated)
//! - `02` thêm ngân hàng, `03` ngừng, `04` thay đổi: comma-separated XML
//! - `05` nhiều nơi ký: same `@` split as `01`, XPaths taken from the request
//!
//! NTDT_CER_001 (giấy nộp tiền) signs the single `BASE64_VALUE` document at
//! `GDT_SIGN_PATH`.
//!
//! The signature profile follows that plugin: XPath Filter 2 intersect on the
//! last element of the path, exclusive canonicalization with comments,
//! `DateTimeStamp`, and KeyInfo with the RSA key plus the certificate. SHA-1
//! is used only when the certificate itself is SHA-1; SHA-256 likewise. The
//! plugin's "success" reply that contains `LOI MIA ROI` after a signing error
//! is not reproduced: a failed signature answers FALSE.
use crate::esign::{
    certificate, legacy,
    online::{self, Algorithm},
    protocol,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rsa::{
    pkcs1v15::{Signature, VerifyingKey},
    signature::Verifier,
    traits::PublicKeyParts,
};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::error::Error;
use xml_sec::c14n::{C14nAlgorithm, C14nMode, canonicalize};
use xml_sec::{Document, Node};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const XPATH_BANK: &str = "/DKY_NTHUE_NH/DKY_NTHUE";
const XPATH_REGISTER: &str = "/DKY_DICHVU/TTIN_DANGKY";
const XPATH_STOP: &str = "/TBAO_NGUNG/TBAO_NGUNG_NT";
const XPATH_MODIFY: &str = "/TDOI_TKHOAN/TDOI_TKHOAN_NT";

struct Job {
    xml: Vec<u8>,
    xpath: String,
}

/// Rejects a request whose XML or XPath cannot be signed, before a PIN prompt.
pub fn prepare(input: &serde_json::Value) -> Result<()> {
    build_jobs(input).map(|_| ())
}

/// Documents this request will sign, checked before the user is asked for a PIN.
fn build_jobs(input: &serde_json::Value) -> Result<Vec<Job>> {
    let code = required(input, "CODE")?.to_ascii_uppercase();
    let value = required(input, "BASE64_VALUE")?;
    let jobs = match code.as_str() {
        "NTDT_CER_001" => vec![Job {
            xml: decode_xml(value)?,
            xpath: required(input, "GDT_SIGN_PATH")?.to_owned(),
        }],
        "NTDT_CER_002" => jobs_002(input, value)?,
        _ => return Err("not an NTDT signing command".into()),
    };
    if jobs.is_empty() || jobs.len() > 100 {
        return Err("invalid signing input count".into());
    }
    for job in &jobs {
        // Reject a missing node before asking for a certificate or PIN.
        canonical_signed(&job.xml, &job.xpath)?;
    }
    Ok(jobs)
}

pub fn document_count(input: &serde_json::Value) -> usize {
    build_jobs(input).map(|j| j.len()).unwrap_or(1)
}

fn jobs_002(input: &serde_json::Value, value: &str) -> Result<Vec<Job>> {
    let code = required(input, "NTDT_SIGN_CODE")?;
    match code.to_ascii_uppercase().as_str() {
        "01" => split_pair(value, XPATH_BANK, XPATH_REGISTER),
        "02" => split_list(value, XPATH_BANK),
        "03" => split_list(value, XPATH_STOP),
        "04" => split_list(value, XPATH_MODIFY),
        "05" => {
            let bank = required(input, "BANK_SIGN_PATH")?;
            let gdt = required(input, "GDT_SIGN_PATH")?;
            let value = if value.contains('@') {
                value.to_owned()
            } else {
                format!("@{value}")
            };
            split_pair(&value, bank, gdt)
        }
        _ => Err(format!("NTDT_SIGN_CODE {code} is not supported").into()),
    }
}

fn split_list(value: &str, xpath: &str) -> Result<Vec<Job>> {
    if value.is_empty() {
        return Err("BASE64_VALUE is empty".into());
    }
    value
        .split(',')
        .map(|part| {
            Ok(Job {
                xml: decode_xml(part)?,
                xpath: xpath.to_owned(),
            })
        })
        .collect()
}

/// `banks@gdt`, where `banks` is empty or a comma-separated list. Matches the
/// plugin's `split("@")` and then `split(",")` of the first piece.
fn split_pair(value: &str, bank_xpath: &str, gdt_xpath: &str) -> Result<Vec<Job>> {
    let Some((banks, gdt)) = value.split_once('@') else {
        return Err("NTDT registration payload must contain '@'".into());
    };
    let mut jobs = Vec::new();
    if !banks.is_empty() {
        jobs.extend(split_list(banks, bank_xpath)?);
    }
    jobs.push(Job {
        xml: decode_xml(gdt)?,
        xpath: gdt_xpath.to_owned(),
    });
    Ok(jobs)
}

fn decode_xml(value: &str) -> Result<Vec<u8>> {
    let xml = online::decode_input(value)?;
    if std::str::from_utf8(&xml).is_err() {
        return Err("XML document is not UTF-8".into());
    }
    Ok(xml)
}

fn required<'a>(input: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    input
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing {field}").into())
}

/// Signs every document. `signed_at` is `ddMMyyyyHHmmss` written into
/// `DateTimeStamp` (the plugin uses the machine clock, not DATE_SIGNING).
pub fn sign_request<F>(
    input: &serde_json::Value,
    cert_der: &[u8],
    signed_at: &str,
    mut signer: F,
) -> Result<serde_json::Value>
where
    F: FnMut(&[u8], Algorithm) -> Result<Vec<u8>>,
{
    protocol::validate(input)?;
    legacy::check_certificate(input, cert_der)?;
    let code = required(input, "CODE")?;
    let algorithm = certificate::signature_algorithm(cert_der)?;
    if !matches!(algorithm, Algorithm::Sha1Rsa | Algorithm::Sha256Rsa) {
        return Err(
            "NTDT signing requires a SHA-1 or SHA-256 RSA certificate, as in eSigner".into(),
        );
    }
    if !signed_at.chars().all(|c| c.is_ascii_digit()) || signed_at.len() != 14 {
        return Err("signing timestamp must be ddMMyyyyHHmmss".into());
    }
    let jobs = build_jobs(input)?;
    let mut signed = Vec::with_capacity(jobs.len());
    for job in &jobs {
        signed.push(sign_xml(
            &job.xml,
            &job.xpath,
            cert_der,
            signed_at,
            &mut signer,
            algorithm,
        )?);
    }
    let joined = join_signed(input, &signed)?;
    let mut payload = serde_json::Map::new();
    payload.insert("SIGNED_CONTENT".into(), serde_json::Value::String(joined));
    // NTDT_CER_001's plugin reply includes these three fields, always empty:
    // the certificate travels inside the signed XML. NTDT_CER_002 omits them.
    if code.eq_ignore_ascii_case("NTDT_CER_001") {
        payload.insert("CERT".into(), serde_json::Value::String(String::new()));
        payload.insert(
            "CERT_CHAIN".into(),
            serde_json::Value::String(String::new()),
        );
        payload.insert("PIN_CODE".into(), serde_json::Value::String(String::new()));
    }
    Ok(protocol::response(code, true, payload))
}

/// Rebuild `bank,bank@gdt` or `a,b,c` from the signed documents, in job order.
fn join_signed(input: &serde_json::Value, signed: &[String]) -> Result<String> {
    let code = required(input, "CODE")?.to_ascii_uppercase();
    if code == "NTDT_CER_001" {
        return Ok(signed[0].clone());
    }
    let variant = required(input, "NTDT_SIGN_CODE")?.to_ascii_uppercase();
    if matches!(variant.as_str(), "02" | "03" | "04") {
        return Ok(signed.join(","));
    }
    // 01 and 05: banks first, the GDT document last, separated by '@'.
    let gdt = signed.last().ok_or("missing signed document")?.clone();
    let banks = signed[..signed.len() - 1].join(",");
    Ok(format!("{banks}@{gdt}"))
}

fn sign_xml<F>(
    xml: &[u8],
    xpath: &str,
    cert_der: &[u8],
    signed_at: &str,
    signer: &mut F,
    algorithm: Algorithm,
) -> Result<String>
where
    F: FnMut(&[u8], Algorithm) -> Result<Vec<u8>>,
{
    let canonical = canonical_signed(xml, xpath)?;
    let filter = format!("//{}", xpath_local_name(xpath)?);
    let digest_b64 = STANDARD.encode(digest_bytes(&canonical, algorithm));
    let property = time_property_c14n(signed_at);
    let property_digest = STANDARD.encode(digest_bytes(property.as_bytes(), algorithm));
    let (dig_uri, sig_uri) = match algorithm {
        Algorithm::Sha1Rsa => (
            "http://www.w3.org/2000/09/xmldsig#sha1",
            "http://www.w3.org/2000/09/xmldsig#rsa-sha1",
        ),
        Algorithm::Sha256Rsa => (
            "http://www.w3.org/2001/04/xmlenc#sha256",
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256",
        ),
        _ => return Err("unsupported NTDT digest".into()),
    };
    let signed_info = signed_info_c14n(&filter, dig_uri, sig_uri, &digest_b64, &property_digest);
    let signature = signer(signed_info.as_bytes(), algorithm)?;
    if signature.is_empty() {
        return Err("token returned an empty signature".into());
    }
    // The same canonical SignedInfo bytes are what a verifier hashes.
    verify_signature(signed_info.as_bytes(), &signature, cert_der, algorithm)?;
    let signature_xml = signature_element(&signed_info, &signature, cert_der, signed_at)?;
    let document = insert_signature(xml, &signature_xml)?;
    Ok(STANDARD.encode(document))
}

fn verify_signature(data: &[u8], sig: &[u8], cert_der: &[u8], algorithm: Algorithm) -> Result<()> {
    let key = certificate::rsa_public_key(cert_der)?;
    let signature = Signature::try_from(sig)?;
    match algorithm {
        Algorithm::Sha1Rsa => VerifyingKey::<Sha1>::new(key).verify(data, &signature)?,
        Algorithm::Sha256Rsa => VerifyingKey::<Sha256>::new(key).verify(data, &signature)?,
        Algorithm::Sha384Rsa => VerifyingKey::<sha2::Sha384>::new(key).verify(data, &signature)?,
        Algorithm::Sha512Rsa => VerifyingKey::<sha2::Sha512>::new(key).verify(data, &signature)?,
    }
    Ok(())
}

fn digest_bytes(data: &[u8], algorithm: Algorithm) -> Vec<u8> {
    match algorithm {
        Algorithm::Sha1Rsa => Sha1::digest(data).to_vec(),
        Algorithm::Sha256Rsa => Sha256::digest(data).to_vec(),
        Algorithm::Sha384Rsa => sha2::Sha384::digest(data).to_vec(),
        Algorithm::Sha512Rsa => sha2::Sha512::digest(data).to_vec(),
    }
}

fn canonical_signed(xml: &[u8], xpath: &str) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(xml)?;
    let doc = Document::parse(text).map_err(|e| format!("XML parse error: {e}"))?;
    let local = xpath_local_name(xpath)?;
    if !path_exists(doc.root_element(), xpath)? {
        return Err(format!("Không tìm thấy node qua PATH: {xpath}").into());
    }
    let mut canonical = Vec::new();
    let algo = C14nAlgorithm::new(C14nMode::Exclusive1_0, true).with_prefix_list("#default");
    canonicalize(
        &doc,
        Some(&|node: Node| node.ancestors().any(|a| a.has_tag_name(local.as_str()))),
        &algo,
        &mut canonical,
    )
    .map_err(|e| format!("canonicalization failed: {e}"))?;
    if canonical.is_empty() {
        return Err(format!("Không tìm thấy node qua PATH: {xpath}").into());
    }
    Ok(canonical)
}

fn xpath_local_name(xpath: &str) -> Result<String> {
    if xpath.contains(['[', '@', '*', '(', ')']) || xpath.contains("::") {
        return Err(format!("unsupported XPath: {xpath}").into());
    }
    let local = xpath
        .rsplit('/')
        .find(|part| !part.is_empty())
        .ok_or_else(|| format!("Không tìm thấy node qua PATH: {xpath}"))?;
    if local.contains(':') {
        return Err(format!("unsupported namespaced XPath: {xpath}").into());
    }
    Ok(local.to_owned())
}

fn path_exists(root: Node, xpath: &str) -> Result<bool> {
    let parts: Vec<&str> = xpath.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return Ok(false);
    }
    if !root.has_tag_name(parts[0]) {
        return Ok(false);
    }
    let mut node = root;
    for part in &parts[1..] {
        let Some(child) = node
            .children()
            .find(|child| child.is_element() && child.has_tag_name(*part))
        else {
            return Ok(false);
        };
        node = child;
    }
    Ok(true)
}

fn time_property_c14n(signed_at: &str) -> String {
    format!(
        "<SignatureProperty xmlns=\"http://www.w3.org/2000/09/xmldsig#\" Id=\"TimeSignature\" Target=\"signatureProperties\"><DateTimeStamp>{signed_at}</DateTimeStamp></SignatureProperty>"
    )
}

fn signed_info_c14n(
    filter: &str,
    dig_uri: &str,
    sig_uri: &str,
    digest: &str,
    property_digest: &str,
) -> String {
    let filter = xml_escape(filter);
    format!(
        "<SignedInfo xmlns=\"http://www.w3.org/2000/09/xmldsig#\"><CanonicalizationMethod Algorithm=\"http://www.w3.org/2001/10/xml-exc-c14n#WithComments\"><InclusiveNamespaces xmlns=\"http://www.w3.org/2001/10/xml-exc-c14n#\" PrefixList=\"#default\"></InclusiveNamespaces></CanonicalizationMethod><SignatureMethod Algorithm=\"{sig_uri}\"></SignatureMethod><Reference URI=\"\"><Transforms><Transform Algorithm=\"http://www.w3.org/2002/06/xmldsig-filter2\"><XPath xmlns=\"http://www.w3.org/2002/06/xmldsig-filter2\" Filter=\"intersect\">{filter}</XPath></Transform><Transform Algorithm=\"http://www.w3.org/2001/10/xml-exc-c14n#WithComments\"><InclusiveNamespaces xmlns=\"http://www.w3.org/2001/10/xml-exc-c14n#\" PrefixList=\"#default\"></InclusiveNamespaces></Transform></Transforms><DigestMethod Algorithm=\"{dig_uri}\"></DigestMethod><DigestValue>{digest}</DigestValue></Reference><Reference URI=\"#TimeSignature\"><DigestMethod Algorithm=\"{dig_uri}\"></DigestMethod><DigestValue>{property_digest}</DigestValue></Reference></SignedInfo>"
    )
}

fn signature_element(
    signed_info: &str,
    signature: &[u8],
    cert_der: &[u8],
    signed_at: &str,
) -> Result<String> {
    let key = certificate::rsa_public_key(cert_der)?;
    let modulus = xml_integer(&key.n().to_bytes_be());
    let exponent = xml_integer(&key.e().to_bytes_be());
    let subject = xml_escape(&rfc2253_subject(cert_der)?);
    let cert = STANDARD.encode(cert_der);
    let value = STANDARD.encode(signature);
    Ok(format!(
        "<Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\" Id=\"signatureProperties\">{signed_info}<SignatureValue>{value}</SignatureValue><KeyInfo><KeyValue><RSAKeyValue><Modulus>{modulus}</Modulus><Exponent>{exponent}</Exponent></RSAKeyValue></KeyValue><X509Data><X509SubjectName>{subject}</X509SubjectName><X509Certificate>{cert}</X509Certificate></X509Data></KeyInfo><Object><SignatureProperties><SignatureProperty Id=\"TimeSignature\" Target=\"signatureProperties\"><DateTimeStamp>{signed_at}</DateTimeStamp></SignatureProperty></SignatureProperties></Object></Signature>"
    ))
}

fn xml_integer(bytes: &[u8]) -> String {
    let mut bytes = bytes.to_vec();
    if bytes.first().is_some_and(|b| b & 0x80 != 0) {
        bytes.insert(0, 0);
    }
    STANDARD.encode(bytes)
}

fn rfc2253_subject(cert_der: &[u8]) -> Result<String> {
    let (remaining, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|_| "invalid X.509 certificate DER")?;
    if !remaining.is_empty() {
        return Err("trailing bytes after X.509 certificate".into());
    }
    // Java X500Principal.getName() is RFC 2253, most-significant RDN last
    // in the certificate (x509-parser iterates the opposite way).
    let mut rdns = Vec::new();
    for rdn in cert.subject().iter_rdn() {
        let mut parts = Vec::new();
        for attr in rdn.iter() {
            let name = attr.attr_type().to_id_string();
            let label = match name.as_str() {
                "2.5.4.3" => "CN",
                "2.5.4.6" => "C",
                "2.5.4.7" => "L",
                "2.5.4.8" => "ST",
                "2.5.4.10" => "O",
                "2.5.4.11" => "OU",
                "0.9.2342.19200300.100.1.1" => "UID",
                "2.5.4.45" => "UID",
                other => other,
            };
            let value = attr
                .as_str()
                .map_err(|_| "certificate subject is not a string")?;
            parts.push(format!("{label}={}", rfc2253_escape(value)));
        }
        rdns.push(parts.join("+"));
    }
    rdns.reverse();
    Ok(rdns.join(","))
}

fn rfc2253_escape(value: &str) -> String {
    let mut out = String::new();
    for (i, ch) in value.chars().enumerate() {
        let escape = matches!(ch, ',' | '+' | '"' | '\\' | '<' | '>' | ';')
            || (i == 0 && ch == '#')
            || (i == 0 && ch == ' ')
            || (i + 1 == value.chars().count() && ch == ' ');
        if escape {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn insert_signature(xml: &[u8], signature: &str) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(xml)?;
    let doc = Document::parse(text).map_err(|e| format!("XML parse error: {e}"))?;
    let root = doc.root_element();
    let source = doc.input_text();
    let at = match last_child_end(root) {
        Some(at) => at,
        None => return expand_empty(source, root, signature),
    };
    let mut out = String::new();
    out.push_str(&source[..at]);
    out.push_str(signature);
    out.push_str(&source[at..]);
    Ok(out.into_bytes())
}

fn last_child_end(root: Node) -> Option<usize> {
    let mut end = None;
    for child in root.children() {
        end = Some(child.range().end);
    }
    end
}

fn expand_empty(source: &str, root: Node, signature: &str) -> Result<Vec<u8>> {
    let slice = &source[root.range()];
    let name = slice
        .trim_start_matches('<')
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or("XML document element has no name")?;
    let mut out = String::new();
    if let Some(rel) = slice.rfind("</") {
        let at = root.range().start + rel;
        out.push_str(&source[..at]);
        out.push_str(signature);
        out.push_str(&source[at..]);
    } else if let Some(rel) = slice.rfind("/>") {
        let at = root.range().start + rel;
        out.push_str(&source[..at]);
        out.push('>');
        out.push_str(signature);
        out.push_str("</");
        out.push_str(name);
        out.push('>');
        out.push_str(&source[root.range().end..]);
    } else {
        return Err("XML document element is not a well-formed tag".into());
    }
    Ok(out.into_bytes())
}

/// Local clock as `ddMMyyyyHHmmss`, which is what eSigner writes when it does
/// not call `XMLSigner.setDateSinging`.
pub fn now_stamp() -> String {
    chrono::Local::now().format("%d%m%Y%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_subtree_canonicalization_matches_esigner_1_0_9() {
        let xml = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><DKY_DICHVU><TTIN_DANGKY><MST>1234567890</MST><TEN>Cong ty thu</TEN></TTIN_DANGKY></DKY_DICHVU>";
        let canonical = canonical_signed(xml, "/DKY_DICHVU/TTIN_DANGKY").unwrap();
        assert_eq!(
            canonical,
            b"<TTIN_DANGKY><MST>1234567890</MST><TEN>Cong ty thu</TEN></TTIN_DANGKY>"
        );
        assert_eq!(
            STANDARD.encode(Sha256::digest(canonical)),
            "P4zorHVhHW2jNyYEUlA8vX6M/odjegMpkyQvRGFXhLQ="
        );
    }
}
