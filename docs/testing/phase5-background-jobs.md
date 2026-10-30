# Phase 5 Background Job Infrastructure

Date: 2026-10-30

## Implemented scope

- Tokio-backed blocking worker pool for acquisition, analysis, hashing, and
  extraction operations.
- Typed pending, running, succeeded, partial, failed, and cancelled states.
- Progress snapshots, result payloads, error codes, and UTC timestamps.
- Shared cancellation token from `forensic-core`.
- Cancellation propagation into the existing acquisition cancellation path.
- Panic containment so a failed worker cannot terminate the desktop process.
- Blocking wait with a bounded timeout for tests and service shutdown.
- C ABI 1.1 manager lifecycle, submission, snapshot, and cancellation calls.
- C ABI job dispatch for partition analysis, filesystem listing, verification,
  and acquisition.
- C ABI job dispatch for bounded hex/text preview, signature identification, and
  single-file extraction.
- C ABI directory extraction with a partial/success manifest and JSON, CSV, and
  HTML extraction reports.

## Verification evidence

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
CARGO_TARGET_DIR=target/verify-tests cargo test --workspace --no-run --message-format=json
```

Every emitted test executable was run with `--test-threads=1`.

Result: 39 test executables, 118 tests passed, 0 failed.

The C ABI smoke harness also compiles with `-Wall -Wextra -Werror` and exits 0
after creating and releasing a job manager.

## Limitations

- The Qt shell and QML models are not implemented yet.
- Jobs are in-process. The architecture's crash-isolated analysis worker and
  worker RPC remain Phase 6 work.
- Completed job records are retained for the process lifetime. Persistence,
  retention, and case-database integration remain future work.
- The C ABI job dispatcher does not yet expose acquisition/verification report
  generation, evidence inventory, or examination-history reports.
