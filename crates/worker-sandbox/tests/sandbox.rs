use worker_sandbox::{
    ResourceLimits, SandboxPolicy, apply_landlock, apply_privilege_drop,
    apply_resource_limits, apply_seccomp_restrictions, landlock_abi_version,
};

#[test]
fn landlock_is_available_on_this_kernel() {
    let abi = landlock_abi_version().unwrap();
    assert!(abi >= 1, "Landlock ABI {abi}");
}

#[test]
fn rejects_empty_read_allowlist() {
    let policy = SandboxPolicy {
        read_paths: Vec::new(),
        write_paths: Vec::new(),
    };
    let error = policy.validate().unwrap_err();
    assert!(error.message().contains("read"));
}

#[test]
fn rejects_duplicate_read_and_write_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_path_buf();
    let policy = SandboxPolicy {
        read_paths: vec![path.clone()],
        write_paths: vec![path],
    };
    let error = policy.validate().unwrap_err();
    assert!(error.message().contains("read and write"));
}

#[test]
fn missing_policy_path_is_rejected_before_restriction() {
    let directory = tempfile::tempdir().unwrap();
    let policy = SandboxPolicy {
        read_paths: vec![directory.path().join("missing")],
        write_paths: Vec::new(),
    };
    let error = policy.validate().unwrap_err();
    assert!(error.message().contains("does not exist"));
}

#[test]
fn sandboxed_child_cannot_read_outside_allowlist_or_open_network() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sandbox_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OFI_SANDBOX_CHILD", "1")
        // LeakSanitizer needs ptrace and fork to scan its child, and this child
        // is confined by exactly the seccomp filter that denies both. Address
        // sanitization still applies; only the leak scan is off in this process.
        .env("ASAN_OPTIONS", "detect_leaks=0")
        .output()
        .unwrap();
    // A pipe rather than the inherited descriptor, for the reason given on
    // `file_size_breach_is_a_typed_error_not_a_signal`: a child that lowers
    // `RLIMIT_FSIZE` must not have its harness writing into a regular file that
    // is already past the limit.
    assert!(
        output.status.success(),
        "the confined child failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn resource_limits_are_applied_before_parsing() {
    // This child carries `parser_default()`'s 64 MiB `RLIMIT_FSIZE`, so an
    // inherited stdout that is a regular file larger than that would trip the
    // limit on the harness's own progress line — the same defect as the 4 KiB
    // sibling, at a larger threshold.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "resource_limit_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OFI_RESOURCE_LIMIT_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the limited child failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn address_space_limit_survives_the_sanitizer_requirement() {
    let limits = ResourceLimits::parser_default();
    if cfg!(ofi_sanitizer) {
        // AddressSanitizer and MemorySanitizer reserve terabytes of shadow
        // memory, so a finite RLIMIT_AS kills the instrumented worker before it
        // parses anything. The cap is still enforced by every normal build.
        assert_eq!(
            limits.address_space_bytes,
            u64::MAX,
            "an instrumented worker cannot run under a finite address-space cap"
        );
    } else {
        assert_eq!(limits.address_space_bytes, 4 * 1024 * 1024 * 1024);
    }
}

/// A write past `RLIMIT_FSIZE` must reach the caller as an error, not as a
/// signal.
///
/// `RLIMIT_FSIZE` is mandated by `SECURITY_MODEL.md` §7, and §9 requires that
/// every limit breach be "a typed error or partial result". The kernel's
/// default disposition for the breach is SIGXFSZ, which terminates the process
/// — a worker killed that way emits no response at all, so the parent reports
/// `worker.crashed` and the partial output is unexplained. Ignoring SIGXFSZ
/// instead makes the same write fail with `EFBIG`, which the acquisition sink
/// already maps to a typed error.
///
/// The child is given pipes, not the inherited file descriptors. It lowers
/// `RLIMIT_FSIZE` to 4 KiB, and an inherited stdout that is a regular file — a
/// `cargo test --workspace > log` redirect, which is exactly how the suite is
/// run — is already past that limit, so the harness's own progress line trips
/// the limit under test before the child reaches its subject write. A pipe is
/// also the shape production uses: the parent spawns the worker with
/// `Stdio::piped()`, and `RLIMIT_FSIZE` does not govern pipes.
#[test]
fn file_size_breach_is_a_typed_error_not_a_signal() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "file_size_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OFI_FILE_SIZE_CHILD", "1")
        .output()
        .unwrap();
    // `ExitStatus` renders a signal as "signal: 25 (SIGXFSZ)" and a panic as an
    // exit code, so the message names what actually happened rather than
    // asserting the mechanism the test exists to rule out.
    assert!(
        output.status.success(),
        "the child did not report EFBIG: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn file_size_child() {
    if std::env::var_os("OFI_FILE_SIZE_CHILD").is_none() {
        return;
    }
    // A budget far below what this child is about to write.
    let mut limits = ResourceLimits::parser_default();
    limits.file_size_bytes = 4096;
    apply_resource_limits(&limits).unwrap();

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("output");
    let error = std::fs::write(&path, vec![0x41_u8; 8192]).unwrap_err();
    assert_eq!(
        error.raw_os_error(),
        Some(libc::EFBIG),
        "expected the breach to surface as EFBIG, got {error:?}"
    );
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 4096);
}

#[test]
fn resource_limit_child() {
    if std::env::var_os("OFI_RESOURCE_LIMIT_CHILD").is_none() {
        return;
    }
    let limits = ResourceLimits::parser_default();
    apply_resource_limits(&limits).unwrap();

    let mut core = libc::rlimit {
        rlim_cur: 1,
        rlim_max: 1,
    };
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut core) }, 0);
    assert_eq!(core.rlim_cur, 0);

    let mut files = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut files) },
        0
    );
    assert_eq!(files.rlim_cur, limits.open_files);
}

#[test]
fn sandbox_child() {
    if std::env::var_os("OFI_SANDBOX_CHILD").is_none() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let allowed = directory.path().join("allowed");
    let denied = directory.path().join("denied");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&denied).unwrap();
    std::fs::write(allowed.join("visible"), b"ok").unwrap();
    std::fs::write(denied.join("secret"), b"no").unwrap();

    let policy = SandboxPolicy {
        read_paths: vec![allowed.clone()],
        write_paths: Vec::new(),
    };
    apply_landlock(&policy).unwrap();
    apply_seccomp_restrictions().unwrap();

    assert_eq!(std::fs::read(allowed.join("visible")).unwrap(), b"ok");
    let read_error = std::fs::read(denied.join("secret")).unwrap_err();
    assert_eq!(read_error.kind(), std::io::ErrorKind::PermissionDenied);
    let write_error = std::fs::write(allowed.join("new"), b"x").unwrap_err();
    assert_eq!(write_error.kind(), std::io::ErrorKind::PermissionDenied);

    let socket_result =
        unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
    assert_eq!(socket_result, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );

    let fork_result = unsafe { libc::fork() };
    assert_eq!(fork_result, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );

    // setpriority is not named by the deny-list, so only a default-deny filter
    // refuses it. A deny-list permits this, which is the difference the test
    // exists to detect.
    let priority_result =
        unsafe { libc::syscall(libc::SYS_setpriority, libc::PRIO_PROCESS, 0, 0) };
    assert_eq!(priority_result, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );
}

/// Verify that the worker drops privilege to a dedicated low-privilege user.
///
/// The worker must run as `ofi-worker` (UID/GID from `/etc/passwd`), not as the
/// parent's UID. This test spawns a sandboxed child, applies the privilege drop,
/// and checks that the child's `getuid()` is different from the parent's.
///
/// If the `ofi-worker` user does not exist, the fallback is UID 65534 (nobody),
/// which this test also accepts as long as it's not the parent UID.
#[test]
fn worker_drops_privilege() {
    let parent_uid = unsafe { libc::getuid() };

    // Spawn a child to apply the sandbox + privilege drop
    match unsafe { libc::fork() } {
        -1 => panic!("fork failed: {}", std::io::Error::last_os_error()),
        0 => {
            // Child: apply Landlock (minimal policy), then privilege drop
            let policy = SandboxPolicy {
                read_paths: vec![],
                write_paths: vec![],
            };
            apply_landlock(&policy).expect("landlock failed in child");

            // Get ofi-worker UID/GID (or fallback to 65534)
            let target_uid = ofi_worker_uid();
            let target_gid = ofi_worker_gid();

            apply_privilege_drop(target_uid, target_gid)
                .expect("privilege drop failed in child");

            // Check that we are no longer the parent UID
            let child_uid = unsafe { libc::getuid() };
            assert_ne!(
                child_uid, parent_uid,
                "child UID {child_uid} == parent UID {parent_uid}, privilege drop did not work"
            );
            assert_eq!(
                child_uid, target_uid,
                "child UID {child_uid} != target UID {target_uid}"
            );

            // Exit cleanly (not with panic, which would confuse the test harness)
            unsafe { libc::_exit(0) };
        }
        child_pid => {
            // Parent: wait for child
            let mut status: libc::c_int = 0;
            let wait_result = unsafe { libc::waitpid(child_pid, &mut status, 0) };
            assert_eq!(wait_result, child_pid, "waitpid failed");
            assert!(
                libc::WIFEXITED(status),
                "child did not exit normally (status {status})"
            );
            assert_eq!(
                libc::WEXITSTATUS(status),
                0,
                "child exited with status {}",
                libc::WEXITSTATUS(status)
            );
        }
    }
}

/// Helper to get ofi-worker UID (same logic as parser_worker.rs).
fn ofi_worker_uid() -> u32 {
    let name = std::ffi::CString::new("ofi-worker").unwrap();
    let pwd = unsafe { libc::getpwnam(name.as_ptr()) };
    if pwd.is_null() {
        65534 // fallback to nobody
    } else {
        unsafe { (*pwd).pw_uid }
    }
}

/// Helper to get ofi-worker GID (same logic as parser_worker.rs).
fn ofi_worker_gid() -> u32 {
    let name = std::ffi::CString::new("ofi-worker").unwrap();
    let pwd = unsafe { libc::getpwnam(name.as_ptr()) };
    if pwd.is_null() {
        65534 // fallback to nogroup
    } else {
        unsafe { (*pwd).pw_gid }
    }
}
