use std::collections::BTreeMap;
use std::path::Path;

use audit_log::{AuditErrorCode, AuditEvent, AuditLog};
use forensic_core::CaseId;
use tempfile::tempdir;
use uuid::Uuid;

fn case_id() -> CaseId {
    CaseId::from(Uuid::from_u128(7))
}

fn event(action: &str) -> AuditEvent {
    AuditEvent {
        case_id: case_id(),
        job_id: None,
        actor: "examiner".to_owned(),
        action: action.to_owned(),
        subject_id: Some("evidence-1".to_owned()),
        details: BTreeMap::from([("note".to_owned(), "synthetic".to_owned())]),
    }
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn append_is_monotonic_and_links_to_previous_hash() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();

    let first = log.append(event("case.created")).unwrap();
    let second = log.append(event("evidence.added")).unwrap();

    assert_eq!(first.sequence, 1);
    assert_eq!(second.sequence, 2);
    let records = lines(&path);
    assert_eq!(records[0]["sequence"], 1);
    assert_eq!(records[0]["previous_hash"], serde_json::Value::Null);
    assert_eq!(
        records[1]["previous_hash"],
        records[0]["record_hash"].as_str().unwrap()
    );
}

#[test]
fn utf8_and_control_characters_do_not_split_records() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();
    let mut input = event("evidence.added");
    input.actor = "examiner\n\"é\"".to_owned();

    log.append(input).unwrap();

    let records = lines(&path);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["actor"], "examiner\n\"é\"");
}

#[test]
fn record_is_readable_after_append_returns() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();

    log.append(event("case.created")).unwrap();

    assert_eq!(lines(&path).len(), 1);
}

#[test]
fn reopen_continues_sequence_and_chain() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    {
        let mut log = AuditLog::open_append(&path).unwrap();
        log.append(event("case.created")).unwrap();
    }
    {
        let mut log = AuditLog::open_append(&path).unwrap();
        let receipt = log.append(event("evidence.added")).unwrap();
        assert_eq!(receipt.sequence, 2);
    }

    let status = AuditLog::verify_chain(&path).unwrap();
    assert!(status.is_valid(), "{:?}", status.reason());
    assert_eq!(status.record_count(), 2);
}

#[test]
fn second_writer_is_rejected_while_log_is_open() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let _first = AuditLog::open_append(&path).unwrap();

    let error = AuditLog::open_append(&path).unwrap_err();

    assert_eq!(error.code(), AuditErrorCode::Locked);
}

#[test]
fn edited_record_breaks_the_chain() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();
    log.append(event("case.created")).unwrap();
    drop(log);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("synthetic", "changed")).unwrap();

    let status = AuditLog::verify_chain(&path).unwrap();

    assert!(!status.is_valid());
}

#[test]
fn reordered_records_break_the_chain() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();
    log.append(event("case.created")).unwrap();
    log.append(event("evidence.added")).unwrap();
    drop(log);
    let text = std::fs::read_to_string(&path).unwrap();
    let mut records = text.lines().collect::<Vec<_>>();
    records.swap(0, 1);
    std::fs::write(&path, format!("{}\n", records.join("\n"))).unwrap();

    let status = AuditLog::verify_chain(&path).unwrap();

    assert!(!status.is_valid());
}

#[test]
fn deleted_record_breaks_the_chain() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();
    log.append(event("case.created")).unwrap();
    log.append(event("evidence.added")).unwrap();
    drop(log);
    let text = std::fs::read_to_string(&path).unwrap();
    let second = text.lines().nth(1).unwrap();
    std::fs::write(&path, format!("{second}\n")).unwrap();

    let status = AuditLog::verify_chain(&path).unwrap();

    assert!(!status.is_valid());
}

#[test]
fn truncated_record_breaks_the_chain() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("audit.jsonl");
    let mut log = AuditLog::open_append(&path).unwrap();
    log.append(event("case.created")).unwrap();
    drop(log);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, &text[..text.len() - 5]).unwrap();

    let status = AuditLog::verify_chain(&path).unwrap();

    assert!(!status.is_valid());
}
