use std::ffi::CString;
use std::os::unix::fs::PermissionsExt;
use std::ptr;
use std::time::{Duration, Instant};

use native_bindings::{
    ofi_bytes_free, ofi_error, ofi_job_cancel, ofi_job_manager, ofi_job_manager_free,
    ofi_job_manager_new, ofi_job_snapshot, ofi_job_submit, ofi_status,
};
use tempfile::tempdir;

unsafe fn submit(manager: *mut ofi_job_manager, request: &serde_json::Value) -> String {
    let request = serde_json::to_vec(request).unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_job_submit(
                manager,
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    unsafe { ofi_bytes_free(response, response_len) };
    value["body"]["job_id"].as_str().unwrap().to_owned()
}

fn wait_for_job(manager: *mut ofi_job_manager, job_id: &str) -> serde_json::Value {
    let job_id = CString::new(job_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut response = ptr::null_mut();
        let mut response_len = 0;
        let mut error = ptr::null_mut();
        assert_eq!(
            unsafe {
                ofi_job_snapshot(
                    manager,
                    job_id.as_ptr(),
                    &mut response,
                    &mut response_len,
                    &mut error,
                )
            },
            ofi_status::OFI_STATUS_OK
        );
        let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
        let snapshot: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        unsafe { ofi_bytes_free(response, response_len) };
        if matches!(
            snapshot["body"]["status"].as_str(),
            Some("succeeded" | "partial" | "failed" | "cancelled")
        ) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cancelling_a_job_stops_the_worker_process() {
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.raw");
    std::fs::write(&evidence, vec![0_u8; 4096]).unwrap();
    let slow = directory.path().join("slow-worker");
    std::fs::write(&slow, "#!/bin/sh\ncat >/dev/null\nsleep 30\n").unwrap();
    let mut permissions = std::fs::metadata(&slow).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&slow, permissions).unwrap();
    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", slow);
        std::env::set_var("OFI_WORKER_TIMEOUT_SECS", "60");
    }

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error: *mut ofi_error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let request = serde_json::json!({
        "operation": "image_identify",
        "path": evidence,
    });
    let job_id = unsafe { submit(manager, &request) };
    std::thread::sleep(Duration::from_millis(20));

    let job_id_c = CString::new(job_id.clone()).unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_job_cancel(
                manager,
                job_id_c.as_ptr(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let cancelled: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    unsafe { ofi_bytes_free(response, response_len) };
    assert_eq!(cancelled["body"]["cancelled"], true);

    let snapshot = wait_for_job(manager, &job_id);
    assert_eq!(snapshot["body"]["status"], "cancelled");
    unsafe { ofi_job_manager_free(manager) };
}

/// ACQ-011: a cancelled acquisition must report where its partial output is.
///
/// The worker is killed before it can answer, so the retained staging directory
/// is named from the request instead of being lost.
#[test]
fn cancelled_acquisition_reports_its_retained_staging_directory() {
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.raw");
    std::fs::write(&evidence, vec![0_u8; 4096]).unwrap();
    let destination = directory.path().join("evidence-out");
    std::fs::create_dir(&destination).unwrap();

    let slow = directory.path().join("slow-worker");
    std::fs::write(&slow, "#!/bin/sh\ncat >/dev/null\nsleep 30\n").unwrap();
    let mut permissions = std::fs::metadata(&slow).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&slow, permissions).unwrap();
    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", slow);
        std::env::set_var("OFI_WORKER_TIMEOUT_SECS", "60");
    }

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error: *mut ofi_error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": evidence,
        "destination_dir": destination,
        "base_name": "acquired",
        "format": "raw",
        "options": {
            "block_size": 4096,
            "maximum_segment_size": 1048576,
            "compression": "none",
            "verify_after": false,
        },
    });
    let job_id = unsafe { submit(manager, &request) };
    std::thread::sleep(Duration::from_millis(20));

    let job_id_c = CString::new(job_id.clone()).unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_job_cancel(
                manager,
                job_id_c.as_ptr(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    unsafe { ofi_bytes_free(response, response_len) };
    let _ = bytes;

    let snapshot = wait_for_job(manager, &job_id);
    assert_eq!(snapshot["body"]["status"], "cancelled");
    let staging = snapshot["body"]["result"]["staging_path"]
        .as_str()
        .unwrap_or_default();
    assert!(
        staging.contains(".acquired.partial-"),
        "cancelled acquisition did not name its retained staging directory: {snapshot}"
    );
    assert!(
        staging.starts_with(destination.to_str().unwrap()),
        "staging path must live under the chosen destination: {staging}"
    );
    unsafe { ofi_job_manager_free(manager) };
}
