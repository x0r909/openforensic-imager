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
    lock_file_waiting(file, libc::LOCK_EX)
}

/// Takes an exclusive lock, waiting briefly for an in-progress reader or writer.
///
/// A short-lived contention is normal (a concurrent verification holds a shared
/// lock), so failing immediately would turn ordinary concurrent use into a
/// spurious `audit.append_after_commit`. The wait is bounded by
/// `LOCK_WAIT_LIMIT` so a genuinely stuck peer still produces a typed error
/// instead of hanging.
/// How long a reader or writer waits for a held audit-log lock.
const LOCK_WAIT_LIMIT: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) fn lock_file_waiting(file: &File, operation: i32) -> Result<(), AuditError> {
    let raw = file.as_raw_fd();
    let deadline = std::time::Instant::now() + LOCK_WAIT_LIMIT;
    loop {
        let result = unsafe { libc::flock(raw, operation | libc::LOCK_NB) };
        if result == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EWOULDBLOCK) {
            return Err(map_io(error));
        }
        if std::time::Instant::now() >= deadline {
            return Err(AuditError::new(
                AuditErrorCode::Locked,
                "audit log is locked by another writer",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Takes a shared lock, waiting for an in-progress writer instead of failing.
///
/// A verification is a read: a concurrent append is transient and will finish,
/// so reporting "locked by another writer" would make ordinary verification
/// flaky rather than protective. The wait is bounded so a permanently stuck
/// writer still produces a typed error.
pub(crate) fn lock_shared_waiting(file: &File) -> Result<(), AuditError> {
    lock_file_waiting(file, libc::LOCK_SH)
}


pub(crate) fn read_last_record(file: &mut File) -> Result<(u64, Option<Sha256Digest>), AuditError> {
    let length = file.metadata().map_err(map_io)?.len();
    if length == 0 {
        return Ok((0, None));
    }

    // A chain is only durably complete when its last record is terminated. A
    // byte-truncated prefix otherwise ends on a complete record with no newline
    // and would be accepted as a shorter valid history.
    file.seek(SeekFrom::End(-1)).map_err(map_io)?;
    let mut terminator = [0_u8; 1];
    file.read_exact(&mut terminator).map_err(map_io)?;
    if terminator[0] != b'\n' {
        return Err(AuditError::new(
            AuditErrorCode::Corrupt,
            "audit log does not end with a complete record",
        ));
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

    // The tail is only a usable head when the whole chain behind it verifies.
    // Without this the next append links to an unverified ancestor, so a forged
    // or truncated history would be re-anchored and later reported as valid.
    // `try_clone` shares the file offset, so rewind before reading from the start.
    file.seek(SeekFrom::Start(0)).map_err(map_io)?;
    let reopen = file.try_clone().map_err(map_io)?;
    let status = verify(reopen)?;
    if let Some(reason) = status.reason() {
        return Err(AuditError::new(
            AuditErrorCode::Corrupt,
            format!("existing audit chain is invalid: {reason}"),
        ));
    }
    let verified_head = status.head_hash();
    if verified_head != Some(&record.record_hash) || status.record_count() != record.sequence {
        return Err(AuditError::new(
            AuditErrorCode::Corrupt,
            "last audit record is not the verified chain head",
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
    let mut terminated = false;

    loop {
        line.clear();
        let bytes_read = reader.read_until(b'\n', &mut line).map_err(map_io)?;
        if bytes_read == 0 {
            break;
        }
        // Track whether this record was newline-terminated before stripping it.
        terminated = false;
        while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
            line.pop();
            terminated = true;
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

    // A chain whose final record was cut short is a truncated history, not a
    // complete one, even when every surviving record is individually valid.
    if reason.is_none() && record_count > 0 && !terminated {
        reason = Some("audit log does not end with a complete record".to_owned());
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
