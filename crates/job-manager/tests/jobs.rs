use std::time::Duration;

use job_manager::{JobError, JobManager, JobOutcome, JobStatus};
use serde_json::json;

#[test]
fn job_runs_in_background_and_returns_its_result() {
    let manager = JobManager::new(2).unwrap();
    let id = manager
        .submit("acquisition", |context| {
            context.update_progress(50, 100)?;
            Ok(JobOutcome::succeeded(json!({"artifact": "disk.raw"})))
        })
        .unwrap();

    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Succeeded);
    assert_eq!(snapshot.completed_bytes, 50);
    assert_eq!(snapshot.total_bytes, 100);
    assert_eq!(snapshot.result.unwrap()["artifact"], "disk.raw");
}

#[test]
fn partial_outcome_is_not_reported_as_success() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("verification", |_| {
            Ok(JobOutcome::partial(
                json!({"unavailable": ["sha1"]}),
                "one stored hash was unavailable",
            ))
        })
        .unwrap();

    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Partial);
    assert_eq!(
        snapshot.message.as_deref(),
        Some("one stored hash was unavailable")
    );
}

#[test]
fn job_error_is_preserved_without_crashing_the_manager() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("analysis", |_| {
            Err(JobError::failed("image.corrupt", "image header is invalid"))
        })
        .unwrap();

    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Failed);
    assert_eq!(snapshot.error_code.as_deref(), Some("image.corrupt"));
    assert_eq!(snapshot.message.as_deref(), Some("image header is invalid"));
}

#[test]
fn panic_is_contained_and_marked_failed() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("analysis", |_| -> Result<JobOutcome, JobError> {
            panic!("synthetic worker panic")
        })
        .unwrap();

    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Failed);
    assert_eq!(snapshot.error_code.as_deref(), Some("panic"));
}

#[test]
fn cancellation_is_visible_to_the_worker_and_reported() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("acquisition", |context| {
            while !context.is_cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(JobError::cancelled("acquisition cancelled"))
        })
        .unwrap();

    assert!(manager.cancel(&id));
    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Cancelled);
    assert_eq!(snapshot.message.as_deref(), Some("acquisition cancelled"));
}

#[test]
fn completed_job_cannot_be_cancelled() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("hashing", |_| Ok(JobOutcome::succeeded(json!({}))))
        .unwrap();
    manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert!(!manager.cancel(&id));
    assert_eq!(
        manager.snapshot(&id).unwrap().unwrap().status,
        JobStatus::Succeeded
    );
}

#[test]
fn progress_cannot_exceed_the_declared_total() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("analysis", |context| {
            let error = context.update_progress(101, 100).unwrap_err();
            Ok(JobOutcome::succeeded(json!({"error": error.message()})))
        })
        .unwrap();

    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Succeeded);
    assert!(
        snapshot.result.unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("progress")
    );
}

#[test]
fn unknown_job_snapshot_is_none() {
    let manager = JobManager::new(1).unwrap();
    let id = forensic_core::JobId::new_v4();

    assert!(manager.snapshot(&id).unwrap().is_none());
}

/// ACQ-011: a cancelled job must keep its retained-artifact payload.
#[test]
fn cancelled_job_keeps_its_retained_result() {
    let manager = JobManager::new(1).unwrap();
    let id = manager
        .submit("acquisition", |_| {
            Err(JobError::cancelled_with_result(
                json!({"retained": true, "staging_path": "/tmp/x"}),
                "cancelled with retained output",
            ))
        })
        .unwrap();
    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();
    assert_eq!(snapshot.status, JobStatus::Cancelled);
    let result = snapshot.result.expect("cancelled job lost its retained payload");
    assert_eq!(result["retained"], true);
    assert_eq!(result["staging_path"], "/tmp/x");
}
