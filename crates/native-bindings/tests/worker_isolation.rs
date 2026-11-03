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

/// Restores the process-global `OFI_*` variables a test set, when it ends.
///
/// The worker reads its spawn configuration — which executable, which
/// timeout — from the test process's environment. Those variables are global to
/// the binary, so a test that sets one changes how *every other* test's worker
/// is spawned; holding `ENV_LOCK` serialises the writes but does not undo them.
/// Left unrestored, whichever test runs next inherits the previous test's
/// budget: a `OFI_WORKER_TIMEOUT_SECS` of 5 s set by a crash test then kills
/// the acquisition tests, which need minutes, with `worker.timeout` — the test
/// binary's outcome would depend on test order and on the host's scheduling.
///
/// Restoring the prior value rather than removing the variable also preserves
/// an operator's own setting: a deliberate `OFI_WORKER_TIMEOUT_SECS` in the
/// environment survives the suite.
struct EnvironmentGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvironmentGuard {
    /// A guard that has saved nothing yet.
    fn new() -> Self {
        Self { saved: Vec::new() }
    }

    /// Sets a variable, remembering the value it replaced.
    ///
    /// The previous value is read here, immediately before the write, rather
    /// than in a separate capture step — so there is no way to call this in the
    /// wrong order and save the value the test just set.
    ///
    /// # Safety
    ///
    /// The caller must hold `ENV_LOCK`, which is what makes mutating the
    /// process environment sound while other tests run.
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

/// The widest stretch an operator may declare.
///
/// It exists so that a mistyped factor cannot turn a deadline into a wait that
/// never ends: however large the scale, the job still has to finish. The
/// longest deadline in this file is five minutes, so a hundredfold is around
/// eight hours — finite, and large enough to be obviously deliberate.
///
/// It is also the documented ceiling for the value the harness passes. That
/// value is not a guess: measured on this project's arm64 guest under TCG,
/// against the workload the slowest tests actually do, `sha256sum` over a
/// 70 MiB image takes 0.94 s natively and 8.9 s in the guest, about 9x. A
/// synthetic arithmetic loop measures far worse (143x) because TCG translates
/// every instruction while a hash spends its time in already-compiled native
/// code; hashing is what these tests do, so it is what the factor is
/// calibrated on.
const MAX_DEADLINE_SCALE: u32 = 100;

/// The factor by which this file's wall-clock deadlines are stretched.
///
/// Every deadline here is written for a host running the job natively. Under
/// whole-system emulation the same job takes tens of times as long, and it is
/// the deadline, not the code, that fails: `scripts/test-clean-vm.sh` boots
/// arm64 through TCG, where a 70 MiB acquisition that takes a couple of minutes
/// on the build host does not finish inside the five minutes this file allows.
/// One slow host then reads as five broken tests.
///
/// `OFI_TEST_DEADLINE_SCALE` lets the emulated run declare its factor rather
/// than have the suite guess it. The default of 1 leaves a native run exactly
/// as it was.
///
/// The deadline stays finite at every factor — the job still has to reach a
/// terminal state, so a genuine hang still fails the test, after a longer and
/// explicitly stated wait rather than a ten-second one. A value that is not an
/// integer, is zero, or exceeds [`MAX_DEADLINE_SCALE`] is ignored, so a typo
/// leaves the native deadlines in place instead of removing the bound.
fn deadline_scale() -> u32 {
    let Ok(value) = std::env::var("OFI_TEST_DEADLINE_SCALE") else {
        return 1;
    };
    match value.trim().parse::<u32>() {
        Ok(scale) if (1..=MAX_DEADLINE_SCALE).contains(&scale) => scale,
        _ => {
            eprintln!(
                "ignoring OFI_TEST_DEADLINE_SCALE={value:?}: expected an integer in \
                 1..={MAX_DEADLINE_SCALE}"
            );
            1
        }
    }
}

/// A deadline written for native speed, stretched by [`deadline_scale`].
fn scaled(deadline: Duration) -> Duration {
    deadline * deadline_scale()
}

/// A worker budget written for native speed, stretched the same way.
///
/// The worker times a job out on its own clock (`OFI_WORKER_TIMEOUT_SECS`), and
/// an emulated run has to stretch that too: a budget that kills the worker at
/// sixty seconds fails the job with `worker.timeout` long before the parent's
/// stretched deadline would fire, and the test would read as a worker defect
/// rather than a slow host.
fn scaled_budget(seconds: u64) -> String {
    (seconds * u64::from(deadline_scale())).to_string()
}

fn wait_for_job(manager: *mut ofi_job_manager, job_id: &str) -> serde_json::Value {
    wait_for_job_within(manager, job_id, Duration::from_secs(10))
}

/// The same wait, with a caller-chosen deadline.
///
/// A test whose job hashes a large image needs longer than the 10 s that suits
/// a job over a fixture: the four digests run over every byte, so the job is
/// minutes of work even when nothing is wrong. The deadline is the test's to
/// state, rather than the shared helper's to guess — and it is written for a
/// native host, then stretched by [`deadline_scale`] so an emulated run can
/// declare the factor instead of every test being re-tuned for it.
fn wait_for_job_within(
    manager: *mut ofi_job_manager,
    job_id: &str,
    within: Duration,
) -> serde_json::Value {
    let job_id = CString::new(job_id).unwrap();
    let within = scaled(within);
    let deadline = Instant::now() + within;
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

    let mut environment = EnvironmentGuard::new();
    unsafe {
        environment.set("OFI_PARSER_WORKER", &crash);
        environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(5));
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

    unsafe { environment.set("OFI_PARSER_WORKER", worker_executable()) };
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

    let mut environment = EnvironmentGuard::new();
    unsafe {
        environment.set("OFI_PARSER_WORKER", &probe);
        environment.set("OFI_AUDIT_SECRET", "DUMMY_ENV_SECRET");
        environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(5));
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
    let mut environment = EnvironmentGuard::new();
    unsafe {
        environment.set("OFI_PARSER_WORKER", &flood);
        environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(60));
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
        elapsed < scaled(Duration::from_secs(20)),
        "the cap waited for the deadline instead of stopping the worker: {elapsed:?}"
    );

    unsafe {
        ofi_job_manager_free(manager);
    }
}

/// An acquisition larger than the worker's default file-size limit must not
/// die at the limit.
///
/// `RLIMIT_FSIZE` caps one file, and acquisition writes evidence — a raw image
/// is exactly as large as its source. A worker whose limit is smaller than the
/// evidence does not fail the acquisition: it is killed by `SIGXFSZ` on the
/// write that crosses the limit, so the job ends with no manifest at all and a
/// truncated image is left in the staging directory. The budget therefore has
/// to follow the job, not the worker.
///
/// `verify_after` is off because verification would hash the finished image a
/// second time; the assertion is on the byte counts, which is what the limit
/// bounds. A run with verification off reports `partial` by design, so the test
/// asserts against `failed` rather than for `succeeded`.
#[test]
fn acquisition_larger_than_the_default_file_size_limit_completes() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    // This job hashes 70 MiB four times, so minutes of wall clock are normal.
    // The parent deadline below says that; the *worker's* deadline has to say
    // it too, or an ambient `OFI_WORKER_TIMEOUT_SECS` — an operator's, or one
    // a sibling test left behind — would kill the worker mid-run. The test
    // declares its budget rather than inheriting one.
    let mut environment = EnvironmentGuard::new();
    unsafe { environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(600)) };

    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    // Past the 64 MiB default, so the write crosses the old limit partway
    // through. Kept close to it: every byte is hashed by four algorithms, and
    // the margin over the limit is what the test is about.
    let source_bytes = 70 * 1024 * 1024;
    let file = std::fs::File::create(&source).unwrap();
    file.set_len(source_bytes).unwrap();
    drop(file);

    let destination = directory.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": source,
        "destination_dir": destination,
        "base_name": "copy",
        "format": "raw",
        "options": {
            "block_size": 4096,
            "maximum_segment_size": 1048576,
            "compression": "none",
            "verify_after": false
        }
    });

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job_within(manager, &job_id, Duration::from_secs(300));
    unsafe { ofi_job_manager_free(manager) };

    assert_ne!(
        snapshot["body"]["status"], "failed",
        "evidence larger than the worker's default file-size limit must still \
         be acquired, not refused at the limit: {snapshot}"
    );
    assert_eq!(
        snapshot["body"]["result"]["manifest"]["bytes_written"], source_bytes,
        "the manifest has to account for the whole source: {snapshot}"
    );
    // A run with verification off stays in staging, so the artifact is found
    // beside the manifest rather than assumed to have reached the destination.
    let manifest_path = PathBuf::from(
        snapshot["body"]["result"]["manifest_path"]
            .as_str()
            .expect("the acquisition reported a manifest path"),
    );
    let written = std::fs::metadata(manifest_path.parent().unwrap().join("copy.raw"))
        .unwrap()
        .len();
    assert_eq!(
        written, source_bytes,
        "the image on disk has to be the whole source, not a prefix cut at the \
         worker's default limit"
    );
}

/// A source far smaller than the default floor is still acquired.
///
/// The per-job budget is derived from the source length, and that length is not
/// all the worker writes: the acquisition also writes its own manifest, which
/// is larger than a tiny image. A budget equal to the source exactly therefore
/// fails the job on the manifest that describes it — and it fails for a small
/// source, which is the opposite of the intuition that only large jobs can
/// reach a file-size limit. The budget is a floor under the derived value for
/// this reason, and this is the test that says so.
#[test]
fn a_source_smaller_than_the_file_size_floor_is_still_acquired() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut environment = EnvironmentGuard::new();
    unsafe { environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(60)) };

    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    // Small enough that the source length alone would be a budget smaller than
    // the manifest, and small enough to fit inside one filesystem block.
    let source_bytes = 512_u64;
    std::fs::write(&source, vec![0x41_u8; source_bytes as usize]).unwrap();

    let destination = directory.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": source,
        "destination_dir": destination,
        "base_name": "tiny",
        "format": "raw",
        "options": {
            "block_size": 4096,
            "maximum_segment_size": 1048576,
            "compression": "none",
            "verify_after": false
        }
    });

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job_within(manager, &job_id, Duration::from_secs(60));
    unsafe { ofi_job_manager_free(manager) };

    assert_ne!(
        snapshot["body"]["status"], "failed",
        "a source smaller than the file-size floor must still be acquired: \
         {snapshot}"
    );
    assert_eq!(
        snapshot["body"]["result"]["manifest"]["bytes_written"], source_bytes,
        "the manifest has to account for the whole source: {snapshot}"
    );
}

/// The same guarantee for EWF, whose per-file bound is the segment size.
///
/// An EWF acquisition rolls segments at `maximum_segment_size`, so that value —
/// not the source length — is the largest file the worker writes. A caller may
/// ask for segments larger than the worker's default 64 MiB floor, and then the
/// default is what kills the run: the segment is still one file, and
/// `RLIMIT_FSIZE` counts bytes per file.
#[test]
fn an_ewf_segment_larger_than_the_default_file_size_limit_is_written() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    // As above: a 66 MiB EWF acquisition is minutes of hashing, so it declares
    // its own worker deadline instead of inheriting the ambient one.
    let mut environment = EnvironmentGuard::new();
    unsafe { environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(600)) };

    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    // Just past the 64 MiB default, and one segment wide, so the whole image is
    // a single file that only the derived budget allows.
    let source_bytes = 66 * 1024 * 1024;
    let file = std::fs::File::create(&source).unwrap();
    file.set_len(source_bytes).unwrap();
    drop(file);

    let destination = directory.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": source,
        "destination_dir": destination,
        "base_name": "copy",
        "format": "ewf1_e01",
        "options": {
            "block_size": 4096,
            "maximum_segment_size": 128 * 1024 * 1024,
            "compression": "none",
            "verify_after": false
        }
    });

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job_within(manager, &job_id, Duration::from_secs(300));
    unsafe { ofi_job_manager_free(manager) };

    assert_ne!(
        snapshot["body"]["status"], "failed",
        "a segment the job asked to be 128 MiB wide must not be cut at the \
         worker's 64 MiB default: {snapshot}"
    );
    assert_eq!(
        snapshot["body"]["result"]["manifest"]["bytes_written"], source_bytes,
        "the manifest has to account for the whole source: {snapshot}"
    );
    let manifest_path = PathBuf::from(
        snapshot["body"]["result"]["manifest_path"]
            .as_str()
            .expect("the acquisition reported a manifest path"),
    );
    let segments: Vec<PathBuf> = std::fs::read_dir(manifest_path.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.to_string_lossy().starts_with('E'))
        })
        .collect();
    assert_eq!(
        segments.len(),
        1,
        "a 66 MiB source in a 128 MiB segment is one segment: {segments:?}"
    );
    let segment_bytes = std::fs::metadata(&segments[0]).unwrap().len();
    assert!(
        segment_bytes > source_bytes,
        "the segment has to hold the source plus the EWF container itself: \
         {segment_bytes} bytes for a {source_bytes} byte source"
    );
    // The bound that matters: libewf keeps each segment under the maximum the
    // job asked for, which is why that value is the budget the worker is given.
    assert!(
        segment_bytes <= 128 * 1024 * 1024,
        "the segment overflowed the maximum it was configured with: {segment_bytes}"
    );
}

/// A physical block device is acquired through the worker, not refused by its
/// filter.
///
/// The worker confines itself with a default-deny seccomp filter, and `ioctl`
/// is deliberately not on the allow-list ("Closed by omission: ... `ioctl`" in
/// `worker-sandbox`). `RawReader::source_length` measured a block device with
/// `ioctl(BLKGETSIZE64)` anyway — and `stat` reports a block device's size as
/// zero, so `metadata.len()` cannot stand in for it. The filter therefore
/// answered that one syscall with `EPERM`, and physical acquisition — ACQ-001,
/// a `Must` — failed at the first read of the device's capacity, before any
/// evidence was read. A regular-file test cannot see this: the ioctl branch is
/// only reached for a block device, and every other acquisition fixture is a
/// file.
///
/// The test runs only where a device is supplied, because a block device needs
/// a privilege the suite does not have:
///
/// ```text
/// OFI_BLOCK_DEVICE_TEST_PATH=/dev/loopN cargo test -p native-bindings \
///     --test worker_isolation block_device_acquisition
/// ```
///
/// Skipping loudly is deliberate: a device path that silently reports success
/// when it never ran is the failure mode this test exists to end.
#[test]
fn a_block_device_is_acquired_through_the_worker_filter() {
    let Some(source) = std::env::var_os("OFI_BLOCK_DEVICE_TEST_PATH").map(PathBuf::from) else {
        eprintln!(
            "skipping a_block_device_is_acquired_through_the_worker_filter: set \
             OFI_BLOCK_DEVICE_TEST_PATH to a readable block device"
        );
        return;
    };

    // The size is read with the same call the worker under test must make, so
    // the assertion below does not restate a number typed into the test.
    let expected_length = {
        use std::os::fd::AsRawFd;
        let file = std::fs::File::open(&source)
            .unwrap_or_else(|error| panic!("cannot open {source:?}: {error}"));
        let length = unsafe { libc::lseek(file.as_raw_fd(), 0, libc::SEEK_END) };
        assert!(length > 0, "device reported a non-positive length: {length}");
        length as u64
    };

    let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut environment = EnvironmentGuard::new();
    // The device is hashed four times over, which is real work even at 16 MiB.
    unsafe { environment.set("OFI_WORKER_TIMEOUT_SECS", scaled_budget(300)) };

    let directory = tempdir().unwrap();
    let destination = directory.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    let request = serde_json::json!({
        "operation": "acquisition",
        "source_path": source,
        "destination_dir": destination,
        "base_name": "device",
        "format": "raw",
        "options": {
            "block_size": 1048576,
            "maximum_segment_size": 134217728,
            "compression": "none",
            "verify_after": false
        }
    });

    let mut manager: *mut ofi_job_manager = ptr::null_mut();
    let mut error = ptr::null_mut();
    assert_eq!(
        unsafe { ofi_job_manager_new(1, &mut manager, &mut error) },
        ofi_status::OFI_STATUS_OK
    );
    let job_id = unsafe { submit(manager, &request) }.unwrap();
    let snapshot = wait_for_job_within(manager, &job_id, Duration::from_secs(300));
    unsafe { ofi_job_manager_free(manager) };

    assert_ne!(
        snapshot["body"]["status"], "failed",
        "a block device must be acquirable: {snapshot}"
    );
    assert_eq!(
        snapshot["body"]["result"]["manifest"]["bytes_written"], expected_length,
        "the image must hold every byte the device reports: {snapshot}"
    );
    let manifest_path = PathBuf::from(
        snapshot["body"]["result"]["manifest_path"]
            .as_str()
            .expect("the acquisition reported a manifest path"),
    );
    let written =
        std::fs::metadata(manifest_path.parent().unwrap().join("device.raw")).unwrap().len();
    assert_eq!(written, expected_length, "the image on disk must be the whole device");
}
