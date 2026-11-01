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
| Resource limits | `worker_sandbox::ResourceLimits` | `RLIMIT_CORE` 0, `RLIMIT_AS` 4 GiB, `RLIMIT_CPU` 1 h, `RLIMIT_NOFILE` 256, `RLIMIT_FSIZE` per job (`file_size_limits`), 64 MiB floor; `SIGXFSZ` ignored so a breach is `EFBIG`, not a kill |
| Environment allowlist | `WORKER_ENVIRONMENT_ALLOWLIST` | `LANG`, `LC_ALL`, `LC_CTYPE`, `TZ`, `TZDIR`, `LIBEWF_VERBOSE` only |
| Path allowlist | `worker_sandbox::apply_landlock` | Read roots from the evidence path only — each EWF segment named individually; write roots from the request's own output fields |
| Syscall filter | `worker_sandbox::apply_seccomp_restrictions` | `SYS_seccomp` with `SECCOMP_FILTER_FLAG_TSYNC`; default-deny allow-list, so network, process creation, execution, ptrace, and any unlisted syscall get `EPERM` |
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
   one. The four options for changing that were assessed in
   [worker-privilege-boundary.md](worker-privilege-boundary.md), which
   recommends a dedicated system user scoped to the DEB. That recommendation is
   **not implemented** — this risk stands, and the release gate stays open.
2. **The response cap is enforced by the parent.** `read_bounded` stops at the
   first byte past `MAX_RESPONSE_BYTES` and the poll loop kills the worker group
   immediately, so a flooding worker cannot be drained to EOF or to the
   wall-clock deadline. The cap does not depend on `RLIMIT_FSIZE`: the response
   travels over a pipe, and `RLIMIT_FSIZE` does not apply to pipes.
   `RLIMIT_FSIZE` bounds the files the worker writes — the evidence — and its
   budget follows the job: `native_bindings` sizes it from the acquisition's
   source length or EWF segment size, or the extraction file cap, before the
   limits are applied.
3. **The seccomp policy is now a default-deny allow-list.** The filter permits
   only the syscalls the worker was observed to issue and refuses everything
   else with `EPERM`, so a syscall added by a future kernel is denied rather
   than permitted. The list was derived from `strace -ff -e trace=all` over
   `cargo test -p native-bindings -p extraction -p acquisition`, restricted to
   the post-`execve` stream of each `ofi-parser-worker` process, then reviewed
   against the source for calls reachable only on paths no test takes:
   `unlinkat` is the cleanup after a failed extraction hash comparison
   (`crates/extraction/src/lib.rs:569`) and `clock_gettime` is served from the
   vDSO, so neither appears in the trace. Two gaps found this way are recorded
   in the justification comment: denying `exit` made glibc spin retrying the
   failure instead of terminating, and `madvise` is the allocator returning
   pages. The residual limitation is narrower than the deny-list it replaced: a
   worker capability that no test and no reviewed code path exercises would
   fail with `EPERM` in production rather than being silently permitted.
   `crates/worker-sandbox/tests/sandbox.rs` pins the difference by requiring
   `EPERM` from `setpriority`, which no deny-list would name.
4. **The EWF read grant covers the image's segments, not its directory.**
   `EwfReader::open` is handed only the first segment and discovers the rest
   itself, so the sandbox used to grant the whole parent directory to make the
   siblings reachable — and a compromised worker could therefore read every
   unrelated file beside the evidence. `libewf_bindings::segment_paths` now
   enumerates the image up front (exact stem match, `E01..E99` then `EAA..`,
   any case, regular files only, so a symlink named like a segment cannot pull
   its target into the grant), and `sandbox_policy` grants each segment
   individually: Landlock `PATH_BENEATH` accepts a regular file.
   `crates/native-bindings/tests/worker_isolation.rs` pins both halves — the
   job still succeeds on a multi-segment image, and the policy names the
   segments without naming the directory. Two decisions this depends on:
   whether a path is EWF is decided by **file signature**, the same way
   `open_image` decides it, rather than by extension, so a raw file named
   `.E01` is granted as itself instead of sweeping up every `.E02` beside it;
   and an image whose segments cannot be enumerated at all is a hard
   `worker.sandbox` failure rather than a job run under a policy the parse is
   certain to fail against. Narrowing does not make an incomplete image look
   complete: with a later segment deleted the job still reports `partial`,
   naming every unreadable byte range, and never `succeeded` — the same
   result-honesty rule the rest of the acquisition path follows. The residual
   is that the grant is computed once, before the parse, so segments present
   when the policy is built are readable for the job's duration even after the
   parse stops needing them.
5. **Cancellation is delivered by SIGKILL, and staging orphans are now reaped.**
   The in-worker cancellation token is a fresh `CancellationToken::default()`,
   so `acquire_reader`'s cooperative check never fires. A cancelled acquisition
   reports its retained staging directory (ACQ-011), and since the orphan cannot
   clean itself up, `acquire_reader` now reaps on the way in: every staging
   directory carries a `.ofi-staging.json` marker with its `job_id`, `pid`, and
   start time, and `acquisition::reap_stale_staging` removes a
   `.<base>.partial-<uuid>` directory only when its marker's process is gone
   *and* the directory is older than `DEFAULT_STAGING_MAX_AGE` (24 h). A marker
   naming a live process is kept regardless of age, so a long acquisition is
   never reaped out from under itself; a directory with no readable marker is
   reaped by age alone, or an interrupted run that died before writing its
   marker would leak forever. Only directories whose names carry a parseable
   UUID suffix are considered, so a dotfile directory that merely resembles
   staging output is left alone. Reaping never fails an acquisition: a scan that
   cannot run is recorded as `staging_reap_warning`, and the count removed is
   `staging_reaped`. The residual limitation is that a job cancelled inside the
   24 h window still occupies its retained bytes until the age passes — that
   window is what makes ACQ-011 retention meaningful.
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
