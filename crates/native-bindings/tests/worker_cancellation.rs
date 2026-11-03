use std::ffi::CString;
use std::os::unix::fs::PermissionsExt;
use std::ptr;
use std::time::{Duration, Instant};

use native_bindings::{
    ofi_bytes_free, ofi_error, ofi_job_cancel, ofi_job_manager, ofi_job_manager_free,
    ofi_job_manager_new, ofi_job_snapshot, ofi_job_submit, ofi_status,
};
use tempfile::tempdir;

/// Serialises the two tests in this binary that mutate process-global `OFI_*`
/// variables.
///
/// Both tests point `OFI_PARSER_WORKER` at a script of their own and the cargo
/// harness runs them on separate threads. Without this lock one test's write
/// lands between the other's write and its `submit`, so a job spawns the other
/// test's script. They happen to be byte-identical today, which is why it passes
/// — but the failure it is waiting for is a cancelled job that never starts, or
/// a busy one that cancels instead.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Restores the process-global `OFI_*` variables a test set, when it ends.
///
/// Mirrors the guard in `worker_isolation.rs`. The worker reads its spawn
/// configuration from the test process's environment, so a value left behind —
/// a `OFI_WORKER_TIMEOUT_SECS` of 60 s — changes how every later test's worker
/// is spawned. Restoring the previous value rather than removing the key also
/// preserves an operator's own setting.
struct EnvironmentGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvironmentGuard {
    fn new() -> Self {
        Self { saved: Vec::new() }
    }

    /// Sets a variable, remembering the value it replaced.
    ///
    /// The previous value is read here, immediately before the write, so there
    /// is no way to call this in the wrong order and save the value just set.
    ///
    /// # Safety
    ///
    /// The caller must hold `ENV_LOCK`, which is what makes mutating the
    /// process environment sound while the other test runs.
    unsafe fn set(&mut self, key: &'static str, value: impl AsRef<std::ffi::OsStr>) {
        if !self.saved.iter().any(|(saved, _)| *saved == key) {
            self.saved.push((key, std::env::var_os(key)));
        }
        unsafe { std::env::set_var(key, value) };
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            // SAFETY: every test that mutates these variables holds ENV_LOCK,
            // and the guard is dropped inside that critical section.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

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
    // Both tests in this binary mutate the same two variables, so the lock is
    // taken before either is written and held to the end of the test.
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut environment = EnvironmentGuard::new();
    unsafe {
        environment.set("OFI_PARSER_WORKER", slow);
        environment.set("OFI_WORKER_TIMEOUT_SECS", "60");
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
    let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut environment = EnvironmentGuard::new();
    unsafe {
        environment.set("OFI_PARSER_WORKER", slow);
        environment.set("OFI_WORKER_TIMEOUT_SECS", "60");
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
