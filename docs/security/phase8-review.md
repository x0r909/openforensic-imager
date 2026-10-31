# Phase 8 Security Review

Status: parser isolation wired and exercised; the release security gate remains
open for the reasons listed under "Residual risks".

This document records the evidence behind the Phase 8 boundary and the residual
risks that remain after it. It is the reference used by
[phase6-review.md](phase6-review.md) for the P6-SEC-001 resolution.

## What Phase 8 changed

Every evidence-facing operation now executes in `ofi-parser-worker` instead of
the desktop process:

- image identification and metadata
- partition analysis
- filesystem open and directory listing
- acquisition and verification
- hex and text preview, signature identification
- single-file and directory extraction

The trusted process keeps only the operations that do not parse evidence
content: `evidence_list`, `audit_append`, and `audit_verify`. The worker refuses
any of those three with `worker.unsupported_operation`, so the direction of the
boundary fails closed in both directions.

## Controls on the worker boundary

| Control | Location | Behaviour |
|---|---|---|
| Process separation | `crates/native-bindings/src/parser_worker.rs` | One request per worker process; the worker exits after answering |
| Request bound | `MAX_REQUEST_BYTES` (1 MiB) | Enforced at read time; oversize input is rejected before parsing |
| Response bound | `MAX_RESPONSE_BYTES` (64 MiB) | Retained bytes are capped and a truncated response is a total failure |
| Diagnostic bound | `MAX_STDERR_BYTES` (64 KiB) | Bounded stderr capture, surfaced only on `worker.crashed` |
| Wall-clock watchdog | `DEFAULT_TIMEOUT_SECONDS` (1 h), `MAX_TIMEOUT_SECONDS` (24 h) | The worker process group is killed on expiry |
| Environment | `WORKER_ENVIRONMENT_ALLOWLIST` | `env_clear()` plus a small allowlist; no inherited credentials |
| Resource limits | `worker_sandbox::ResourceLimits` | `RLIMIT_CORE` 0, `RLIMIT_AS` 4 GiB, `RLIMIT_CPU` 1 h, `RLIMIT_NOFILE` 256, `RLIMIT_FSIZE` 64 MiB |
| Environment allowlist | `WORKER_ENVIRONMENT_ALLOWLIST` | `LANG`, `LC_ALL`, `LC_CTYPE`, `TZ`, `TZDIR`, `LIBEWF_VERBOSE` only |
| Path allowlist | `worker_sandbox::apply_landlock` | Read roots from the evidence path only; write roots from the request's own output fields |
| Syscall filter | `worker_sandbox::apply_seccomp_restrictions` | `SYS_seccomp` with `SECCOMP_FILTER_FLAG_TSYNC`; denies network, process creation, execution, ptrace |
| Privilege escalation | `PR_SET_NO_NEW_PRIVS` | Set before Landlock and seccomp are installed |
| Crash containment | `catch_unwind` in the worker | A panic becomes a `worker.panic` response, not a lost job |

Any sandbox step that fails aborts the job with `worker.sandbox` before evidence
is parsed. There is no silent reduced-assurance path.

## Verified by test

- `crates/worker-sandbox/tests/sandbox.rs` — a sandboxed child cannot read
  outside its allowlist, cannot write inside a read-only grant, gets `EPERM`
  for `socket()`, and gets `EPERM` for `fork()`.
- `crates/native-bindings/tests/worker_isolation.rs` — a crashing worker is
  contained and a new worker still runs; the worker does not inherit parent
  environment variables outside the allowlist.
- `crates/native-bindings/tests/worker_timeout.rs` — a hung worker is killed at
  the watchdog deadline.
- `crates/native-bindings/tests/worker_cancellation.rs` — a cancelled job
  terminates the worker process group and reports `cancelled`.
- `crates/native-bindings/tests/abi.rs` — EWF acquisition and verification run
  through the parser worker, and job results survive the round trip.

## Residual risks

1. **The worker is not a privilege boundary.** It runs as the examiner's Unix
   user. A kernel or sandbox escape, or a defect in a native parser that yields
   arbitrary code execution, compromises that user's authority, not a lesser
   one.
2. **The response cap is enforced from both sides.** `read_bounded` stops at the
   first byte past `MAX_RESPONSE_BYTES` and the poll loop kills the worker group
   immediately, so a flooding worker cannot be drained to EOF or to the
   wall-clock deadline. The worker's `RLIMIT_FSIZE` matches the same 64 MiB
   budget, so it cannot emit more than the parent accepts.
3. **The seccomp policy is a deny-list.** A syscall added to a future kernel is
   permitted by default, which is weaker than the allow-list the security model
   describes.
4. **The EWF read grant covers the image directory.** Sibling segments must be
   openable, so a compromised worker can read unrelated files in that
   directory.
5. **Cancellation is delivered by SIGKILL.** The in-worker cancellation token is
   a fresh `CancellationToken::default()`, so `acquire_reader`'s cooperative
   check never fires. A cancelled acquisition does report its retained staging
   directory (ACQ-011), but there is no reaper or age marker for orphaned
   `.<base>.partial-<job_id>` directories, so a cancel/retry loop slowly fills
   the destination volume.
6. **Worker identity is fixable only in debug builds.** `OFI_PARSER_WORKER` is
   honoured only under `cfg!(debug_assertions)`; a release build always uses the
   beside-application binary. The environment is still cleared before spawn.
7. **Sanitizer and long-duration fuzz coverage are not run on this host.** No
   ASan, MSan, or scheduled fuzz budget is claimed.

## Release gate decision

Phase 8 closes the parser-dispatch portion of P6-SEC-001. The release security
gate is **not** fully passed: sanitizer coverage is unavailable, the worker is
not a privilege boundary, and the residual risks above are accepted limitations
rather than fixes.

See [phase6-review.md](phase6-review.md) for the full finding list.
