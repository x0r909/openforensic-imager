use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;

use forensic_core::{CaseId, JobId, UtcTimestamp};
use hashing::{HashAlgorithm, StreamingHasher};
use serde::{Deserialize, Serialize};

use crate::{AuditChainStatus, AuditError, AuditErrorCode, AuditEvent, Sha256Digest};

const SCHEMA_MAJOR: u32 = 1;
const SCHEMA_MINOR: u32 = 0;
const MAX_LAST_RECORD_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AuditRecord {
    pub(crate) schema_major: u32,
    pub(crate) schema_minor: u32,
    pub(crate) sequence: u64,
    pub(crate) recorded_at: UtcTimestamp,
    pub(crate) case_id: CaseId,
    pub(crate) job_id: Option<JobId>,
    pub(crate) actor: String,
    pub(crate) action: String,
    pub(crate) subject_id: Option<String>,
    pub(crate) details: BTreeMap<String, String>,
    pub(crate) previous_hash: Option<Sha256Digest>,
    pub(crate) record_hash: Sha256Digest,
}

impl AuditRecord {
    pub(crate) fn new(
        sequence: u64,
        recorded_at: UtcTimestamp,
        previous_hash: Option<Sha256Digest>,
        event: AuditEvent,
    ) -> Self {
        Self {
            schema_major: SCHEMA_MAJOR,
            schema_minor: SCHEMA_MINOR,
            sequence,
            recorded_at,
            case_id: event.case_id,
            job_id: event.job_id,
            actor: event.actor,
            action: event.action,
            subject_id: event.subject_id,
            details: event.details,
            previous_hash,
            record_hash: Sha256Digest::default(),
        }
    }
}

impl Default for Sha256Digest {
    fn default() -> Self {
        Self::from_bytes(&[0_u8; 32])
    }
}

impl Sha256Digest {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        let mut value = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Self(value)
    }
}

#[derive(Serialize)]
struct RecordForHash<'a> {
    schema_major: u32,
    schema_minor: u32,
    sequence: u64,
    recorded_at: &'a UtcTimestamp,
    case_id: CaseId,
    job_id: &'a Option<JobId>,
    actor: &'a str,
    action: &'a str,
    subject_id: &'a Option<String>,
    details: &'a BTreeMap<String, String>,
    previous_hash: &'a Option<Sha256Digest>,
}

pub(crate) fn hash_record(record: &AuditRecord) -> Result<Sha256Digest, AuditError> {
    let canonical = RecordForHash {
        schema_major: record.schema_major,
        schema_minor: record.schema_minor,
        sequence: record.sequence,
        recorded_at: &record.recorded_at,
        case_id: record.case_id,
        job_id: &record.job_id,
        actor: &record.actor,
        action: &record.action,
        subject_id: &record.subject_id,
        details: &record.details,
        previous_hash: &record.previous_hash,
    };
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| AuditError::new(AuditErrorCode::Serialization, error.to_string()))?;
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    hasher.update(&bytes);
    let digest = &hasher.finish()[&HashAlgorithm::Sha256];
    Ok(Sha256Digest::from_bytes(digest.as_bytes()))
}

pub(crate) fn append_record(
    file: &mut File,
    record: &AuditRecord,
    record_hash: &Sha256Digest,
) -> Result<(), AuditError> {
    let mut stored = record.clone();
    stored.record_hash = record_hash.clone();
    let bytes = serde_json::to_vec(&stored)
        .map_err(|error| AuditError::new(AuditErrorCode::Serialization, error.to_string()))?;
    file.write_all(&bytes).map_err(map_io)?;
    file.write_all(b"\n").map_err(map_io)
}

pub(crate) fn lock_exclusive(file: &File) -> Result<(), AuditError> {
    lock_file(file, libc::LOCK_EX)
}

pub(crate) fn lock_shared(file: &File) -> Result<(), AuditError> {
    lock_file(file, libc::LOCK_SH)
}

fn lock_file(file: &File, operation: i32) -> Result<(), AuditError> {
    // The file owns the descriptor for the lifetime of the lock.
    let result = unsafe { libc::flock(file.as_raw_fd(), operation | libc::LOCK_NB) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
        Err(AuditError::new(
            AuditErrorCode::Locked,
            "audit log is locked by another writer",
        ))
    } else {
        Err(map_io(error))
    }
}

pub(crate) fn read_last_record(file: &mut File) -> Result<(u64, Option<Sha256Digest>), AuditError> {
    let length = file.metadata().map_err(map_io)?.len();
    if length == 0 {
        return Ok((0, None));
    }

    let start = length.saturating_sub(MAX_LAST_RECORD_BYTES);
    file.seek(SeekFrom::Start(start)).map_err(map_io)?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).map_err(map_io)?;
    if start > 0 && !tail.contains(&b'\n') {
        return Err(AuditError::new(
            AuditErrorCode::Corrupt,
            "last audit record exceeds the tail-read limit",
        ));
    }
    while tail.last() == Some(&b'\n') {
        tail.pop();
    }
    let line_start = tail
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let line = &tail[line_start..];
    let record: AuditRecord = serde_json::from_slice(line).map_err(|error| {
        AuditError::new(
            AuditErrorCode::Corrupt,
            format!("last audit record is invalid: {error}"),
        )
    })?;
    let expected_hash = hash_record(&record)?;
    if expected_hash != record.record_hash {
        return Err(AuditError::new(
            AuditErrorCode::Corrupt,
            "last audit record hash does not match its contents",
        ));
    }
    Ok((record.sequence, Some(record.record_hash)))
}

pub(crate) fn verify(file: File) -> Result<AuditChainStatus, AuditError> {
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut expected_sequence = 1_u64;
    let mut previous_hash = None;
    let mut record_count = 0_u64;
    let mut reason = None;

    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(map_io)?;
        if bytes_read == 0 {
            break;
        }
        while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() {
            reason = Some("blank audit line".to_owned());
            break;
        }
        let record: AuditRecord = match serde_json::from_slice(&line) {
            Ok(record) => record,
            Err(error) => {
                reason = Some(format!("invalid audit record: {error}"));
                break;
            }
        };
        if record.sequence != expected_sequence {
            reason = Some(format!(
                "expected sequence {expected_sequence}, found {}",
                record.sequence
            ));
            break;
        }
        if record.previous_hash != previous_hash {
            reason = Some(format!(
                "sequence {} has a broken hash link",
                record.sequence
            ));
            break;
        }
        let expected_hash = hash_record(&record)?;
        if expected_hash != record.record_hash {
            reason = Some(format!("sequence {} hash does not match", record.sequence));
            break;
        }
        record_count = record.sequence;
        previous_hash = Some(record.record_hash);
        expected_sequence += 1;
    }

    Ok(AuditChainStatus {
        record_count,
        head_hash: previous_hash,
        reason,
    })
}

fn map_io(error: std::io::Error) -> AuditError {
    AuditError::new(AuditErrorCode::Io, error.to_string())
}
