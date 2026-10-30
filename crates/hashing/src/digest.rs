use std::fmt::Write;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashScope {
    SourceRead,
    OutputLogical,
    StoredFormat,
    ExtractedFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HashDigest(Vec<u8>);

impl HashDigest {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        let mut output = String::with_capacity(self.0.len() * 2);
        for byte in &self.0 {
            write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
        }
        output
    }
}

