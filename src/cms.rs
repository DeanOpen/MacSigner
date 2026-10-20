// Minimal DER/CMS support for detached CAdES-BES signatures (RFC 5652, ETSI EN
// 319 122): enough to build what PDF signing needs and to read it back.
use sha2::{Digest, Sha256};

pub const OID_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x01];
pub const OID_SIGNED_DATA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x07, 0x02];
pub const OID_CONTENT_TYPE: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x03];
pub const OID_MESSAGE_DIGEST: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x04];
pub const OID_SIGNING_TIME: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x05];
pub const OID_SIGNING_CERT_V2: &[u8] = &[
    0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x10, 0x02, 0x2f,
];
pub const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
pub const OID_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];

pub fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let first = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        out.push(0x80 | (bytes.len() - first) as u8);
        out.extend_from_slice(&bytes[first..]);
    }
    out.extend_from_slice(content);
    out
}

fn seq(parts: &[&[u8]]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

fn oid(value: &[u8]) -> Vec<u8> {
    tlv(0x06, value)
}

fn algorithm(value: &[u8]) -> Vec<u8> {
    seq(&[&oid(value), &[0x05, 0x00]])
}

fn attribute(kind: &[u8], value: &[u8]) -> Vec<u8> {
    seq(&[&oid(kind), &tlv(0x31, value)])
}

/// Certificate fields a SignerInfo needs, taken from the leaf certificate.
struct Leaf {
    issuer: Vec<u8>,
    serial: Vec<u8>,
}

fn leaf(der: &[u8]) -> Result<Leaf, String> {
    let (_, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|e| format!("chứng thư không hợp lệ: {e}"))?;
    Ok(Leaf {
        issuer: cert.tbs_certificate.issuer.as_raw().to_vec(),
        serial: tlv(0x02, cert.tbs_certificate.raw_serial()),
    })
}

/// DER of the signed attributes as a SET (the bytes that are actually signed).
pub fn signed_attributes(content_digest: &[u8], leaf_cert: &[u8]) -> Vec<u8> {
    let cert_hash = Sha256::digest(leaf_cert);
    // ESSCertIDv2 with the default SHA-256 algorithm omitted.
    let signing_cert = seq(&[&seq(&[&seq(&[&tlv(0x04, &cert_hash)])])]);
    let mut attributes = [
        attribute(OID_CONTENT_TYPE, &oid(OID_DATA)),
        attribute(OID_MESSAGE_DIGEST, &tlv(0x04, content_digest)),
        attribute(OID_SIGNING_CERT_V2, &signing_cert),
    ];
    // DER SET OF: elements sorted by their encodings.
    attributes.sort();
    tlv(0x31, &attributes.concat())
}

/// Builds a detached CMS SignedData. `sign` receives the DER signed attributes and
/// returns an RSASSA-PKCS1-v1_5 SHA-256 signature over them.
pub fn sign_detached(
    content_digest: &[u8],
    chain_leaf_first: &[Vec<u8>],
    sign: impl FnOnce(&[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let leaf_der = chain_leaf_first.first().ok_or("thiếu chứng thư ký")?;
    let leaf = leaf(leaf_der)?;
    let attrs = signed_attributes(content_digest, leaf_der);
    let signature = sign(&attrs)?;
    let mut implicit_attrs = attrs.clone();
    implicit_attrs[0] = 0xa0;
    let signer_info = seq(&[
        &tlv(0x02, &[1]),
        &seq(&[&leaf.issuer, &leaf.serial]),
        &algorithm(OID_SHA256),
        &implicit_attrs,
        &algorithm(OID_RSA),
        &tlv(0x04, &signature),
    ]);
    let signed_data = seq(&[
        &tlv(0x02, &[1]),
        &tlv(0x31, &algorithm(OID_SHA256)),
        &seq(&[&oid(OID_DATA)]),
        &tlv(0xa0, &chain_leaf_first.concat()),
        &tlv(0x31, &signer_info),
    ]);
    Ok(seq(&[&oid(OID_SIGNED_DATA), &tlv(0xa0, &signed_data)]))
}

// ---- Reading -----------------------------------------------------------------

#[derive(Clone, Copy)]
struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
    raw: &'a [u8],
}

fn read(input: &[u8]) -> Result<(Tlv<'_>, &[u8]), String> {
    let bad = || "CMS DER không hợp lệ".to_string();
    let tag = *input.first().ok_or_else(bad)?;
    let first = *input.get(1).ok_or_else(bad)? as usize;
    let (len, header) = if first < 0x80 {
        (first, 2)
    } else {
        let n = first & 0x7f;
        if n == 0 || n > 4 {
            return Err(bad());
        }
        let bytes = input.get(2..2 + n).ok_or_else(bad)?;
        (
            bytes.iter().fold(0usize, |a, b| (a << 8) | *b as usize),
            2 + n,
        )
    };
    let end = header.checked_add(len).ok_or_else(bad)?;
    let raw = input.get(..end).ok_or_else(bad)?;
    Ok((
        Tlv {
            tag,
            content: &raw[header..],
            raw,
        },
        &input[end..],
    ))
}

fn children(content: &[u8]) -> Result<Vec<Tlv<'_>>, String> {
    let mut rest = content;
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (item, next) = read(rest)?;
        out.push(item);
        rest = next;
    }
    Ok(out)
}

/// Length of the leading DER element; PDF /Contents is zero padded after it.
pub fn der_length(input: &[u8]) -> Result<usize, String> {
    Ok(read(input)?.0.raw.len())
}

pub struct Parsed {
    pub certificates: Vec<Vec<u8>>,
    pub signer_serial: Vec<u8>,
    /// Signed attributes re-tagged as SET, i.e. the bytes the signature covers.
    pub signed_attributes: Vec<u8>,
    pub message_digest: Vec<u8>,
    pub digest_oid: Vec<u8>,
    pub signature: Vec<u8>,
}

pub fn parse(der: &[u8]) -> Result<Parsed, String> {
    let (info, _) = read(der)?;
    let info = children(info.content)?;
    if info.len() < 2 || info[0].content != OID_SIGNED_DATA {
        return Err("không phải CMS SignedData".into());
    }
    let signed = children(
        children(info[1].content)?
            .first()
            .ok_or("thiếu SignedData")?
            .content,
    )?;
    let mut certificates = Vec::new();
    let mut signer_infos = None;
    for item in &signed[1..] {
        match item.tag {
            0xa0 => {
                certificates = children(item.content)?
                    .iter()
                    .map(|c| c.raw.to_vec())
                    .collect();
            }
            0x31 => signer_infos = Some(*item),
            _ => {}
        }
    }
    let signer = children(signer_infos.ok_or("thiếu SignerInfo")?.content)?;
    let signer = children(signer.first().ok_or("thiếu SignerInfo")?.content)?;
    // version, sid, digestAlgorithm, [0] signedAttrs, signatureAlgorithm, signature
    if signer.len() < 6 || signer[3].tag != 0xa0 {
        return Err("SignerInfo không có signed attributes".into());
    }
    let sid = children(signer[1].content)?;
    let signer_serial = sid.get(1).map(|s| s.content.to_vec()).unwrap_or_default();
    let digest_oid = children(signer[2].content)?
        .first()
        .map(|o| o.content.to_vec())
        .unwrap_or_default();
    let mut signed_attributes = signer[3].raw.to_vec();
    signed_attributes[0] = 0x31;
    let mut message_digest = Vec::new();
    for attr in children(signer[3].content)? {
        let parts = children(attr.content)?;
        if parts.len() == 2
            && parts[0].content == OID_MESSAGE_DIGEST
            && let Some(value) = children(parts[1].content)?.first()
        {
            message_digest = value.content.to_vec();
        }
    }
    Ok(Parsed {
        certificates,
        signer_serial,
        signed_attributes,
        message_digest,
        digest_oid,
        signature: signer[5].content.to_vec(),
    })
}
