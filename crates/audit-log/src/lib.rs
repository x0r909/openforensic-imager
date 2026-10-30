mod chain;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Seek, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use forensic_core::{CaseId, JobId, UtcTimestamp};
use serde::{Deserialize, Serialize};

use chain::{append_record, hash_record, lock_exclusive, lock_shared, read_last_record, verify};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub case_id: CaseId,
    pub job_id: Option<JobId>,
    pub actor: String,
    pub action: String,
    pub subject_id: Option<String>,
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn to_hex(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditReceipt {
    pub sequence: u64,
    pub record_hash: Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditChainStatus {
    record_count: u64,
    head_hash: Option<Sha256Digest>,
    reason: Option<String>,
}

impl AuditChainStatus {
    pub fn is_valid(&self) -> bool {
        self.reason.is_none()
    }

    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    pub fn head_hash(&self) -> Option<&Sha256Digest> {
        self.head_hash.as_ref()
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditErrorCode {
    Io,
    Locked,
    Corrupt,
    Serialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditError {
    code: AuditErrorCode,
    message: String,
}

impl AuditError {
    pub(crate) fn new(code: AuditErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> AuditErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AuditError {}

#[derive(Debug)]
pub struct AuditLog {
    file: File,
    last_sequence: u64,
    last_hash: Option<Sha256Digest>,
    poisoned: bool,
}

impl AuditLog {
    pub fn open_append(path: &Path) -> Result<Self, AuditError> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .map_err(map_io)?;
        lock_exclusive(&file)?;
        let (last_sequence, last_hash) = read_last_record(&mut file)?;
        file.seek(std::io::SeekFrom::End(0)).map_err(map_io)?;
        Ok(Self {
            file,
            last_sequence,
            last_hash,
            poisoned: false,
        })
    }

    pub fn append(&mut self, event: AuditEvent) -> Result<AuditReceipt, AuditError> {
        if self.poisoned {
            return Err(AuditError::new(
                AuditErrorCode::Corrupt,
                "audit log is poisoned after an uncertain write",
            ));
        }
        let sequence = self
            .last_sequence
            .checked_add(1)
            .ok_or_else(|| AuditError::new(AuditErrorCode::Corrupt, "audit sequence overflow"))?;
        let record =
            chain::AuditRecord::new(sequence, UtcTimestamp::now(), self.last_hash.clone(), event);
        let record_hash = hash_record(&record)?;
        if let Err(error) = append_record(&mut self.file, &record, &record_hash) {
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = self.file.flush() {
            self.poisoned = true;
            return Err(map_io(error));
        }
        if let Err(error) = self.file.sync_data() {
            self.poisoned = true;
            return Err(map_io(error));
        }
        self.last_sequence = sequence;
        self.last_hash = Some(record_hash.clone());
        Ok(AuditReceipt {
            sequence,
            record_hash,
        })
    }

    pub fn verify_chain(path: &Path) -> Result<AuditChainStatus, AuditError> {
        let file = File::open(path).map_err(map_io)?;
        lock_shared(&file)?;
        verify(file)
    }
}

fn map_io(error: std::io::Error) -> AuditError {
    AuditError::new(AuditErrorCode::Io, error.to_string())
}
