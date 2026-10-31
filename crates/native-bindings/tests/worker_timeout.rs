use std::ffi::CString;
use std::os::unix::fs::PermissionsExt;
use std::ptr;
use std::time::{Duration, Instant};

use native_bindings::{
    ofi_bytes_free, ofi_job_manager, ofi_job_manager_free, ofi_job_manager_new, ofi_job_snapshot,
    ofi_job_submit, ofi_status,
};
use tempfile::tempdir;

#[test]
fn a_stalled_worker_is_terminated_at_the_deadline() {
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.raw");
    std::fs::write(&evidence, vec![0_u8; 4096]).unwrap();
    let worker = directory.path().join("stalled-worker");
    std::fs::write(&worker, "#!/bin/sh\ncat >/dev/null\nsleep 30\n").unwrap();
    let mut permissions = std::fs::metadata(&worker).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&worker, permissions).unwrap();
    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", worker);
        std::env::set_var("OFI_WORKER_TIMEOUT_SECS", "1");
    }

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let request = serde_json::to_vec(&serde_json::json!({
        "operation": "image_identify",
        "path": evidence,
    }))
    .unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
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
    let submitted: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    unsafe { ofi_bytes_free(response, response_len) };
    let job_id = CString::new(submitted["body"]["job_id"].as_str().unwrap().to_owned()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let snapshot = loop {
        let mut response = ptr::null_mut();
        let mut response_len = 0;
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
            break snapshot;
        }
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(20));
    };

    assert_eq!(snapshot["body"]["status"], "failed");
    assert_eq!(snapshot["body"]["error_code"], "worker.timeout");
    unsafe { ofi_job_manager_free(manager) };
}
