use std::time::Duration;

use acquisition::{
    AcquisitionCompression, AcquisitionOptions, AcquisitionRequest, AcquisitionStatus,
    OutputFormat, acquire_path_with_cancel,
};
use job_manager::{JobError, JobManager, JobOutcome, JobStatus};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn shared_cancellation_stops_an_acquisition_job() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    std::fs::File::create(&source)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    let manager = JobManager::new(1).unwrap();
    let request = AcquisitionRequest {
        source_path: source,
        destination_dir: destination,
        base_name: "cancelled".to_owned(),
        format: OutputFormat::Raw,
        options: AcquisitionOptions {
            block_size: 64 * 1024,
            maximum_segment_size: 1024 * 1024,
            compression: AcquisitionCompression::None,
            verify_after: true,
        },
    };
    let id = manager
        .submit("acquisition", move |context| {
            let result = acquire_path_with_cancel(request, &context.cancellation_token())
                .map_err(|error| JobError::failed("acquisition.failed", error.to_string()))?;
            match result.status {
                AcquisitionStatus::Cancelled => Err(JobError::cancelled("acquisition cancelled")),
                AcquisitionStatus::Succeeded => Ok(JobOutcome::succeeded(json!({
                    "status": result.status,
                    "manifest": result.manifest,
                }))),
                AcquisitionStatus::Partial => Ok(JobOutcome::partial(
                    json!({"status": result.status, "manifest": result.manifest}),
                    "acquisition completed with read errors",
                )),
                AcquisitionStatus::Failed => Err(JobError::failed(
                    "acquisition.failed",
                    "acquisition did not produce a verified image",
                )),
            }
        })
        .unwrap();

    assert!(manager.cancel(&id));
    let snapshot = manager.wait(&id, Duration::from_secs(5)).unwrap().unwrap();

    assert_eq!(snapshot.status, JobStatus::Cancelled);
    assert!(snapshot.cancellation_requested);
}
