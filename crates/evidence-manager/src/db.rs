use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forensic_core::{CaseId, EvidenceId, UtcTimestamp};
use rusqlite::{Connection, Row, params};

use crate::migrations;
use crate::{
    AddEvidence, CaseRecord, CreateCase, EvidenceRecord, SourceSnapshot, StoreError, StoreErrorCode,
};

pub struct CaseStore {
    connection: Connection,
}

impl CaseStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut connection = Connection::open(path).map_err(map_db_error)?;
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = FULL;
                ",
            )
            .map_err(map_db_error)?;
        migrations::apply(&mut connection).map_err(map_db_error)?;
        Ok(Self { connection })
    }

    pub fn create_case(&self, input: CreateCase) -> Result<CaseRecord, StoreError> {
        let created_at = UtcTimestamp::now();
        // The case row and its audit intent commit together, so a later audit
        // append failure leaves a recoverable outbox entry, never silent state.
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_db_error)?;
        transaction
            .execute(
                "INSERT INTO cases (id, title, examiner, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    input.id.to_string(),
                    input.title,
                    input.examiner,
                    created_at.to_rfc3339()
                ],
            )
            .map_err(|error| map_insert_error(error, StoreErrorCode::DuplicateCase))?;
        transaction
            .execute(
                "INSERT INTO audit_outbox (case_id, event_json, created_at) VALUES (?1, ?2, ?3)",
                params![
                    input.id.to_string(),
                    outbox_event_json(
                        input.id,
                        "case.created",
                        &input.id.to_string(),
                        BTreeMap::from([("title".to_owned(), input.title.clone())]),
                    ),
                    created_at.to_rfc3339()
                ],
            )
            .map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;

        Ok(CaseRecord {
            id: input.id,
            title: input.title,
            examiner: input.examiner,
            created_at,
        })
    }

    pub fn add_evidence(&self, input: AddEvidence) -> Result<EvidenceRecord, StoreError> {
        let added_at = UtcTimestamp::now();
        let device = u64_to_i64(input.source.device)?;
        let inode = u64_to_i64(input.source.inode)?;
        let length = u64_to_i64(input.source.length)?;
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(map_db_error)?;
        transaction
            .execute(
                "
                INSERT INTO evidence (
                    id, case_id, description, source_path, device, inode, length,
                    mtime_seconds, mtime_nanoseconds, ctime_seconds, ctime_nanoseconds,
                    read_only, added_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ",
                params![
                    input.id.to_string(),
                    input.case_id.to_string(),
                    input.description,
                    input.source_path,
                    device,
                    inode,
                    length,
                    input.source.mtime_seconds,
                    input.source.mtime_nanoseconds,
                    input.source.ctime_seconds,
                    input.source.ctime_nanoseconds,
                    input.source.read_only,
                    added_at.to_rfc3339()
                ],
            )
            .map_err(map_evidence_insert_error)?;
        transaction
            .execute(
                "INSERT INTO audit_outbox (case_id, event_json, created_at) VALUES (?1, ?2, ?3)",
                params![
                    input.case_id.to_string(),
                    outbox_event_json(
                        input.case_id,
                        "evidence.added",
                        &input.id.to_string(),
                        BTreeMap::from([
                            ("source_path".to_owned(), input.source_path.clone()),
                            ("length".to_owned(), input.source.length.to_string()),
                        ]),
                    ),
                    added_at.to_rfc3339()
                ],
            )
            .map_err(map_db_error)?;
        transaction.commit().map_err(map_db_error)?;

        Ok(EvidenceRecord {
            id: input.id,
            case_id: input.case_id,
            description: input.description,
            source_path: input.source_path,
            source: input.source,
            added_at,
        })
    }

    /// Returns the pending audit events for a case, oldest first, as
    /// `(outbox_id, event_json)` pairs.
    pub fn pending_audit_events(&self, case_id: CaseId) -> Result<Vec<(i64, String)>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, event_json FROM audit_outbox \
                 WHERE case_id = ?1 AND appended_sequence IS NULL ORDER BY id",
            )
            .map_err(map_db_error)?;
        let rows = statement
            .query_map(params![case_id.to_string()], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(map_db_error)?;
        rows.map(|row| row.map_err(map_db_error))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Marks an outbox entry as appended at the given chain sequence.
    pub fn mark_audit_event_appended(
        &self,
        outbox_id: i64,
        sequence: u64,
    ) -> Result<(), StoreError> {
        let sequence = i64::try_from(sequence).map_err(|_| {
            StoreError::new(
                StoreErrorCode::InvalidData,
                "audit sequence exceeds signed 64-bit storage",
            )
        })?;
        self.connection
            .execute(
                "UPDATE audit_outbox SET appended_sequence = ?2 WHERE id = ?1",
                params![outbox_id, sequence],
            )
            .map_err(map_db_error)?;
        Ok(())
    }

    /// Returns how many audit events are still awaiting their append.
    pub fn pending_audit_count(&self) -> Result<u64, StoreError> {
        let count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM audit_outbox WHERE appended_sequence IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(map_db_error)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Returns a case that still has queued audit events, if any.
    pub fn pending_case_id(&self) -> Result<Option<CaseId>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT case_id FROM audit_outbox \
                 WHERE appended_sequence IS NULL ORDER BY id LIMIT 1",
            )
            .map_err(map_db_error)?;
        let mut rows = statement.query([]).map_err(map_db_error)?;
        match rows.next().map_err(map_db_error)? {
            Some(row) => {
                let case_id: String = row.get(0).map_err(map_db_error)?;
                case_id.parse::<CaseId>().map(Some).map_err(|error| {
                    StoreError::new(
                        StoreErrorCode::InvalidData,
                        format!("queued audit event has an invalid case id: {error}"),
                    )
                })
            }
            None => Ok(None),
        }
    }

    /// Returns the audit log path recorded for a case, if one has been bound.
    pub fn case_audit_path(&self, case_id: CaseId) -> Result<Option<PathBuf>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT audit_path FROM cases WHERE id = ?1")
            .map_err(map_db_error)?;
        let mut rows = statement
            .query(params![case_id.to_string()])
            .map_err(map_db_error)?;
        let row = rows.next().map_err(map_db_error)?;
        match row {
            Some(row) => {
                let path: Option<String> = row.get(0).map_err(map_db_error)?;
                Ok(path.map(PathBuf::from))
            }
            None => Err(StoreError::new(
                StoreErrorCode::CaseNotFound,
                format!("case {case_id} does not exist"),
            )),
        }
    }

    /// Records the audit log a case writes to, and the chain head after a write.
    ///
    /// A bound case may not be rebound to a different log, so a caller can never
    /// redirect an existing case's history.
    pub fn bind_case_audit(&self, case_id: CaseId, path: &Path) -> Result<(), StoreError> {
        let existing = self.case_audit_path(case_id)?;
        if let Some(bound) = existing
            && bound != path
        {
            return Err(StoreError::new(
                StoreErrorCode::InvalidData,
                format!(
                    "case {case_id} is bound to audit log {} and may not be rebound to {}",
                    bound.display(),
                    path.display()
                ),
            ));
        }
        self.connection
            .execute(
                "UPDATE cases SET audit_path = ?2 WHERE id = ?1",
                params![case_id.to_string(), path.display().to_string()],
            )
            .map_err(map_db_error)?;
        Ok(())
    }

    /// Records the latest verified chain head for a case.
    pub fn record_case_audit_head(
        &self,
        case_id: CaseId,
        head_hash: Option<&str>,
    ) -> Result<(), StoreError> {
        self.connection
            .execute(
                "UPDATE cases SET audit_head_hash = ?2 WHERE id = ?1",
                params![case_id.to_string(), head_hash],
            )
            .map_err(map_db_error)?;
        Ok(())
    }

    /// Returns the last recorded chain head for a case.
    pub fn case_audit_head(&self, case_id: CaseId) -> Result<Option<String>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT audit_head_hash FROM cases WHERE id = ?1")
            .map_err(map_db_error)?;
        let mut rows = statement
            .query(params![case_id.to_string()])
            .map_err(map_db_error)?;
        let row = rows.next().map_err(map_db_error)?;
        match row {
            Some(row) => row.get(0).map_err(map_db_error),
            None => Err(StoreError::new(
                StoreErrorCode::CaseNotFound,
                format!("case {case_id} does not exist"),
            )),
        }
    }

    pub fn list_evidence(&self, case_id: CaseId) -> Result<Vec<EvidenceRecord>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT id, case_id, description, source_path, device, inode, length,
                       mtime_seconds, mtime_nanoseconds, ctime_seconds, ctime_nanoseconds,
                       read_only, added_at
                FROM evidence
                WHERE case_id = ?1
                ORDER BY rowid
                ",
            )
            .map_err(map_db_error)?;
        let rows = statement
            .query_map(params![case_id.to_string()], map_evidence_row)
            .map_err(map_db_error)?;

        rows.map(|row| row.map_err(map_db_error))
            .collect::<Result<Vec<_>, _>>()
    }
}

fn map_evidence_row(row: &Row<'_>) -> rusqlite::Result<EvidenceRecord> {
    let id: String = row.get(0)?;
    let case_id: String = row.get(1)?;
    let added_at: String = row.get(12)?;
    Ok(EvidenceRecord {
        id: id.parse::<EvidenceId>().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        case_id: case_id.parse::<CaseId>().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        description: row.get(2)?,
        source_path: row.get(3)?,
        source: SourceSnapshot {
            device: i64_to_u64(row.get(4)?, 4)?,
            inode: i64_to_u64(row.get(5)?, 5)?,
            length: i64_to_u64(row.get(6)?, 6)?,
            mtime_seconds: row.get(7)?,
            mtime_nanoseconds: row.get(8)?,
            ctime_seconds: row.get(9)?,
            ctime_nanoseconds: row.get(10)?,
            read_only: row.get(11)?,
        },
        added_at: added_at.parse::<UtcTimestamp>().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

fn map_insert_error(error: rusqlite::Error, duplicate_code: StoreErrorCode) -> StoreError {    if is_constraint(&error) {
        StoreError::new(duplicate_code, error.to_string())
    } else {
        map_db_error(error)
    }
}

fn map_evidence_insert_error(error: rusqlite::Error) -> StoreError {
    if matches!(
        &error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if sqlite_error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    ) {
        return StoreError::new(StoreErrorCode::CaseNotFound, error.to_string());
    }
    map_insert_error(error, StoreErrorCode::DuplicateEvidence)
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if sqlite_error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
                || sqlite_error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

fn map_db_error(error: rusqlite::Error) -> StoreError {
    match error {
        rusqlite::Error::InvalidQuery => {
            StoreError::new(StoreErrorCode::InvalidData, error.to_string())
        }
        _ => StoreError::new(StoreErrorCode::Database, error.to_string()),
    }
}

fn u64_to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| {
        StoreError::new(
            StoreErrorCode::InvalidData,
            "SQLite source value exceeds signed 64-bit storage",
        )
    })
}

fn i64_to_u64(value: i64, column: usize) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

/// Serialises an audit event for the outbox.
///
/// The shape matches `audit_log::AuditEvent` so the replay path can deserialize
/// it without the store depending on the audit crate.
fn outbox_event_json(
    case_id: CaseId,
    action: &str,
    subject_id: &str,
    details: BTreeMap<String, String>,
) -> String {
    serde_json::json!({
        "case_id": case_id.to_string(),
        "job_id": serde_json::Value::Null,
        "actor": "examiner",
        "action": action,
        "subject_id": subject_id,
        "details": details,
    })
    .to_string()
}
