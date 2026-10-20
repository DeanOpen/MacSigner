use serde::{Deserialize, Serialize};

// Preserve original spelling and capitalization; these are the wire contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashInfo {
    #[serde(rename = "Ma")]
    pub ma: Option<String>,
    #[serde(rename = "DLieuHash")]
    pub digest: String,
    #[serde(rename = "CKy", default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignHashRequest {
    #[serde(default, rename = "requestId")]
    pub request_id: Option<String>,
    pub serinumber: String,
    #[serde(default)]
    pub signature: Option<String>,
    pub hash: Vec<HashInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response<T> {
    pub status: i32,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T> Response<T> {
    pub fn success(data: T, message: impl Into<String>) -> Self {
        Self {
            status: 200,
            data: Some(data),
            message: Some(message.into()),
        }
    }
    pub fn error(status: i32, message: impl Into<String>) -> Self {
        Self {
            status,
            data: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub serinumber: String,
    #[serde(rename = "subjectDN")]
    pub subject: String,
    #[serde(rename = "issuedDN")]
    pub issuer: String,
    #[serde(rename = "pemCert")]
    pub pem: String,
    pub valid: bool,
    #[serde(rename = "validFrom")]
    pub valid_from: String,
    #[serde(rename = "validTo")]
    pub valid_to: String,
    pub error: Option<String>,
}

/// Request body of the `HubClient/signpdf` API.
#[derive(Debug, Clone, Deserialize)]
pub struct SignPdfRequest {
    #[serde(default, rename = "requestId")]
    pub request_id: Option<String>,
    pub serinumber: String,
    #[serde(default, rename = "dataFile")]
    pub data_file: Option<String>,
    #[serde(default, rename = "filePath")]
    pub file_path: Option<String>,
    #[serde(default)]
    pub sig: Vec<PdfSignatureInfo>,
}

/// Original `PdfSignatureInfoApi`. type: 0 image+text, 1 image only, 2 text only,
/// 3 image as background, 5 default 200x100 box with signer and date.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PdfSignatureInfo {
    #[serde(rename = "type")]
    pub kind: i32,
    pub image: Option<String>,
    pub info: Option<String>,
    #[serde(rename = "sigField")]
    pub sig_field: Option<String>,
    pub page: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
