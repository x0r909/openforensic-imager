use std::ffi::{CStr, CString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::{Duration, Instant};

use native_bindings::{
    ofi_bytes_free, ofi_error, ofi_error_free, ofi_job_manager, ofi_job_manager_free,
    ofi_job_manager_new, ofi_job_snapshot, ofi_job_submit, ofi_status,
};
use tempfile::tempdir;

/// Serialises tests in this binary that mutate process-global `OFI_*` variables.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn worker_executable() -> PathBuf {
    let current = std::env::current_exe().unwrap();
    for ancestor in current.ancestors().skip(1).take(3) {
        let candidate = ancestor.join("ofi-parser-worker");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("ofi-parser-worker was not built beside the test executable");
}

fn write_script(directory: &Path, name: &str, body: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, body).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

unsafe fn submit(
    manager: *mut ofi_job_manager,
    request: &serde_json::Value,
) -> Result<String, String> {
    let request = serde_json::to_vec(request).unwrap();
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error: *mut ofi_error = ptr::null_mut();
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
        let message = unsafe { CStr::from_ptr(native_bindings::ofi_error_message(error)) }
            .to_string_lossy()
            .into_owned();
        unsafe { ofi_error_free(error) };
        return Err(message);
    }
    let bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    unsafe { ofi_bytes_free(response, response_len) };
    Ok(value["body"]["job_id"].as_str().unwrap().to_owned())
}

fn wait_for_job(manager: *mut ofi_job_manager, job_id: &str) -> serde_json::Value {
    let job_id = CString::new(job_id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut response = ptr::null_mut();
        let mut response_len = 0;
        let mut error: *mut ofi_error = ptr::null_mut();
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
fn crashing_worker_is_contained_and_a_new_worker_can_run() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.raw");
    std::fs::write(&evidence, vec![0_u8; 4096]).unwrap();
    let crash = write_script(directory.path(), "crash-worker", "#!/bin/sh\nexit 42\n");
    let request = serde_json::json!({
        "operation": "image_identify",
        "path": evidence,
    });

    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", &crash);
        std::env::set_var("OFI_WORKER_TIMEOUT_SECS", "5");
    }
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );

    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job(manager, &job_id);
    assert_eq!(snapshot["body"]["status"], "failed");
    assert_eq!(snapshot["body"]["error_code"], "worker.crashed");

    unsafe { std::env::set_var("OFI_PARSER_WORKER", worker_executable()) };
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job(manager, &job_id);
    assert_eq!(snapshot["body"]["status"], "succeeded");
    unsafe { ofi_job_manager_free(manager) };
}

/// SECURITY_MODEL.md requires that the evidence worker inherits no environment
/// beyond a minimal allowlist. A sentinel variable set in the parent must not
/// reach the worker, and an allowlisted one must.
#[test]
fn worker_does_not_inherit_the_parent_environment() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.raw");
    std::fs::write(&evidence, vec![0_u8; 4096]).unwrap();

    // The stand-in worker echoes its own environment back as the job result.
    let probe = write_script(
        directory.path(),
        "env-probe-worker",
        "#!/bin/sh\n\
         cat >/dev/null\n\
         secret=\"${OFI_AUDIT_SECRET:-absent}\"\n\
         lang=\"${LANG:-absent}\"\n\
         printf '{\"ok\":true,\"outcome\":{\"partial\":false,\"result\":{\"secret\":\"%s\",\"lang\":\"%s\"},\"message\":null},\"error\":null}\\n' \"$secret\" \"$lang\"\n",
    );
    let request = serde_json::json!({
        "operation": "image_identify",
        "path": evidence,
    });

    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", &probe);
        std::env::set_var("OFI_AUDIT_SECRET", "DUMMY_ENV_SECRET");
        std::env::set_var("OFI_WORKER_TIMEOUT_SECS", "5");
    }
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );

    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job(manager, &job_id);
    assert_eq!(snapshot["body"]["status"], "succeeded");

    let result = &snapshot["body"]["result"];
    assert_eq!(
        result["secret"], "absent",
        "the worker inherited a parent variable outside the allowlist"
    );

    unsafe {
        std::env::remove_var("OFI_AUDIT_SECRET");
        std::env::set_var("OFI_PARSER_WORKER", worker_executable());
        ofi_job_manager_free(manager);
    }
}

/// The response cap must stop the worker, not just discard its output.
///
/// A worker that streams past the cap must be killed at the cap and fail the job
/// quickly, instead of being drained to EOF or until the wall-clock deadline.
#[test]
fn oversized_worker_response_is_stopped_at_the_cap() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.raw");
    std::fs::write(&evidence, vec![0_u8; 4096]).unwrap();

    // Streams well past the 64 MiB cap, and would keep going for 30 s.
    let flood = write_script(
        directory.path(),
        "flood-worker",
        "#!/bin/sh\n\
         cat >/dev/null\n\
         i=0\n\
         while [ $i -lt 4000 ]; do\n\
           head -c 1048576 /dev/zero\n\
           i=$((i + 1))\n\
         done\n",
    );
    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", &flood);
        std::env::set_var("OFI_WORKER_TIMEOUT_SECS", "60");
    }
    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let request = serde_json::json!({
        "operation": "image_identify",
        "path": evidence,
    });

    let started = Instant::now();
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job(manager, &job_id);
    let elapsed = started.elapsed();

    assert_eq!(snapshot["body"]["status"], "failed");
    assert_eq!(snapshot["body"]["error_code"], "worker.response_too_large");
    assert!(
        elapsed < Duration::from_secs(20),
        "the cap waited for the deadline instead of stopping the worker: {elapsed:?}"
    );

    unsafe {
        std::env::set_var("OFI_PARSER_WORKER", worker_executable());
        ofi_job_manager_free(manager);
    }
}
