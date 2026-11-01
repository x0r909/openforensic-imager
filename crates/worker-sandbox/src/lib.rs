use std::ffi::CString;
use std::fmt;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
const LANDLOCK_ACCESS_FS_REFER: u64 = 1 << 13;
const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14;
const LANDLOCK_ACCESS_FS_IOCTL_DEV: u64 = 1 << 15;

const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
const SECCOMP_SET_MODE_FILTER: libc::c_uint = 1;
const SECCOMP_FILTER_FLAG_TSYNC: libc::c_ulong = 1;
const AUDIT_ARCH_X86_64: u32 = 0xC000_003E;
const BPF_LD: u16 = 0x00;
const BPF_W: u16 = 0x00;
const BPF_ABS: u16 = 0x20;
const BPF_JMP: u16 = 0x05;
const BPF_JEQ: u16 = 0x10;
const BPF_K: u16 = 0x00;
const BPF_RET: u16 = 0x06;

/// The complete set of system calls the parser worker may issue.
///
/// Derived from `strace -ff -e trace=all` over
/// `cargo test -p native-bindings -p extraction -p acquisition`, restricted to
/// the post-`execve` stream of each `ofi-parser-worker` process, then reviewed
/// against the source for calls reachable only on paths the tests do not take.
/// Anything absent here is refused with `EPERM`, so a syscall added by a future
/// kernel, or by a compromised parser, is denied by default.
///
/// Justification, one line per family:
///
/// - Process lifecycle: `exit`, `exit_group`. The worker answers one request
///   and exits. `clone`, `clone3`, `fork`, `vfork`, `execve`, `execveat` stay
///   out; the worker runs one request per process and must never spawn or exec.
///   `exit` is not optional: with it denied, glibc spins retrying the failure
///   instead of terminating, which is how this gap was found.
/// - Memory: `brk`, `madvise`, `mmap`, `mprotect`, `munmap`. The allocator, the
///   sanitizer runtime, and the parsers' own buffers. `madvise` is how the
///   allocator returns pages to the kernel.
/// - File inspection and read: `openat`, `openat2`, `read`, `pread64`,
///   `lseek`, `close`, `fcntl`, `fstat`, `newfstatat`, `statx`, `access`,
///   `readlink`, `getdents64`, `poll`. Reading evidence and listing a filesystem
///   image. Every one of these is still confined by Landlock and by the
///   `O_RDONLY | O_CLOEXEC | O_NOFOLLOW` flags the image reader opens with.
/// - File creation and rename: `write`, `fsync`, `mkdir`, `mkdirat`, `rename`,
///   `rmdir`, `unlinkat`. Staging an acquisition, writing an extraction, and
///   separating a replaced target. `unlinkat` is not reached by any test — it
///   is the cleanup on an extraction whose hash or size did not match — so it
///   comes from the source at `crates/extraction/src/lib.rs:569`, not the trace.
/// - Signalling: `rt_sigaction`, `rt_sigprocmask`, `sigaltstack`. libc and the
///   Rust runtime install handlers before `main`.
/// - Time: `clock_gettime`. Tracing, timeouts, and `Instant`. Served from the
///   vDSO on this host, which is why the trace does not show it.
/// - Identity: `geteuid`, `gettid`. Ownership checks on output directories.
/// - Sandbox installation: `prctl`, `seccomp`, `landlock_create_ruleset`,
///   `landlock_add_rule`, `landlock_restrict_self`. The worker installs the
///   filter on itself; ordering keeps these before the enclosing restrictions.
/// - Runtime bookkeeping: `arch_prctl`, `rseq`, `set_robust_list`,
///   `set_tid_address`, `getrandom`, `futex`, `prlimit64`,
///   `sched_getaffinity`. glibc and the allocator at start-up.
/// - Closed by omission: the socket family, `ptrace`, `process_vm_readv`,
///   `process_vm_writev`, `kill`, `ioctl`, and every `*at` variant that would
///   escape the Landlock root.
const ALLOWED_SYSCALLS: &[libc::c_long] = &[
    // Process lifecycle. `exit` is the single-thread path; denying it makes
    // glibc retry it in a spin loop instead of terminating.
    libc::SYS_exit,
    libc::SYS_exit_group,
    // Memory.
    libc::SYS_brk,
    libc::SYS_madvise,
    libc::SYS_mmap,
    libc::SYS_mprotect,
    libc::SYS_munmap,
    // File inspection and read.
    libc::SYS_openat,
    libc::SYS_openat2,
    libc::SYS_read,
    libc::SYS_pread64,
    libc::SYS_lseek,
    libc::SYS_close,
    libc::SYS_fcntl,
    libc::SYS_fstat,
    libc::SYS_newfstatat,
    libc::SYS_statx,
    libc::SYS_access,
    libc::SYS_readlink,
    libc::SYS_getdents64,
    libc::SYS_poll,
    // File creation, rename, and cleanup.
    libc::SYS_write,
    libc::SYS_fsync,
    libc::SYS_mkdir,
    libc::SYS_mkdirat,
    libc::SYS_rename,
    libc::SYS_rmdir,
    libc::SYS_unlinkat,
    // Signalling.
    libc::SYS_rt_sigaction,
    libc::SYS_rt_sigprocmask,
    libc::SYS_sigaltstack,
    // Time.
    libc::SYS_clock_gettime,
    // Identity.
    libc::SYS_geteuid,
    libc::SYS_gettid,
    // Sandbox installation.
    libc::SYS_prctl,
    libc::SYS_seccomp,
    libc::SYS_landlock_create_ruleset,
    libc::SYS_landlock_add_rule,
    libc::SYS_landlock_restrict_self,
    // Runtime bookkeeping.
    libc::SYS_arch_prctl,
    libc::SYS_rseq,
    libc::SYS_set_robust_list,
    libc::SYS_set_tid_address,
    libc::SYS_getrandom,
    libc::SYS_futex,
    libc::SYS_prlimit64,
    libc::SYS_sched_getaffinity,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub read_paths: Vec<PathBuf>,
    pub write_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    pub address_space_bytes: u64,
    pub cpu_seconds: u64,
    pub open_files: u64,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxError {
    message: String,
}

impl SandboxError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SandboxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SandboxError {}

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

impl SandboxPolicy {
    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.read_paths.is_empty() {
            return Err(SandboxError::new("sandbox requires at least one read path"));
        }
        for path in self.read_paths.iter().chain(self.write_paths.iter()) {
            if !path.exists() {
                return Err(SandboxError::new(format!(
                    "sandbox path does not exist: {}",
                    path.display()
                )));
            }
        }
        for write in &self.write_paths {
            if self.read_paths.iter().any(|read| read == write) {
                return Err(SandboxError::new(format!(
                    "path cannot be both read and write allowlisted: {}",
                    write.display()
                )));
            }
        }
        Ok(())
    }
}

impl ResourceLimits {
    pub fn parser_default() -> Self {
        Self {
            // AddressSanitizer and MemorySanitizer reserve terabytes of shadow
            // memory, so a finite RLIMIT_AS kills an instrumented worker before
            // it parses anything. scripts/run-sanitizers.sh sets this cfg; every
            // normal build keeps the 4 GiB cap. `u64::MAX` is `RLIM_INFINITY`.
            address_space_bytes: if cfg!(ofi_sanitizer) {
                u64::MAX
            } else {
                4 * 1024 * 1024 * 1024
            },
            cpu_seconds: 60 * 60,
            open_files: 256,
            // A floor, not the whole policy: `RLIMIT_FSIZE` caps one file the
            // worker writes, and the worker writes evidence — a raw image is as
            // large as its source and an EWF segment as large as the segment
            // size the job asked for. `parser_default` has no request to read
            // those from, so it carries a budget only the small writers stay
            // under. native-bindings derives the real per-job budget and passes
            // it to `apply_resource_limits`; see `file_size_limits`.
            //
            // It does *not* bound the worker's response. The response travels
            // over a pipe, and `RLIMIT_FSIZE` does not apply to pipes. The
            // parent enforces `MAX_RESPONSE_BYTES` on its own, by reading at
            // most that much and killing the worker past it.
            file_size_bytes: 64 * 1024 * 1024,
        }
    }

    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.address_space_bytes == 0
            || self.cpu_seconds == 0
            || self.open_files == 0
            || self.file_size_bytes == 0
        {
            return Err(SandboxError::new("resource limits must all be non-zero"));
        }
        Ok(())
    }
}

pub fn apply_resource_limits(limits: &ResourceLimits) -> Result<(), SandboxError> {
    limits.validate()?;
    set_limit(libc::RLIMIT_CORE, 0, 0, "RLIMIT_CORE")?;
    set_limit(
        libc::RLIMIT_AS,
        limits.address_space_bytes,
        limits.address_space_bytes,
        "RLIMIT_AS",
    )?;
    set_limit(
        libc::RLIMIT_CPU,
        limits.cpu_seconds,
        limits.cpu_seconds,
        "RLIMIT_CPU",
    )?;
    set_limit(
        libc::RLIMIT_NOFILE,
        limits.open_files,
        limits.open_files,
        "RLIMIT_NOFILE",
    )?;
    set_limit(
        libc::RLIMIT_FSIZE,
        limits.file_size_bytes,
        limits.file_size_bytes,
        "RLIMIT_FSIZE",
    )?;
    ignore_file_size_signal()?;
    Ok(())
}

/// Makes a write past `RLIMIT_FSIZE` fail with `EFBIG` instead of killing the
/// worker with `SIGXFSZ`.
///
/// The kernel raises `SIGXFSZ` on the write that crosses the limit, and its
/// default disposition terminates the process. A worker killed that way reports
/// nothing at all, so the parent can only describe it as a crash — which is not
/// the "typed error or partial result" SECURITY_MODEL.md §9 requires of every
/// limit breach, and which leaves the partial output on disk unexplained. With
/// the signal ignored the same write returns `EFBIG` ("File too large") to the
/// caller, where the writers that can reach the limit already turn an io error
/// into a typed failure. The bytes written before the limit are kept.
///
/// Ignoring the disposition is safe here because the worker executes nothing:
/// it is the last process in its job, so an inherited disposition reaches no
/// other program.
fn ignore_file_size_signal() -> Result<(), SandboxError> {
    // SAFETY: `sigaction` is a plain aggregate of integers and a handler
    // address; a zeroed one with `SIG_IGN` is a valid disposition, and the old
    // disposition is not read back.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = libc::SIG_IGN;
    if unsafe { libc::sigaction(libc::SIGXFSZ, &action, std::ptr::null_mut()) } != 0 {
        return Err(last_error("sigaction(SIGXFSZ) failed"));
    }
    Ok(())
}

pub fn landlock_abi_version() -> Result<i64, SandboxError> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0_usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if result < 0 {
        return Err(last_error("Landlock is unavailable"));
    }
    Ok(result as i64)
}

pub fn apply_landlock(policy: &SandboxPolicy) -> Result<(), SandboxError> {
    policy.validate()?;
    let abi = landlock_abi_version()?;
    let handled = handled_access(abi);
    let attr = RulesetAttr {
        handled_access_fs: handled,
    };
    let ruleset = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attr,
            std::mem::size_of::<RulesetAttr>(),
            0_u32,
        )
    };
    if ruleset < 0 {
        return Err(last_error("landlock_create_ruleset failed"));
    }
    let ruleset = unsafe { OwnedFd::from_raw_fd(ruleset as i32) };

    let read_access = LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR;
    let mut write_access = read_access
        | LANDLOCK_ACCESS_FS_WRITE_FILE
        | LANDLOCK_ACCESS_FS_MAKE_REG
        | LANDLOCK_ACCESS_FS_MAKE_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_FILE
        | LANDLOCK_ACCESS_FS_REMOVE_DIR;
    if abi >= 2 {
        write_access |= LANDLOCK_ACCESS_FS_REFER;
    }
    if abi >= 3 {
        write_access |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }

    for path in &policy.read_paths {
        add_path_rule(&ruleset, path, read_access & handled)?;
    }
    for path in &policy.write_paths {
        add_path_rule(&ruleset, path, write_access & handled)?;
    }

    if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(last_error("prctl(PR_SET_NO_NEW_PRIVS) failed"));
    }
    let result =
        unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0_u32) };
    if result != 0 {
        return Err(last_error("landlock_restrict_self failed"));
    }
    Ok(())
}

pub fn apply_seccomp_restrictions() -> Result<(), SandboxError> {
    let mut filter = Vec::with_capacity(ALLOWED_SYSCALLS.len() * 2 + 6);
    filter.push(statement(BPF_LD | BPF_W | BPF_ABS, 4));
    filter.push(jump(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH_X86_64, 1, 0));
    filter.push(statement(BPF_RET | BPF_K, libc::SECCOMP_RET_KILL_PROCESS));
    filter.push(statement(BPF_LD | BPF_W | BPF_ABS, 0));
    for syscall in ALLOWED_SYSCALLS {
        filter.push(jump(BPF_JMP | BPF_JEQ | BPF_K, *syscall as u32, 0, 1));
        filter.push(statement(BPF_RET | BPF_K, libc::SECCOMP_RET_ALLOW));
    }
    // Default deny: a syscall outside the allow-list gets EPERM. A future
    // kernel's new syscall lands here rather than being permitted, which is the
    // difference between this filter and the deny-list it replaced.
    filter.push(statement(
        BPF_RET | BPF_K,
        libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
    ));
    let mut program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };

    if unsafe { libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(last_error("prctl(PR_SET_NO_NEW_PRIVS) failed"));
    }
    let result = unsafe {
        libc::syscall(
            libc::SYS_seccomp,
            SECCOMP_SET_MODE_FILTER,
            SECCOMP_FILTER_FLAG_TSYNC,
            &mut program as *mut libc::sock_fprog,
        )
    };
    if result < 0 {
        return Err(last_error("seccomp filter installation failed"));
    }
    Ok(())
}

fn set_limit(
    resource: libc::__rlimit_resource_t,
    soft: u64,
    hard: u64,
    name: &str,
) -> Result<(), SandboxError> {
    let limit = libc::rlimit {
        rlim_cur: soft as libc::rlim_t,
        rlim_max: hard as libc::rlim_t,
    };
    if unsafe { libc::setrlimit(resource, &limit) } != 0 {
        return Err(last_error(&format!("setrlimit({name}) failed")));
    }
    Ok(())
}

fn add_path_rule(ruleset: &OwnedFd, path: &Path, allowed_access: u64) -> Result<(), SandboxError> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| SandboxError::new(format!("unable to inspect sandbox path: {error}")))?;
    let allowed_access = if metadata.is_dir() {
        allowed_access
    } else {
        allowed_access
            & (LANDLOCK_ACCESS_FS_EXECUTE
                | LANDLOCK_ACCESS_FS_WRITE_FILE
                | LANDLOCK_ACCESS_FS_READ_FILE
                | LANDLOCK_ACCESS_FS_TRUNCATE
                | LANDLOCK_ACCESS_FS_IOCTL_DEV)
    };
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| SandboxError::new("sandbox path contains a NUL byte"))?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_PATH | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(last_error("unable to open sandbox policy path"));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let attr = PathBeneathAttr {
        allowed_access,
        parent_fd: fd.as_raw_fd(),
    };
    let result = unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            ruleset.as_raw_fd(),
            LANDLOCK_RULE_PATH_BENEATH,
            &attr,
            0_u32,
        )
    };
    if result != 0 {
        return Err(last_error("landlock_add_rule failed"));
    }
    Ok(())
}

fn handled_access(abi: i64) -> u64 {
    let mut access = LANDLOCK_ACCESS_FS_EXECUTE
        | LANDLOCK_ACCESS_FS_WRITE_FILE
        | LANDLOCK_ACCESS_FS_READ_FILE
        | LANDLOCK_ACCESS_FS_READ_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_DIR
        | LANDLOCK_ACCESS_FS_REMOVE_FILE
        | LANDLOCK_ACCESS_FS_MAKE_CHAR
        | LANDLOCK_ACCESS_FS_MAKE_DIR
        | LANDLOCK_ACCESS_FS_MAKE_REG
        | LANDLOCK_ACCESS_FS_MAKE_SOCK
        | LANDLOCK_ACCESS_FS_MAKE_FIFO
        | LANDLOCK_ACCESS_FS_MAKE_BLOCK
        | LANDLOCK_ACCESS_FS_MAKE_SYM;
    if abi >= 2 {
        access |= LANDLOCK_ACCESS_FS_REFER;
    }
    if abi >= 3 {
        access |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
    if abi >= 5 {
        access |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
    access
}

fn statement(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

fn last_error(context: &str) -> SandboxError {
    SandboxError::new(format!("{context}: {}", std::io::Error::last_os_error()))
}
