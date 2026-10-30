mod db;
mod migrations;

use std::fmt;

use forensic_core::{CaseId, EvidenceId, UtcTimestamp};
use serde::{Deserialize, Serialize};

pub use db::CaseStore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateCase {
    pub id: CaseId,
    pub title: String,
    pub examiner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseRecord {
    pub id: CaseId,
    pub title: String,
    pub examiner: String,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub device: u64,
    pub inode: u64,
    pub length: u64,
    pub mtime_nanoseconds: i64,
    pub ctime_nanoseconds: i64,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddEvidence {
    pub id: EvidenceId,
    pub case_id: CaseId,
    pub description: String,
    pub source_path: String,
    pub source: SourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub id: EvidenceId,
    pub case_id: CaseId,
    pub description: String,
    pub source_path: String,
    pub source: SourceSnapshot,
    pub added_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreErrorCode {
    DuplicateCase,
    DuplicateEvidence,
    CaseNotFound,
    InvalidData,
    Database,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError {
    code: StoreErrorCode,
    message: String,
}

impl StoreError {
    pub(crate) fn new(code: StoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> StoreErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for StoreError {}


