use evidence_manager::{AddEvidence, CaseStore, CreateCase, SourceSnapshot, StoreErrorCode};
use forensic_core::{CaseId, EvidenceId};
use tempfile::tempdir;
use uuid::Uuid;

fn case_id() -> CaseId {
    CaseId::from(Uuid::from_u128(1))
}

fn evidence_id() -> EvidenceId {
    EvidenceId::from(Uuid::from_u128(2))
}

fn create_case(store: &CaseStore) -> evidence_manager::CaseRecord {
    store
        .create_case(CreateCase {
            id: case_id(),
            title: "Synthetic Case".to_owned(),
            examiner: "Examiner".to_owned(),
        })
        .unwrap()
}

fn add_evidence(store: &CaseStore) -> evidence_manager::EvidenceRecord {
    store
        .add_evidence(AddEvidence {
            id: evidence_id(),
            case_id: case_id(),
            description: "Synthetic RAW image".to_owned(),
            source_path: "/fixtures/raw.img".to_owned(),
            source: SourceSnapshot {
                device: 1,
                inode: 2,
                length: 4096,
                mtime_seconds: 10,
                mtime_nanoseconds: 3,
                ctime_seconds: 20,
                ctime_nanoseconds: 4,
                read_only: true,
            },
        })
        .unwrap()
}

#[test]
fn case_creation_records_a_utc_timestamp() {
    let directory = tempdir().unwrap();
    let store = CaseStore::open(&directory.path().join("case.db")).unwrap();

    let record = create_case(&store);

    assert_eq!(record.id, case_id());
    assert_eq!(record.title, "Synthetic Case");
    assert!(record.created_at.to_rfc3339().ends_with('Z'));
}

#[test]
fn evidence_for_missing_case_is_rejected_by_foreign_key_policy() {
    let directory = tempdir().unwrap();
    let store = CaseStore::open(&directory.path().join("case.db")).unwrap();

    let error = store
        .add_evidence(AddEvidence {
            id: evidence_id(),
            case_id: CaseId::from(Uuid::from_u128(99)),
            description: "missing case".to_owned(),
            source_path: "/fixtures/missing.img".to_owned(),
            source: SourceSnapshot {
                device: 1,
                inode: 2,
                length: 3,
                mtime_seconds: 4,
                mtime_nanoseconds: 4,
                ctime_seconds: 5,
                ctime_nanoseconds: 5,
                read_only: true,
            },
        })
        .unwrap_err();

    assert_eq!(error.code(), StoreErrorCode::CaseNotFound);
}

#[test]
fn duplicate_case_id_is_rejected() {
    let directory = tempdir().unwrap();
    let store = CaseStore::open(&directory.path().join("case.db")).unwrap();
    create_case(&store);

    let error = store
        .create_case(CreateCase {
            id: case_id(),
            title: "Duplicate".to_owned(),
            examiner: "Examiner".to_owned(),
        })
        .unwrap_err();

    assert_eq!(error.code(), StoreErrorCode::DuplicateCase);
}

#[test]
fn duplicate_evidence_id_is_rejected_without_partial_insert() {
    let directory = tempdir().unwrap();
    let store = CaseStore::open(&directory.path().join("case.db")).unwrap();
    create_case(&store);
    add_evidence(&store);

    let error = store
        .add_evidence(AddEvidence {
            id: evidence_id(),
            case_id: case_id(),
            description: "Duplicate evidence".to_owned(),
            source_path: "/fixtures/other.img".to_owned(),
            source: SourceSnapshot {
                device: 9,
                inode: 8,
                length: 7,
                mtime_seconds: 6,
                mtime_nanoseconds: 6,
                ctime_seconds: 7,
                ctime_nanoseconds: 5,
                read_only: true,
            },
        })
        .unwrap_err();

    assert_eq!(error.code(), StoreErrorCode::DuplicateEvidence);
    assert_eq!(store.list_evidence(case_id()).unwrap().len(), 1);
}

#[test]
fn reopening_preserves_evidence_identifier_and_source_snapshot() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("case.db");
    {
        let store = CaseStore::open(&path).unwrap();
        create_case(&store);
        add_evidence(&store);
    }

    let store = CaseStore::open(&path).unwrap();
    let evidence = store.list_evidence(case_id()).unwrap();

    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].id, evidence_id());
    assert_eq!(evidence[0].source.length, 4096);
    assert_eq!(evidence[0].source.mtime_seconds, 10);
    assert_eq!(evidence[0].source.ctime_seconds, 20);
    assert!(evidence[0].source.read_only);
}

#[test]
fn case_audit_binding_is_recorded_and_cannot_be_redirected() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("case.db");
    let store = CaseStore::open(&path).unwrap();
    create_case(&store);
    let log = directory.path().join("audit.jsonl");

    assert_eq!(store.case_audit_path(case_id()).unwrap(), None);

    store.bind_case_audit(case_id(), &log).unwrap();
    assert_eq!(
        store.case_audit_path(case_id()).unwrap().as_deref(),
        Some(log.as_path())
    );

    // Rebinding an existing case to a different log is refused.
    let other = directory.path().join("other.jsonl");
    let error = store.bind_case_audit(case_id(), &other).unwrap_err();
    assert_eq!(error.code(), StoreErrorCode::InvalidData);
    assert_eq!(
        store.case_audit_path(case_id()).unwrap().as_deref(),
        Some(log.as_path()),
        "a refused rebind must not change the recorded path"
    );

    // Re-binding to the same path is idempotent.
    store.bind_case_audit(case_id(), &log).unwrap();
}

#[test]
fn case_audit_head_is_recorded_per_case() {
    let directory = tempdir().unwrap();
    let store = CaseStore::open(&directory.path().join("case.db")).unwrap();
    create_case(&store);

    assert_eq!(store.case_audit_head(case_id()).unwrap(), None);

    let head = "ab".repeat(32);
    store.record_case_audit_head(case_id(), Some(&head)).unwrap();
    assert_eq!(store.case_audit_head(case_id()).unwrap(), Some(head));

    store.record_case_audit_head(case_id(), None).unwrap();
    assert_eq!(store.case_audit_head(case_id()).unwrap(), None);
}

#[test]
fn version_one_database_upgrades_to_the_audit_binding_schema() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("legacy.db");

    // Build a version 1 database exactly as the previous release wrote it.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
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
                    mtime_seconds INTEGER NOT NULL,
                    mtime_nanoseconds INTEGER NOT NULL,
                    ctime_seconds INTEGER NOT NULL,
                    ctime_nanoseconds INTEGER NOT NULL,
                    read_only INTEGER NOT NULL CHECK (read_only IN (0, 1)),
                    added_at TEXT NOT NULL
                );
                CREATE INDEX evidence_by_case ON evidence(case_id);
                PRAGMA user_version = 1;
                ",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO cases (id, title, examiner, created_at) VALUES (?1, 'Legacy', 'Examiner', '2026-01-01T00:00:00Z')",
                [case_id().to_string()],
            )
            .unwrap();
    }

    let store = CaseStore::open(&path).unwrap();
    let evidence = store.list_evidence(case_id()).unwrap();
    assert!(evidence.is_empty(), "existing case must survive the upgrade");
    // A pre-upgrade case is unbound until its next audited write.
    assert_eq!(store.case_audit_path(case_id()).unwrap(), None);
    store
        .bind_case_audit(case_id(), directory.path().join("audit.jsonl").as_path())
        .unwrap();
}
