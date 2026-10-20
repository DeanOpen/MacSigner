use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Algorithm {
    Sha1Rsa,
    Sha256Rsa,
    Sha384Rsa,
    Sha512Rsa,
}

impl Algorithm {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "sha1withrsa" | "sha1" => Ok(Self::Sha1Rsa),
            "sha256withrsa" | "sha256" => Ok(Self::Sha256Rsa),
            "sha384withrsa" | "sha384" => Ok(Self::Sha384Rsa),
            "sha512withrsa" | "sha512" => Ok(Self::Sha512Rsa),
            _ => Err("unsupported legacy signing algorithm".into()),
        }
    }
}
