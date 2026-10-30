use std::ffi::{CStr, CString};
use std::io::Write;
use std::process::{Command, Stdio};
use std::ptr;
use std::time::{Duration, Instant};

use native_bindings::{
    ofi_abi_version, ofi_abi_version_minor, ofi_bytes_free, ofi_case_create, ofi_error,
    ofi_error_code, ofi_error_free, ofi_evidence_add, ofi_job_cancel, ofi_job_manager,
    ofi_job_manager_free, ofi_job_manager_new, ofi_job_snapshot, ofi_job_submit, ofi_status,
};
use tempfile::tempdir;

unsafe fn error_code(error: *mut ofi_error) -> String {
    assert!(!error.is_null());
    let code = unsafe { CStr::from_ptr(ofi_error_code(error)) };
    let value = code.to_string_lossy().into_owned();
    unsafe { ofi_error_free(error) };
    value
}

#[test]
fn abi_version_is_one() {
    assert_eq!(ofi_abi_version(), 1);
    assert_eq!(ofi_abi_version_minor(), 1);
}

#[test]
fn null_request_is_rejected_without_dereference() {
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status =
        unsafe { ofi_case_create(ptr::null(), 0, &mut response, &mut response_len, &mut error) };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_ARGUMENT);
    assert!(response.is_null());
    assert_eq!(unsafe { error_code(error) }, "invalid_argument");
}

#[test]
fn invalid_utf8_is_rejected() {
    let bytes = [0xff_u8];
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            bytes.as_ptr(),
            bytes.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_JSON);
    assert!(response.is_null());
    assert_eq!(unsafe { error_code(error) }, "invalid_json");
}

#[test]
fn oversized_request_length_is_rejected_before_slice_creation() {
    let bytes = b"{}";
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            bytes.as_ptr(),
            isize::MAX as usize,
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_ARGUMENT);
    assert_eq!(unsafe { error_code(error) }, "invalid_argument");
}

#[test]
fn case_and_evidence_requests_return_owned_json() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("abi.db");
    let case_id = "00000000-0000-0000-0000-000000000001";
    let evidence_id = "00000000-0000-0000-0000-000000000002";
    let request = format!(
        r#"{{"db_path":"{}","id":"{case_id}","title":"ABI Case","examiner":"Examiner"}}"#,
        path.display()
    );
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            request.as_ptr(),
            request.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    assert!(error.is_null());
    let response_bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let response_json: serde_json::Value = serde_json::from_slice(response_bytes).unwrap();
    assert_eq!(response_json["body"]["id"], case_id);
    unsafe { ofi_bytes_free(response, response_len) };

    let request = format!(
        r#"{{"db_path":"{}","id":"{evidence_id}","case_id":"{case_id}","description":"disk","source_path":"/tmp/disk.raw","source":{{"device":1,"inode":2,"length":4096,"mtime_seconds":10,"mtime_nanoseconds":3,"ctime_seconds":20,"ctime_nanoseconds":4,"read_only":true}}}}"#,
        path.display()
    );
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let status = unsafe {
        ofi_evidence_add(
            request.as_ptr(),
            request.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    assert!(error.is_null());
    let response_bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let response_json: serde_json::Value = serde_json::from_slice(response_bytes).unwrap();
    assert_eq!(response_json["body"]["id"], evidence_id);
    unsafe { ofi_bytes_free(response, response_len) };
}

unsafe fn response_json(response: *mut u8, response_len: usize) -> serde_json::Value {
    let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn job_manager_runs_partition_analysis_through_the_c_abi() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("mbr.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    let mut sfdisk = Command::new("sfdisk")
        .arg(&image)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    sfdisk
        .stdin
        .take()
        .unwrap()
        .write_all(b"label: dos\nstart=2048, size=32768, type=83\n")
        .unwrap();
    let output = sfdisk.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    let status = unsafe { ofi_job_manager_new(2, &mut manager, &mut error) };
    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    assert!(!manager.is_null());
    assert!(error.is_null());

    let request = serde_json::to_vec(&serde_json::json!({
        "operation": "partition_analysis",
        "path": image,
    }))
    .unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let status = unsafe {
        ofi_job_submit(
            manager,
            request.as_ptr(),
            request.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };
    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    let submitted = unsafe { response_json(response, response_len) };
    unsafe { ofi_bytes_free(response, response_len) };
    let job_id = submitted["body"]["job_id"].as_str().unwrap().to_owned();
    let job_id_c = CString::new(job_id).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let snapshot = loop {
        let mut response = ptr::null_mut();
        let mut response_len = 0;
        let status = unsafe {
            ofi_job_snapshot(
                manager,
                job_id_c.as_ptr(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        };
        assert_eq!(status, ofi_status::OFI_STATUS_OK);
        let snapshot = unsafe { response_json(response, response_len) };
        unsafe { ofi_bytes_free(response, response_len) };
        if matches!(
            snapshot["body"]["status"].as_str(),
            Some("succeeded" | "partial" | "failed" | "cancelled")
        ) {
            break snapshot;
        }
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(10));
    };

    assert_eq!(snapshot["body"]["status"], "succeeded");
    assert_eq!(snapshot["body"]["result"]["scheme"], "mbr");
    assert!(
        snapshot["body"]["result"]["partitions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|partition| partition["start_sector"] == 2048)
    );

    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let status = unsafe {
        ofi_job_cancel(
            manager,
            job_id_c.as_ptr(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };
    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    let cancelled = unsafe { response_json(response, response_len) };
    unsafe { ofi_bytes_free(response, response_len) };
    assert_eq!(cancelled["body"]["cancelled"], false);
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn invalid_job_identifiers_are_typed_errors() {
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    let status = unsafe { ofi_job_manager_new(1, &mut manager, &mut error) };
    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    let invalid = CString::new("not-a-uuid").unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;

    let status = unsafe {
        ofi_job_snapshot(
            manager,
            invalid.as_ptr(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_ARGUMENT);
    assert!(response.is_null());
    assert_eq!(unsafe { error_code(error) }, "invalid_argument");
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn zero_job_workers_is_rejected() {
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();

    let status = unsafe { ofi_job_manager_new(0, &mut manager, &mut error) };

    assert_eq!(status, ofi_status::OFI_STATUS_OPERATION_FAILED);
    assert!(manager.is_null());
    assert_eq!(unsafe { error_code(error) }, "job.invalid_worker_count");
}
