use worker_sandbox::{
    SandboxPolicy, apply_landlock, apply_seccomp_restrictions, landlock_abi_version,
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
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sandbox_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OFI_SANDBOX_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success());
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
}
