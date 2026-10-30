use std::path::Path;

use forensic_core::{CaseId, EvidenceId, UtcTimestamp};
use rusqlite::{Connection, Row, params};

use crate::migrations;
use crate::{
    AddEvidence, CaseRecord, CreateCase, EvidenceRecord, SourceSnapshot, StoreError,
    StoreErrorCode,
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
        self.connection
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
                    mtime_nanoseconds, ctime_nanoseconds, read_only, added_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    input.id.to_string(),
                    input.case_id.to_string(),
                    input.description,
                    input.source_path,
                    device,
                    inode,
                    length,
                    input.source.mtime_nanoseconds,
                    input.source.ctime_nanoseconds,
                    input.source.read_only,
                    added_at.to_rfc3339()
                ],
            )
            .map_err(map_evidence_insert_error)?;
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

    pub fn list_evidence(&self, case_id: CaseId) -> Result<Vec<EvidenceRecord>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "
                SELECT id, case_id, description, source_path, device, inode, length,
                       mtime_nanoseconds, ctime_nanoseconds, read_only, added_at
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
    let added_at: String = row.get(10)?;
    Ok(EvidenceRecord {
        id: id
            .parse::<EvidenceId>()
            .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            ))?,
        case_id: case_id
            .parse::<CaseId>()
            .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            ))?,
        description: row.get(2)?,
        source_path: row.get(3)?,
        source: SourceSnapshot {
            device: i64_to_u64(row.get(4)?, 4)?,
            inode: i64_to_u64(row.get(5)?, 5)?,
            length: i64_to_u64(row.get(6)?, 6)?,
            mtime_nanoseconds: row.get(7)?,
            ctime_nanoseconds: row.get(8)?,
            read_only: row.get(9)?,
        },
        added_at: added_at
            .parse::<UtcTimestamp>()
            .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            ))?,
    })
}

fn map_insert_error(error: rusqlite::Error, duplicate_code: StoreErrorCode) -> StoreError {
    if is_constraint(&error) {
        StoreError::new(duplicate_code, error.to_string())
    } else {
        map_db_error(error)
    }
}

fn map_evidence_insert_error(error: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(sqlite_error, _) = &error {
        if sqlite_error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY {
            return StoreError::new(StoreErrorCode::CaseNotFound, error.to_string());
        }
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

