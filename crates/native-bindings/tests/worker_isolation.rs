use std::ffi::{CStr, CString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::{Duration, Instant};

use libewf_bindings::{EwfCompression, EwfWriteOptions, EwfWriter};
use native_bindings::{
    ofi_bytes_free, ofi_error, ofi_error_free, ofi_job_manager, ofi_job_manager_free,
    ofi_job_manager_new, ofi_job_snapshot, ofi_job_submit, ofi_status,
};
use tempfile::tempdir;

/// Serialises tests in this binary that mutate process-global `OFI_*` variables.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn worker_executable() -> PathBuf {
    let current = std::env::current_exe().unwrap();
    // Cargo on this host emits test binaries below
    // `target/<triple>/debug/build/<crate>/<hash>/out`, so the worker sits five
    // levels up rather than the two a plain `target/debug/deps` layout needs.
    for ancestor in current.ancestors().skip(1).take(8) {
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

/// The paths, out of a policy's read set, that live inside `root`. Filtered so
/// the assertion does not depend on the unrelated grants the policy also
/// carries (`/usr/share/zoneinfo`, `/etc/localtime`).
fn reads_beneath(root: &Path, read_paths: &[PathBuf]) -> Vec<PathBuf> {
    let root = std::fs::canonicalize(root).unwrap();
    let mut paths: Vec<PathBuf> = read_paths
        .iter()
        .filter(|path| path.starts_with(&root))
        .cloned()
        .collect();
    paths.sort();
    paths
}

/// Residual risk 4 in docs/security/phase8-review.md: an EWF image is split
/// across `E01`/`E02`/… siblings, and the grant used to cover the whole
/// directory to reach them, exposing every unrelated file beside the evidence.
///
/// The grant must name each segment instead — enough for the job to succeed
/// (both segments are readable), and no more (the directory is not).
#[test]
fn ewf_reads_are_granted_per_segment_and_not_per_directory() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let directory = tempdir().unwrap();
    let source = directory.path().join("source");
    let media = vec![0x5a_u8; 5 * 1024 * 1024 / 2];
    let mut writer = EwfWriter::create(
        &source,
        media.len() as u64,
        EwfWriteOptions {
            bytes_per_sector: 512,
            sectors_per_chunk: 64,
            maximum_segment_size: 1024 * 1024,
            compression: EwfCompression::None,
        },
    )
    .unwrap();
    writer.write_all(&media).unwrap();
    let segments = writer.finalize().unwrap();
    assert!(
        segments.len() > 1,
        "the fixture has to span more than one segment to prove anything: {segments:?}"
    );

    let destination = directory.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": segments[0],
        "destination_dir": destination,
        "base_name": "copy",
        "format": "ewf1_e01",
        "options": {
            "block_size": 4096,
            "maximum_segment_size": 1048576,
            "compression": "none",
            "verify_after": true
        }
    });

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job(manager, &job_id);
    assert_eq!(
        snapshot["body"]["status"], "succeeded",
        "a per-segment grant must still read the whole image: {snapshot}"
    );
    unsafe { ofi_job_manager_free(manager) };

    let encoded = serde_json::to_vec(&request).unwrap();
    let (read_paths, _) = native_bindings::sandbox_paths_for_debug(&encoded).unwrap();

    let mut expected: Vec<PathBuf> = segments
        .iter()
        .map(|segment| std::fs::canonicalize(segment).unwrap())
        .collect();
    expected.sort();
    assert_eq!(
        reads_beneath(directory.path(), &read_paths),
        expected,
        "the policy must grant the segments themselves"
    );

    let parent = std::fs::canonicalize(directory.path()).unwrap();
    assert!(
        !read_paths.contains(&parent),
        "the grant widened back to the directory holding the evidence: {read_paths:?}"
    );
}

/// Whether a path is EWF is decided by signature, the same way `open_image`
/// decides it. An extension-only test would call a raw file named `.E01` EWF and
/// sweep up every `.E02`, `.E03`, … beside it — files the reader never opens,
/// granted to the parser anyway.
#[test]
fn an_ewf_extension_without_an_ewf_signature_does_not_widen_the_grant() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("mislabelled.E01");
    std::fs::write(&image, vec![0x11_u8; 8192]).unwrap();
    let neighbour = directory.path().join("mislabelled.E02");
    std::fs::write(&neighbour, vec![0x22_u8; 8192]).unwrap();

    let request = serde_json::json!({
        "operation": "image_identify",
        "path": image,
    });
    let encoded = serde_json::to_vec(&request).unwrap();

    let (read_paths, _) = native_bindings::sandbox_paths_for_debug(&encoded).unwrap();

    assert_eq!(
        reads_beneath(directory.path(), &read_paths),
        vec![std::fs::canonicalize(&image).unwrap()],
        "a file that only looks like a segment must be granted as itself: {read_paths:?}"
    );
}

/// Narrowing the grant must not turn an incomplete evidence set into a silent
/// result. An image whose later segment is gone must fail the job, not acquire
/// the first segment and report success on a truncated copy.
#[test]
fn a_missing_ewf_segment_fails_the_job_instead_of_narrowing_it() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let directory = tempdir().unwrap();
    let source = directory.path().join("source");
    let media = vec![0x5a_u8; 5 * 1024 * 1024 / 2];
    let mut writer = EwfWriter::create(
        &source,
        media.len() as u64,
        EwfWriteOptions {
            bytes_per_sector: 512,
            sectors_per_chunk: 64,
            maximum_segment_size: 1024 * 1024,
            compression: EwfCompression::None,
        },
    )
    .unwrap();
    writer.write_all(&media).unwrap();
    let segments = writer.finalize().unwrap();
    assert!(segments.len() > 1, "{segments:?}");
    std::fs::remove_file(segments.last().unwrap()).unwrap();

    let destination = directory.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": segments[0],
        "destination_dir": destination,
        "base_name": "copy",
        "format": "ewf1_e01",
        "options": {
            "block_size": 4096,
            "maximum_segment_size": 1048576,
            "compression": "none",
            "verify_after": true
        }
    });

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job(manager, &job_id);
    unsafe { ofi_job_manager_free(manager) };

    assert_eq!(
        snapshot["body"]["status"], "partial",
        "a missing segment has to surface as a partial result, never as success: {snapshot}"
    );
    let manifest = &snapshot["body"]["result"]["manifest"];
    assert_eq!(manifest["status"], "partial");
    assert!(
        !manifest["unreadable_ranges"].as_array().unwrap().is_empty(),
        "the manifest has to name the byte ranges it could not read: {manifest}"
    );
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
