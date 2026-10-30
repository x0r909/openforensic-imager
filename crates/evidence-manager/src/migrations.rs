use rusqlite::{Connection, Result};

const SCHEMA_VERSION: i64 = 1;

pub(crate) fn apply(connection: &mut Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            examiner TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE evidence (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
            description TEXT NOT NULL,
            source_path TEXT NOT NULL,
            device INTEGER NOT NULL,
            inode INTEGER NOT NULL,
            length INTEGER NOT NULL,
            mtime_nanoseconds INTEGER NOT NULL,
            ctime_nanoseconds INTEGER NOT NULL,
            read_only INTEGER NOT NULL CHECK (read_only IN (0, 1)),
            added_at TEXT NOT NULL
        );

        CREATE INDEX evidence_by_case ON evidence(case_id);
        ",
    )?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()
}
