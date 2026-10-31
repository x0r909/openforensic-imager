use rusqlite::{Connection, Result};

const SCHEMA_VERSION: i64 = 3;

pub(crate) fn apply(connection: &mut Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version == 1 {
        upgrade_v1_to_v2(connection)?;
        return upgrade_v2_to_v3(connection);
    }
    if version == 2 {
        return upgrade_v2_to_v3(connection);
    }

    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE cases (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            examiner TEXT NOT NULL,
            created_at TEXT NOT NULL,
            audit_path TEXT,
            audit_head_hash TEXT
        );

        CREATE TABLE evidence (
            id TEXT PRIMARY KEY NOT NULL,
            case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
            description TEXT NOT NULL,
            source_path TEXT NOT NULL,
            device INTEGER NOT NULL,
            inode INTEGER NOT NULL,
            length INTEGER NOT NULL,
            mtime_seconds INTEGER NOT NULL,
            mtime_nanoseconds INTEGER NOT NULL,
            ctime_seconds INTEGER NOT NULL,
            ctime_nanoseconds INTEGER NOT NULL,
            read_only INTEGER NOT NULL CHECK (read_only IN (0, 1)),
            added_at TEXT NOT NULL
        );

        CREATE INDEX evidence_by_case ON evidence(case_id);
        ",
    )?;
    // The base batch already includes the version 2 columns, so a fresh database
    // only needs the version 3 upgrade on top.
    transaction.pragma_update(None, "user_version", 2)?;
    transaction.commit()?;
    upgrade_v2_to_v3(connection)
}

/// Adds the per-case audit binding columns to a version 1 database.
///
/// Existing cases keep a null `audit_path`, which means "not yet bound"; the
/// next audited write records it. Nothing is fabricated for prior history.
fn upgrade_v1_to_v2(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        ALTER TABLE cases ADD COLUMN audit_path TEXT;
        ALTER TABLE cases ADD COLUMN audit_head_hash TEXT;
        ",
    )?;
    transaction.pragma_update(None, "user_version", 2)?;
    transaction.commit()
}

/// Adds the durable audit outbox.
///
/// An audit event is inserted in the same transaction as the state change it
/// records, then replayed into the hash-chained log. If the append cannot happen
/// the row stays pending, so a committed case or evidence row always has a
/// recoverable route into the authoritative history instead of being lost.
fn upgrade_v2_to_v3(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE audit_outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            case_id TEXT NOT NULL,
            event_json TEXT NOT NULL,
            appended_sequence INTEGER,
            created_at TEXT NOT NULL
        );

        CREATE INDEX audit_outbox_pending ON audit_outbox(appended_sequence);
        ",
    )?;
    transaction.pragma_update(None, "user_version", 3)?;
    transaction.commit()
}
