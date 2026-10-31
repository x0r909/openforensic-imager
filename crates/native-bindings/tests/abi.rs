use std::ffi::{CStr, CString};
use std::io::Write;
use std::process::{Command, Stdio};
use std::ptr;
use std::time::{Duration, Instant};

use native_bindings::{
    ofi_abi_version, ofi_abi_version_minor, ofi_bytes_free, ofi_case_create, ofi_error,
    ofi_error_code, ofi_error_free, ofi_evidence_add, ofi_job_cancel, ofi_job_manager,
    ofi_job_manager_free, ofi_job_manager_new, ofi_job_snapshot, ofi_job_submit, ofi_status,
    validate_job_request,
};
use forensic_core::CaseId;
use proptest::prelude::*;
use std::os::unix::fs::DirBuilderExt;
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
    assert_eq!(ofi_abi_version_minor(), 2);
}

#[test]
fn job_request_validation_has_no_filesystem_side_effects() {
    let directory = tempdir().unwrap();
    let audit = directory.path().join("audit.jsonl");
    let request = serde_json::to_vec(&serde_json::json!({
        "operation": "audit_append",
        "audit_path": audit,
        "case_id": "00000000-0000-0000-0000-000000000030",
        "actor": "fuzzer",
        "action": "must.not.run",
        "subject_id": null,
        "details": {},
    }))
    .unwrap();

    let operation = validate_job_request(&request).unwrap();

    assert_eq!(operation, "audit_append");
    assert!(!audit.exists());
}

proptest! {
    #[test]
    fn arbitrary_job_request_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..8192)) {
        let _ = validate_job_request(&bytes);
    }
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

    // Omitting audit_path must still record the event, beside the case database.
    let audit = std::fs::read_to_string(directory.path().join("audit.jsonl"))
        .expect("case and evidence writes must always produce an audit record");
    assert!(audit.contains("case.created"), "missing case.created in {audit}");
    assert!(audit.contains("evidence.added"), "missing evidence.added in {audit}");
}

#[test]
fn omitted_audit_path_still_audits_beside_the_database() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("case").join("case.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let case_id = "00000000-0000-0000-0000-00000000003a";
    let request = format!(
        r#"{{"db_path":"{}","id":"{case_id}","title":"No Path Case","examiner":"Examiner"}}"#,
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
    unsafe { ofi_bytes_free(response, response_len) };

    let audit = path.parent().unwrap().join("audit.jsonl");
    let contents = std::fs::read_to_string(&audit)
        .unwrap_or_else(|_| panic!("no audit log at {}", audit.display()));
    assert!(
        contents.contains("case.created"),
        "a case committed without its audit record: {contents}"
    );
}

#[test]
fn audit_operations_are_bound_to_the_case() {
    let directory = tempdir().unwrap();
    let case_a_db = directory.path().join("a").join("case.db");
    let case_b_db = directory.path().join("b").join("case.db");
    std::fs::create_dir_all(case_a_db.parent().unwrap()).unwrap();
    std::fs::create_dir_all(case_b_db.parent().unwrap()).unwrap();

    let id_a = "00000000-0000-0000-0000-0000000000a1";
    let id_b = "00000000-0000-0000-0000-0000000000b1";
    for (db, id, title) in [
        (&case_a_db, id_a, "Case A"),
        (&case_b_db, id_b, "Case B"),
    ] {
        let request = format!(
            r#"{{"db_path":"{}","id":"{id}","title":"{title}","examiner":"Examiner"}}"#,
            db.display()
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
        assert_eq!(status, ofi_status::OFI_STATUS_OK, "case creation failed");
        unsafe { ofi_bytes_free(response, response_len) };
    }

    let log_a = directory.path().join("a").join("audit.jsonl");
    let log_b = directory.path().join("b").join("audit.jsonl");
    assert!(log_a.exists() && log_b.exists());

    // Each case is bound to its own log.
    let store_a = evidence_manager::CaseStore::open(&case_a_db).unwrap();
    let store_b = evidence_manager::CaseStore::open(&case_b_db).unwrap();
    let bound_a = store_a.case_audit_path(id_a.parse::<CaseId>().unwrap()).unwrap();
    let bound_b = store_b.case_audit_path(id_b.parse::<CaseId>().unwrap()).unwrap();
    assert_eq!(bound_a.as_deref(), Some(log_a.as_path()));
    assert_eq!(bound_b.as_deref(), Some(log_b.as_path()));

    // Appending case A's event into case B's log must be rejected.
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let request = serde_json::json!({
        "operation": "audit_append",
        "audit_path": log_b,
        "case_id": id_a,
        "actor": "examiner",
        "action": "evidence.opened",
        "subject_id": serde_json::Value::Null,
        "details": {},
        "db_path": case_a_db,
    });
    let job_id = unsafe { submit_job(manager, &request) }.unwrap();
    let snapshot = unsafe { wait_for_snapshot(manager, &job_id) };
    assert_eq!(snapshot["body"]["status"], "failed");
    assert_eq!(snapshot["body"]["error_code"], "audit.case");

    // The cross-case write must not have landed in case B's history.
    let contents = std::fs::read_to_string(&log_b).unwrap();
    assert!(
        !contents.contains("evidence.opened"),
        "case A's event was written into case B's log"
    );

    // Verifying case B's log while claiming case A is also rejected.
    let request = serde_json::json!({
        "operation": "audit_verify",
        "audit_path": log_b,
        "db_path": case_a_db,
        "case_id": id_a,
    });
    let job_id = unsafe { submit_job(manager, &request) }.unwrap();
    let snapshot = unsafe { wait_for_snapshot(manager, &job_id) };
    assert_eq!(snapshot["body"]["status"], "failed");
    assert_eq!(snapshot["body"]["error_code"], "audit.case");

    // The honest pair still works.
    let request = serde_json::json!({
        "operation": "audit_verify",
        "audit_path": log_b,
        "db_path": case_b_db,
        "case_id": id_b,
    });
    let job_id = unsafe { submit_job(manager, &request) }.unwrap();
    let snapshot = unsafe { wait_for_snapshot(manager, &job_id) };
    assert_eq!(snapshot["body"]["status"], "succeeded");
    assert_eq!(snapshot["body"]["result"]["valid"], true);

    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn case_create_with_unopenable_audit_path_refuses_to_commit() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("refused.db");
    let case_id = "00000000-0000-0000-0000-00000000003b";
    // A directory can never be opened as the audit log, so the preflight fails.
    let blocked = directory.path().join("audit-is-a-directory");
    std::fs::create_dir(&blocked).unwrap();
    let request = format!(
        r#"{{"db_path":"{}","id":"{case_id}","title":"Refused","examiner":"Examiner","audit_path":"{}"}}"#,
        path.display(),
        blocked.display()
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
    assert_ne!(status, ofi_status::OFI_STATUS_OK);
    let code = unsafe { error_code(error) };
    assert_eq!(code, "audit.open", "unexpected error code {code}");

    // The preflight must run before the commit, so the case row may not exist.
    if let Ok(store) = evidence_manager::CaseStore::open(&path) {
        let existing = store
            .list_evidence(case_id.parse::<CaseId>().unwrap())
            .expect("evidence list");
        assert!(
            existing.is_empty(),
            "a case must not commit when its audit log cannot open"
        );
    }
}

unsafe fn response_json(response: *mut u8, response_len: usize) -> serde_json::Value {
    let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    serde_json::from_slice(bytes).unwrap()
}

/// Submits a job request and returns its job id.
unsafe fn submit_job(
    manager: *mut ofi_job_manager,
    request: &serde_json::Value,
) -> Result<String, String> {
    let request = serde_json::to_vec(request).unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
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
    if status != ofi_status::OFI_STATUS_OK {
        unsafe { ofi_error_free(error) };
        return Err(format!("submit failed: {status:?}"));
    }
    let value = unsafe { response_json(response, response_len) };
    unsafe { ofi_bytes_free(response, response_len) };
    Ok(value["body"]["job_id"].as_str().unwrap().to_owned())
}

/// Polls until the job reaches a terminal status.
unsafe fn wait_for_snapshot(manager: *mut ofi_job_manager, job_id: &str) -> serde_json::Value {
    let job_id = CString::new(job_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut response = ptr::null_mut();
        let mut response_len = 0;
        let mut error = ptr::null_mut();
        let status = unsafe {
            ofi_job_snapshot(
                manager,
                job_id.as_ptr(),
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
            return snapshot;
        }
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
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

unsafe fn submit_request(manager: *mut ofi_job_manager, request: &serde_json::Value) -> String {
    let request = serde_json::to_vec(request).unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
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
    assert!(error.is_null());
    let submitted = unsafe { response_json(response, response_len) };
    unsafe { ofi_bytes_free(response, response_len) };
    submitted["body"]["job_id"].as_str().unwrap().to_owned()
}

unsafe fn wait_for_job(manager: *mut ofi_job_manager, job_id: &str) -> serde_json::Value {
    let job_id = CString::new(job_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut response = ptr::null_mut();
        let mut response_len = 0;
        let mut error = ptr::null_mut();
        let status = unsafe {
            ofi_job_snapshot(
                manager,
                job_id.as_ptr(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        };
        assert_eq!(status, ofi_status::OFI_STATUS_OK);
        assert!(error.is_null());
        let snapshot = unsafe { response_json(response, response_len) };
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
fn preview_and_extraction_jobs_return_bounded_results() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("phase5.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    let output = Command::new("mke2fs")
        .args(["-q", "-F", "-t", "ext4", "-L", "PHASE5"])
        .arg(&image)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = directory.path().join("hello.txt");
    std::fs::write(&text, b"hello from phase five\n").unwrap();
    let png = directory.path().join("image.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
    for (source, destination) in [(&text, "/hello.txt"), (&png, "/image.png")] {
        let output = Command::new("debugfs")
            .args(["-w", "-R"])
            .arg(format!("write {} {destination}", source.display()))
            .arg(&image)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(2, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let list_id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "filesystem_list",
                "path": image,
                "offset_bytes": 0,
                "inode": 2,
            }),
        )
    };
    let list = unsafe { wait_for_job(manager, &list_id) };
    assert_eq!(list["body"]["status"], "succeeded");
    let entries = list["body"]["result"]["entries"].as_array().unwrap();
    let text_entry = entries
        .iter()
        .find(|entry| entry["name"] == "hello.txt")
        .unwrap();
    let png_entry = entries
        .iter()
        .find(|entry| entry["name"] == "image.png")
        .unwrap();
    let text_address = text_entry["metadata_address"].as_u64().unwrap();
    let png_address = png_entry["metadata_address"].as_u64().unwrap();

    let hex_id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "hex_preview",
                "path": image,
                "offset_bytes": 0,
                "metadata_address": text_address,
                "file_offset": 0,
                "length": 8,
            }),
        )
    };
    let hex = unsafe { wait_for_job(manager, &hex_id) };
    assert_eq!(
        hex["body"]["result"]["bytes"],
        serde_json::json!([104, 101, 108, 108, 111, 32, 102, 114])
    );

    let text_id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "text_preview",
                "path": image,
                "offset_bytes": 0,
                "metadata_address": text_address,
                "file_offset": 0,
                "max_bytes": 256,
            }),
        )
    };
    let preview = unsafe { wait_for_job(manager, &text_id) };
    assert_eq!(preview["body"]["result"]["content"], "text");
    assert_eq!(preview["body"]["result"]["text"], "hello from phase five\n");

    let signature_id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "file_signature",
                "path": image,
                "offset_bytes": 0,
                "metadata_address": png_address,
            }),
        )
    };
    let signature = unsafe { wait_for_job(manager, &signature_id) };
    assert_eq!(signature["body"]["result"]["kind"], "png");

    let destination = directory.path().join("export");
    let extract_id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "extract_file",
                "path": image,
                "offset_bytes": 0,
                "parent_inode": 2,
                "metadata_address": text_address,
                "destination_root": destination,
            }),
        )
    };
    let extraction = unsafe { wait_for_job(manager, &extract_id) };
    assert_eq!(extraction["body"]["status"], "succeeded", "{extraction}");
    assert_eq!(
        std::fs::read(destination.join("hello.txt")).unwrap(),
        b"hello from phase five\n"
    );
    assert_eq!(
        extraction["body"]["result"]["output_sha256"],
        extraction["body"]["result"]["source_sha256"]
    );
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn directory_extraction_job_writes_reports() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("directory.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    let output = Command::new("mke2fs")
        .args(["-q", "-F", "-t", "ext4", "-L", "DIRJOB"])
        .arg(&image)
        .output()
        .unwrap();
    assert!(output.status.success());
    for (name, content) in [("first.txt", "first"), ("second.txt", "second")] {
        let source = directory.path().join(name);
        std::fs::write(&source, content).unwrap();
        let output = Command::new("debugfs")
            .args(["-w", "-R"])
            .arg(format!("write {} /{name}", source.display()))
            .arg(&image)
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let export_root = directory.path().join("export");
    let reports_root = directory.path().join("reports");
    // Owner-only: the worker refuses an output directory another principal can write.
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&reports_root)
        .unwrap();

    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "extract_directory",
                "path": image,
                "offset_bytes": 0,
                "inode": 2,
                "directory_name": "files",
                "destination_root": export_root,
                "reports_dir": reports_root,
                "base_name": "directory-export",
            }),
        )
    };
    let snapshot = unsafe { wait_for_job(manager, &id) };

    assert_eq!(snapshot["body"]["status"], "succeeded", "{snapshot}");
    assert_eq!(snapshot["body"]["result"]["manifest"]["status"], "complete");
    assert_eq!(
        snapshot["body"]["result"]["manifest"]["records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        std::fs::read(export_root.join("files/first.txt")).unwrap(),
        b"first"
    );
    assert_eq!(
        std::fs::read(export_root.join("files/second.txt")).unwrap(),
        b"second"
    );
    assert!(
        reports_root
            .join("directory-export.extraction.json")
            .exists()
    );
    assert!(
        reports_root
            .join("directory-export.extraction.csv")
            .exists()
    );
    assert!(
        reports_root
            .join("directory-export.extraction.html")
            .exists()
    );
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn acquisition_job_writes_reports_and_verification_job_verifies_output() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    std::fs::write(&source, vec![0x41_u8; 8192]).unwrap();
    let destination = directory.path().join("acquired");
    std::fs::create_dir(&destination).unwrap();
    let reports = directory.path().join("reports");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&reports)
        .unwrap();
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(2, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "acquisition",
                "source_path": source,
                "destination_dir": destination,
                "base_name": "evidence",
                "format": "raw",
                "options": {
                    "block_size": 4096,
                    "maximum_segment_size": 1048576,
                    "compression": "none",
                    "verify_after": true
                },
                "reports_dir": reports,
                "report_base_name": "phase5"
            }),
        )
    };
    let acquired = unsafe { wait_for_job(manager, &id) };
    assert_eq!(acquired["body"]["status"], "succeeded", "{acquired}");
    assert!(reports.join("phase5.acquisition.json").exists());
    assert!(reports.join("phase5.acquisition.csv").exists());
    assert!(reports.join("phase5.acquisition.html").exists());
    let image = destination.join("evidence.raw");
    let expected = acquired["body"]["result"]["manifest"]["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hash| hash["scope"] == "output_logical" && hash["algorithm"] == "sha256")
        .unwrap()["digest_hex"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        acquired["body"]["result"]["manifest"]["hashes"][0]["scope"],
        "output_logical"
    );

    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "verification",
                "path": image,
                "algorithms": ["sha256"],
                "expected": {"sha256": expected},
                "reports_dir": reports,
                "report_base_name": "phase5"
            }),
        )
    };
    let verified = unsafe { wait_for_job(manager, &id) };
    assert_eq!(verified["body"]["status"], "succeeded");
    assert_eq!(
        verified["body"]["result"]["report"]["outcomes"]["sha256"],
        "match"
    );
    assert!(reports.join("phase5.verification.json").exists());
    assert!(reports.join("phase5.verification.csv").exists());
    assert!(reports.join("phase5.verification.html").exists());
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn ewf_acquisition_and_verification_run_through_the_parser_worker() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    std::fs::write(&source, vec![0x43_u8; 128 * 1024]).unwrap();
    let destination = directory.path().join("acquired");
    std::fs::create_dir(&destination).unwrap();
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "acquisition",
                "source_path": source,
                "destination_dir": destination,
                "base_name": "evidence",
                "format": "ewf1_e01",
                "options": {
                    "block_size": 4096,
                    "maximum_segment_size": 1048576,
                    "compression": "none",
                    "verify_after": true
                }
            }),
        )
    };
    let acquired = unsafe { wait_for_job(manager, &id) };
    assert_eq!(acquired["body"]["status"], "succeeded", "{acquired}");
    assert_eq!(acquired["body"]["result"]["manifest"]["format"], "ewf1_e01");
    let image = destination.join("evidence.E01");
    assert!(image.is_file());
    let expected = acquired["body"]["result"]["manifest"]["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hash| hash["scope"] == "output_logical" && hash["algorithm"] == "sha256")
        .unwrap()["digest_hex"]
        .as_str()
        .unwrap()
        .to_owned();

    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "verification",
                "path": image,
                "algorithms": ["sha256"],
                "expected": {"sha256": expected}
            }),
        )
    };
    let verified = unsafe { wait_for_job(manager, &id) };
    assert_eq!(verified["body"]["status"], "succeeded", "{verified}");
    assert_eq!(
        verified["body"]["result"]["report"]["outcomes"]["sha256"],
        "match"
    );
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn source_snapshot_and_evidence_list_jobs_work() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("image.raw");
    std::fs::write(&image, vec![0x42_u8; 4096]).unwrap();
    let database = directory.path().join("case.db");
    let case_id = "00000000-0000-0000-0000-000000000010";
    let evidence_id = "00000000-0000-0000-0000-000000000011";
    let request = format!(
        r#"{{"db_path":"{}","id":"{case_id}","title":"UI Case","examiner":"Examiner"}}"#,
        database.display()
    );
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_case_create(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    unsafe { ofi_bytes_free(response, response_len) };
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({"operation": "source_snapshot", "path": image}),
        )
    };
    let snapshot = unsafe { wait_for_job(manager, &id) };
    assert_eq!(snapshot["body"]["status"], "succeeded");
    assert_eq!(snapshot["body"]["result"]["logical_length"], 4096);
    assert_eq!(snapshot["body"]["result"]["source"]["read_only"], true);
    let source = snapshot["body"]["result"]["source"].clone();
    let request = format!(
        r#"{{"db_path":"{}","id":"{evidence_id}","case_id":"{case_id}","description":"image","source_path":"{}","source":{}}}"#,
        database.display(),
        image.display(),
        source
    );
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    assert_eq!(
        unsafe {
            ofi_evidence_add(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    unsafe { ofi_bytes_free(response, response_len) };

    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "evidence_list",
                "db_path": database,
                "case_id": case_id
            }),
        )
    };
    let listed = unsafe { wait_for_job(manager, &id) };
    assert_eq!(listed["body"]["status"], "succeeded");
    assert_eq!(listed["body"]["result"][0]["id"], evidence_id);
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn case_and_evidence_writes_append_to_the_audit_chain() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("case.db");
    let audit = directory.path().join("audit.jsonl");
    let case_id = "00000000-0000-0000-0000-000000000020";
    let evidence_id = "00000000-0000-0000-0000-000000000021";
    let request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": case_id,
        "title": "Audited case",
        "examiner": "Examiner",
        "audit_path": audit,
    }))
    .unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_case_create(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    unsafe { ofi_bytes_free(response, response_len) };
    let request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": evidence_id,
        "case_id": case_id,
        "description": "disk",
        "source_path": "/tmp/disk.raw",
        "source": {
            "device": 1, "inode": 2, "length": 4096,
            "mtime_seconds": 10, "mtime_nanoseconds": 3,
            "ctime_seconds": 20, "ctime_nanoseconds": 4,
            "read_only": true
        },
        "audit_path": audit,
    }))
    .unwrap();
    let evidence_status = unsafe {
        ofi_evidence_add(
            request.as_ptr(),
            request.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };
    assert_eq!(evidence_status, ofi_status::OFI_STATUS_OK);
    unsafe { ofi_bytes_free(response, response_len) };

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({"operation": "audit_verify", "audit_path": audit}),
        )
    };
    let verified = unsafe { wait_for_job(manager, &id) };

    assert_eq!(
        verified["body"]["status"], "succeeded",
        "audit_verify failed: {verified}"
    );
    assert_eq!(verified["body"]["result"]["valid"], true, "{verified}");
    assert_eq!(verified["body"]["result"]["record_count"], 2, "{verified}");
    assert!(verified["body"]["result"]["head_hash"].is_string());
    unsafe { ofi_job_manager_free(manager) };
}

#[test]
fn audit_open_failure_prevents_case_commit() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("case.db");
    let invalid_audit = directory.path().join("audit-directory");
    std::fs::create_dir(&invalid_audit).unwrap();
    let valid_audit = directory.path().join("audit.jsonl");
    let case_id = "00000000-0000-0000-0000-000000000040";
    let mut request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": case_id,
        "title": "Preflight case",
        "examiner": "Examiner",
        "audit_path": invalid_audit,
    }))
    .unwrap();
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

    assert_eq!(status, ofi_status::OFI_STATUS_OPERATION_FAILED);
    assert_eq!(unsafe { error_code(error) }, "audit.open");

    request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": case_id,
        "title": "Preflight case",
        "examiner": "Examiner",
        "audit_path": valid_audit,
    }))
    .unwrap();
    error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_case_create(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    unsafe { ofi_bytes_free(response, response_len) };
}

/// P6-SEC-002: a committed case must never be lost from the audit history.
///
/// The audit append can fail after the database commit (a full or failing
/// device). The event must stay queued and be replayed into the chain by the
/// next audited operation, so the case is never silently unlogged.
#[test]
fn audit_append_failure_is_recovered_from_the_durable_outbox() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("case.db");
    let audit = directory.path().join("audit.jsonl");
    let case_id = "00000000-0000-0000-0000-000000000060";
    let evidence_id = "00000000-0000-0000-0000-000000000061";

    // /dev/full opens successfully and fails only on write, so the preflight
    // passes and the failure lands after the case row is committed.
    let request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": case_id,
        "title": "Committed but unlogged",
        "examiner": "Examiner",
        "audit_path": "/dev/full",
    }))
    .unwrap();
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
    assert_eq!(status, ofi_status::OFI_STATUS_OPERATION_FAILED);
    assert_eq!(unsafe { error_code(error) }, "audit.append_after_commit");

    // The case row is committed and the event is durably queued.
    let store = evidence_manager::CaseStore::open(&database).unwrap();
    assert_eq!(store.pending_audit_count().unwrap(), 1);

    // A later write to a working log replays the queued event into the chain.
    // The failed log was never bound, because it could not record anything.
    let request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": evidence_id,
        "case_id": case_id,
        "description": "disk",
        "source_path": "/tmp/disk.raw",
        "source": {
            "device": 1, "inode": 2, "length": 4096,
            "mtime_seconds": 10, "mtime_nanoseconds": 3,
            "ctime_seconds": 20, "ctime_nanoseconds": 4,
            "read_only": true
        },
        "audit_path": audit,
    }))
    .unwrap();
    error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_evidence_add(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    unsafe { ofi_bytes_free(response, response_len) };

    let store = evidence_manager::CaseStore::open(&database).unwrap();
    assert_eq!(
        store.pending_audit_count().unwrap(),
        0,
        "replayed events must be marked appended"
    );

    let log = std::fs::read_to_string(&audit).unwrap();
    assert!(
        log.contains("case.created"),
        "the committed case was never replayed into the chain: {log}"
    );
    assert!(log.contains("evidence.added"), "missing evidence.added: {log}");

    let status = audit_log::AuditLog::verify_chain(&audit).unwrap();
    assert!(status.is_valid(), "{:?}", status.reason());
    assert_eq!(status.record_count(), 2);
}

/// A chain that verifies but sits behind the head recorded for the case has
/// lost records. Chain-internal checks cannot see that, so the recorded head
/// must be compared.
#[test]
fn audit_verify_detects_a_chain_behind_the_recorded_head() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("case.db");
    let audit = directory.path().join("audit.jsonl");
    let case_id = "00000000-0000-0000-0000-000000000070";

    let request = serde_json::to_vec(&serde_json::json!({
        "db_path": database,
        "id": case_id,
        "title": "Rollback case",
        "examiner": "Examiner",
        "audit_path": audit,
    }))
    .unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe {
            ofi_case_create(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
            )
        },
        ofi_status::OFI_STATUS_OK
    );
    unsafe { ofi_bytes_free(response, response_len) };

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );

    // First verification records the head.
    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "audit_verify",
                "audit_path": audit,
                "db_path": database,
                "case_id": case_id,
            }),
        )
    };
    let verified = unsafe { wait_for_job(manager, &id) };
    assert_eq!(verified["body"]["status"], "succeeded", "{verified}");

    // Roll the chain back to an empty file: it verifies trivially, but it no
    // longer reaches the head recorded for the case.
    std::fs::write(&audit, "").unwrap();

    let id = unsafe {
        submit_request(
            manager,
            &serde_json::json!({
                "operation": "audit_verify",
                "audit_path": audit,
                "db_path": database,
                "case_id": case_id,
            }),
        )
    };
    let verified = unsafe { wait_for_job(manager, &id) };
    assert_eq!(verified["body"]["status"], "failed", "{verified}");
    assert_eq!(verified["body"]["error_code"], "audit.head_mismatch");

    unsafe { ofi_job_manager_free(manager) };
}
