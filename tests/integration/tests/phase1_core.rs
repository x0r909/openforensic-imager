use std::collections::BTreeMap;
use std::io::Read;

use audit_log::{AuditEvent, AuditLog};
use evidence_manager::{AddEvidence, CaseStore, CreateCase, SourceSnapshot};
use forensic_core::{ByteRange, CaseId, EvidenceId};
use forensic_fixtures::{RawLayout, generate_raw_fixture};
use hashing::{HashAlgorithm, StreamingHasher};
use image_reader::{ImageReader, RawReader};
use tempfile::tempdir;
use uuid::Uuid;

fn sha256_file(path: &std::path::Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    let mut buffer = [0_u8; 4096];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    hasher.finish()[&HashAlgorithm::Sha256].to_hex()
}

#[test]
fn phase1_case_evidence_hash_read_and_audit_round_trip() {
    let directory = tempdir().unwrap();
    let fixture_dir = directory.path().join("fixtures");
    let manifest = generate_raw_fixture(
        &fixture_dir,
        42,
        RawLayout::Dense {
            length: 4096,
            block_size: 512,
        },
    )
    .unwrap();
    let source_path = fixture_dir.join(&manifest.segments[0].path);
    let hash_before = sha256_file(&source_path);
    assert_eq!(hash_before, manifest.segments[0].sha256);

    let reader = RawReader::open_read_only(&source_path).unwrap();
    let source = reader.metadata().source;
    let range = ByteRange::new(512, 512, reader.logical_length()).unwrap();
    let mut page = [0_u8; 512];
    reader.read_exact_at(range, &mut page).unwrap();
    assert!(page.iter().any(|byte| *byte != 0));

    let case_id = CaseId::from(Uuid::from_u128(101));
    let evidence_id = EvidenceId::from(Uuid::from_u128(102));
    let db_path = directory.path().join("case.db");
    let audit_path = directory.path().join("audit.jsonl");
    {
        let store = CaseStore::open(&db_path).unwrap();
        let case = store
            .create_case(CreateCase {
                id: case_id,
                title: "Phase 1 Integration".to_owned(),
                examiner: "Examiner".to_owned(),
            })
            .unwrap();
        assert_eq!(case.id, case_id);

        let evidence = store
            .add_evidence(AddEvidence {
                id: evidence_id,
                case_id,
                description: "Dense RAW fixture".to_owned(),
                source_path: source_path.display().to_string(),
                source: SourceSnapshot {
                    device: source.device(),
                    inode: source.inode(),
                    length: source.length(),
                    mtime_nanoseconds: source.mtime_nanoseconds(),
                    ctime_nanoseconds: source.ctime_nanoseconds(),
                    read_only: source.read_only(),
                },
            })
            .unwrap();
        assert_eq!(evidence.id, evidence_id);

        let mut audit = AuditLog::open_append(&audit_path).unwrap();
        audit
            .append(AuditEvent {
                case_id,
                job_id: None,
                actor: "Examiner".to_owned(),
                action: "case.created".to_owned(),
                subject_id: Some(case_id.to_string()),
                details: BTreeMap::new(),
            })
            .unwrap();
        audit
            .append(AuditEvent {
                case_id,
                job_id: None,
                actor: "Examiner".to_owned(),
                action: "evidence.added".to_owned(),
                subject_id: Some(evidence_id.to_string()),
                details: BTreeMap::from([("sha256".to_owned(), hash_before.clone())]),
            })
            .unwrap();
    }

    let hash_after = sha256_file(&source_path);
    assert_eq!(hash_before, hash_after);
    assert_eq!(native_bindings::ofi_abi_version(), 1);

    let store = CaseStore::open(&db_path).unwrap();
    let evidence = store.list_evidence(case_id).unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].id, evidence_id);
    assert_eq!(evidence[0].source.length, 4096);

    let status = AuditLog::verify_chain(&audit_path).unwrap();
    assert!(status.is_valid(), "{:?}", status.reason());
    assert_eq!(status.record_count(), 2);
}
