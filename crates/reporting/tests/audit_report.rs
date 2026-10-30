use std::collections::BTreeMap;

use audit_log::{AuditEvent, AuditLog};
use forensic_core::CaseId;
use reporting::write_audit_report_set;
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn audit_report_states_chain_status_and_limitation() {
    let directory = tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let case_id = CaseId::from(Uuid::from_u128(88));
    {
        let mut log = AuditLog::open_append(&audit).unwrap();
        log.append(AuditEvent {
            case_id,
            job_id: None,
            actor: "examiner".to_owned(),
            action: "case.created".to_owned(),
            subject_id: Some(case_id.to_string()),
            details: BTreeMap::from([("note".to_owned(), "<unsafe>".to_owned())]),
        })
        .unwrap();
    }
    let status = AuditLog::verify_chain(&audit).unwrap();

    let paths = write_audit_report_set(directory.path(), "case", &audit, &status).unwrap();

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&paths.json).unwrap()).unwrap();
    assert_eq!(json["status"]["record_count"], 1);
    assert_eq!(json["status"]["valid"], true);
    assert!(
        json["limitation"]
            .as_str()
            .unwrap()
            .contains("not externally")
    );
    let csv = std::fs::read_to_string(&paths.csv).unwrap();
    assert!(csv.contains("head_hash"));
    let html = std::fs::read_to_string(&paths.html).unwrap();
    assert!(html.contains("not an externally notarized"));
    assert!(!html.contains("<unsafe>"));
}
