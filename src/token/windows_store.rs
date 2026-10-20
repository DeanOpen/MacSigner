// Windows certificate store (CurrentUser\My), the source CTSigningHub uses.
// USB tokens register their certificates there through the vendor CSP/minidriver;
// Windows shows the vendor PIN dialog when the private key is used.
#![cfg(windows)]

use crate::crypto::Algorithm;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::Security::Cryptography::*;

/// Window that owns the token's PIN dialog. Signing runs on a worker thread, so
/// without an owner the CSP/KSP dialog can open behind the app.
static OWNER: AtomicIsize = AtomicIsize::new(0);

/// Sets the parent window for token PIN dialogs (process wide).
pub fn set_owner_window(hwnd: isize) {
    OWNER.store(hwnd, Ordering::Relaxed);
    if hwnd != 0 {
        // SAFETY: with hProv = 0, PP_CLIENT_HWND sets the parent window for every
        // CryptoAPI context acquired later in this process; pbData points at an HWND.
        unsafe {
            CryptSetProvParam(0, PP_CLIENT_HWND, &hwnd as *const isize as *const u8, 0);
        }
    }
}

/// SCARD_W_CANCELLED_BY_USER, NTE_USER_CANCELLED, ERROR_CANCELLED.
fn cancelled(code: u32) -> bool {
    matches!(code, 0x8010_006E | 0x8009_0036 | 0x0000_04C7)
}

const CANCELLED: &str = "hộp thoại PIN của token đã bị đóng hoặc hủy";

/// A certificate context with a private key, reference counted by CryptoAPI.
pub struct StoreCert {
    context: *const CERT_CONTEXT,
}

// SAFETY: CERT_CONTEXT is immutable and reference counted by CryptoAPI, which is
// thread safe; this wrapper only reads it and frees its own reference once.
unsafe impl Send for StoreCert {}
unsafe impl Sync for StoreCert {}

impl Drop for StoreCert {
    fn drop(&mut self) {
        // SAFETY: `context` came from CertDuplicateCertificateContext.
        unsafe {
            CertFreeCertificateContext(self.context);
        }
    }
}

impl StoreCert {
    pub fn der(&self) -> Vec<u8> {
        // SAFETY: the context is valid while self lives; the encoded buffer has
        // cbCertEncoded bytes.
        unsafe {
            let c = &*self.context;
            std::slice::from_raw_parts(c.pbCertEncoded, c.cbCertEncoded as usize).to_vec()
        }
    }

    /// Only metadata properties are read. For the user's store this must stay
    /// KEY_PROV_INFO alone: asking for a key context or NCrypt handle can make a
    /// token's CSP/minidriver open the key and show its PIN dialog.
    fn has_private_key(context: *const CERT_CONTEXT, in_memory: bool) -> bool {
        let props: &[u32] = if in_memory {
            // Non-persisted keys from PFXImportCertStore carry a handle instead.
            &[
                CERT_KEY_PROV_INFO_PROP_ID,
                CERT_KEY_CONTEXT_PROP_ID,
                CERT_NCRYPT_KEY_HANDLE_PROP_ID,
            ]
        } else {
            &[CERT_KEY_PROV_INFO_PROP_ID]
        };
        props.iter().copied().any(|prop| {
            let mut len = 0u32;
            // SAFETY: size query with a null buffer.
            unsafe {
                CertGetCertificateContextProperty(context, prop, std::ptr::null_mut(), &mut len)
                    != 0
            }
        })
    }

    /// Signs a precomputed digest with RSASSA-PKCS1-v1_5. The OS/vendor may show
    /// a PIN prompt.
    pub fn sign_digest(&self, digest: &[u8], algorithm: Algorithm) -> Result<Vec<u8>, String> {
        let mut handle: HCRYPTPROV_OR_NCRYPT_KEY_HANDLE = 0;
        let mut spec: CERT_KEY_SPEC = 0;
        let mut free = 0;
        let owner = OWNER.load(Ordering::Relaxed);
        let mut flags = CRYPT_ACQUIRE_PREFER_NCRYPT_KEY_FLAG | CRYPT_ACQUIRE_COMPARE_KEY_FLAG;
        if owner != 0 {
            flags |= CRYPT_ACQUIRE_WINDOW_HANDLE_FLAG;
        }
        // SAFETY: all out-pointers reference locals; the context is valid. With
        // CRYPT_ACQUIRE_WINDOW_HANDLE_FLAG, pvParameters points at an HWND.
        let ok = unsafe {
            CryptAcquireCertificatePrivateKey(
                self.context,
                flags,
                if owner != 0 {
                    &owner as *const isize as *const _
                } else {
                    std::ptr::null()
                },
                &mut handle,
                &mut spec,
                &mut free,
            )
        };
        if ok == 0 {
            // In-memory (non-persisted) keys are attached as an NCrypt handle
            // owned by the certificate context; use it without freeing.
            for prop in [
                CERT_NCRYPT_KEY_HANDLE_PROP_ID,
                CERT_NCRYPT_KEY_HANDLE_TRANSFER_PROP_ID,
            ] {
                let mut key: usize = 0;
                let mut len = std::mem::size_of::<usize>() as u32;
                // SAFETY: the property value is an NCRYPT_KEY_HANDLE (pointer sized).
                let found = unsafe {
                    CertGetCertificateContextProperty(
                        self.context,
                        prop,
                        &mut key as *mut usize as *mut _,
                        &mut len,
                    ) != 0
                };
                if found && key != 0 {
                    return sign_ncrypt(key, digest, algorithm);
                }
            }
            let e = std::io::Error::last_os_error();
            if cancelled(e.raw_os_error().unwrap_or(0) as u32) {
                return Err(CANCELLED.into());
            }
            return Err(format!(
                "không lấy được khóa bí mật (đã cắm token chưa?): {e}"
            ));
        }
        let result = if spec == CERT_NCRYPT_KEY_SPEC {
            set_ncrypt_owner(handle, owner);
            sign_ncrypt(handle, digest, algorithm)
        } else {
            sign_capi(handle, spec, digest, algorithm)
        };
        if free != 0 {
            // SAFETY: handle ownership was transferred to us by the call above.
            unsafe {
                if spec == CERT_NCRYPT_KEY_SPEC {
                    NCryptFreeObject(handle);
                } else {
                    CryptReleaseContext(handle, 0);
                }
            }
        }
        result
    }
}

fn set_ncrypt_owner(key: usize, owner: isize) {
    if owner == 0 {
        return;
    }
    let name: Vec<u16> = "HWND Handle".encode_utf16().chain([0]).collect();
    // SAFETY: NCRYPT_WINDOW_HANDLE_PROPERTY takes a pointer-sized HWND value.
    unsafe {
        NCryptSetProperty(
            key,
            name.as_ptr(),
            &owner as *const isize as *const u8,
            std::mem::size_of::<isize>() as u32,
            0,
        );
    }
}

fn sign_ncrypt(key: usize, digest: &[u8], algorithm: Algorithm) -> Result<Vec<u8>, String> {
    let alg: Vec<u16> = match algorithm {
        Algorithm::Sha1Rsa => "SHA1",
        Algorithm::Sha256Rsa => "SHA256",
        Algorithm::Sha384Rsa => "SHA384",
        Algorithm::Sha512Rsa => "SHA512",
    }
    .encode_utf16()
    .chain([0])
    .collect();
    let padding = BCRYPT_PKCS1_PADDING_INFO {
        pszAlgId: alg.as_ptr(),
    };
    let mut len = 0u32;
    // SAFETY: first call sizes the output, second writes into a buffer of that size.
    unsafe {
        let status = NCryptSignHash(
            key,
            &padding as *const _ as *const _,
            digest.as_ptr(),
            digest.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut len,
            BCRYPT_PAD_PKCS1,
        );
        if status != 0 {
            return Err(format!("NCryptSignHash lỗi 0x{status:08X}"));
        }
        let mut signature = vec![0u8; len as usize];
        let status = NCryptSignHash(
            key,
            &padding as *const _ as *const _,
            digest.as_ptr(),
            digest.len() as u32,
            signature.as_mut_ptr(),
            len,
            &mut len,
            BCRYPT_PAD_PKCS1,
        );
        if status != 0 {
            return Err(match status as u32 {
                s if cancelled(s) => CANCELLED.into(),
                _ => format!("ký bằng token lỗi 0x{status:08X}"),
            });
        }
        signature.truncate(len as usize);
        Ok(signature)
    }
}

fn sign_capi(
    prov: usize,
    spec: u32,
    digest: &[u8],
    algorithm: Algorithm,
) -> Result<Vec<u8>, String> {
    let alg = match algorithm {
        Algorithm::Sha1Rsa => CALG_SHA1,
        Algorithm::Sha256Rsa => CALG_SHA_256,
        Algorithm::Sha384Rsa => CALG_SHA_384,
        Algorithm::Sha512Rsa => CALG_SHA_512,
    };
    let last = || std::io::Error::last_os_error();
    let mut hash = 0usize;
    // SAFETY: CryptoAPI hash object lifecycle; buffers sized per the API contract.
    unsafe {
        if CryptCreateHash(prov, alg, 0, 0, &mut hash) == 0 {
            return Err(format!(
                "CSP của token không hỗ trợ thuật toán băm: {}",
                last()
            ));
        }
        let result = (|| {
            if CryptSetHashParam(hash, HP_HASHVAL, digest.as_ptr(), 0) == 0 {
                return Err(format!("CryptSetHashParam: {}", last()));
            }
            let mut len = 0u32;
            if CryptSignHashW(
                hash,
                spec,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                &mut len,
            ) == 0
            {
                return Err(format!("CryptSignHash: {}", last()));
            }
            let mut signature = vec![0u8; len as usize];
            if CryptSignHashW(
                hash,
                spec,
                std::ptr::null(),
                0,
                signature.as_mut_ptr(),
                &mut len,
            ) == 0
            {
                let e = last();
                if cancelled(e.raw_os_error().unwrap_or(0) as u32) {
                    return Err(CANCELLED.into());
                }
                return Err(format!("ký bằng token lỗi: {e}"));
            }
            signature.truncate(len as usize);
            // CryptoAPI returns little-endian signatures.
            signature.reverse();
            Ok(signature)
        })();
        CryptDestroyHash(hash);
        result
    }
}

fn collect(store: HCERTSTORE, in_memory: bool) -> Vec<StoreCert> {
    let mut out = Vec::new();
    let mut prev: *const CERT_CONTEXT = std::ptr::null();
    loop {
        // SAFETY: enumeration frees `prev` and returns the next context or null.
        let next = unsafe { CertEnumCertificatesInStore(store, prev) };
        if next.is_null() {
            break;
        }
        if StoreCert::has_private_key(next, in_memory) {
            // SAFETY: duplicating keeps a reference beyond the enumeration.
            out.push(StoreCert {
                context: unsafe { CertDuplicateCertificateContext(next) },
            });
        }
        prev = next;
    }
    out
}

/// Certificates with a private key in CurrentUser\My.
pub fn personal() -> Result<Vec<StoreCert>, String> {
    let name: Vec<u16> = "MY".encode_utf16().chain([0]).collect();
    // SAFETY: opens the per-user system store by name; closed below (contexts
    // keep their own references).
    unsafe {
        let store = CertOpenSystemStoreW(0, name.as_ptr());
        if store.is_null() {
            return Err(format!(
                "không mở được kho chứng thư: {}",
                std::io::Error::last_os_error()
            ));
        }
        let certs = collect(store, false);
        CertCloseStore(store, 0);
        Ok(certs)
    }
}

/// Imports a PKCS#12 into a memory-only store with non-persisted keys (tests and
/// one-off use; nothing is written to the user's certificate store).
pub fn import_pfx_ephemeral(pfx: &[u8], password: &str) -> Result<Vec<StoreCert>, String> {
    let password: Vec<u16> = password.encode_utf16().chain([0]).collect();
    let blob = CRYPT_INTEGER_BLOB {
        cbData: pfx.len() as u32,
        pbData: pfx.as_ptr() as *mut u8,
    };
    // SAFETY: blob points at `pfx`, which outlives the call.
    unsafe {
        let store = PFXImportCertStore(
            &blob,
            password.as_ptr(),
            PKCS12_NO_PERSIST_KEY | PKCS12_ALWAYS_CNG_KSP,
        );
        if store.is_null() {
            return Err(format!(
                "không nhập được PFX: {}",
                std::io::Error::last_os_error()
            ));
        }
        let certs = collect(store, true);
        CertCloseStore(store, 0);
        Ok(certs)
    }
}
