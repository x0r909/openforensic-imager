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
