# Phase 8 Validation Results

Status: parser isolation wired, exercised, and hardened against the audit
findings recorded in `docs/security/phase8-review.md`.

## Test commands

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cmake -G Ninja -S apps/desktop -B build/desktop
cmake --build build/desktop
ctest --test-dir build/desktop --output-on-failure
```

Ephemeral verification on this host:

- `cargo test --workspace` — 0 failures across repeated runs.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `ctest` — 4/4 suites pass (`openforensic-imager-tests`,
  `openforensic-imager-self-test`, `-self-test-dark`, `-qml-tests`).

## Regression tests added for the audit findings

| Finding | Test |
|---|---|
| Audit append after commit | `audit_append_failure_is_recovered_from_the_durable_outbox` |
| Forged or truncated audit tail | `verified_chain_head_is_required_before_appending` |
| Audit path not bound to case | `audit_operations_are_bound_to_the_case` |
| Omitted `audit_path` skipped the audit | `omitted_audit_path_still_audits_beside_the_database`, `case_and_evidence_requests_return_owned_json` |
| Audit failure blocked the commit | `case_create_with_unopenable_audit_path_refuses_to_commit` |
| Verification compared nothing | `verify_image` assertions in `crates/acquisition/tests/acquire.rs` |
| Evidence text rendered as rich text | `evidenceTextBindingsArePlainText` in `apps/desktop/tests/tst_models.cpp` |
| Planted symlink redirected a report | `report_write_refuses_a_planted_symlink_leaf` |
| Worker inherited the environment | `worker_does_not_inherit_the_parent_environment` |
| Cancelled acquisition lost its retained output | `cancelled_job_keeps_its_retained_result`, `cancelled_acquisition_reports_its_retained_staging_directory` |
| Response cap drained to EOF | `oversized_worker_response_is_stopped_at_the_cap` |
| Report directory was a symlink | `report_write_refuses_a_symlinked_output_directory` |
| Chain rolled back behind the recorded head | `audit_verify_detects_a_chain_behind_the_recorded_head` |
| Schema upgrade to the audit binding | `version_one_database_upgrades_to_the_audit_binding_schema` |

Each was confirmed to fail against the pre-fix source before being accepted.

## Behaviour changes

- `audit_path` is optional at the ABI but never skips the audit: it defaults to
  `audit.jsonl` beside the case database.
- `verify_image` returns `VerificationNotPerformed` unless every requested
  algorithm matched. `verify_image_report` preserves per-algorithm outcomes for
  report rendering.
- Output directories must be owned by the current user and not group- or
  other-writable.
- Report leaves are opened with `O_NOFOLLOW | O_NONBLOCK`; a symlinked reports
  directory is refused.
- A cancelled acquisition reports `staging_path` and `retained` instead of
  discarding where its partial output went.
- `OFI_PARSER_WORKER` is honoured only in debug builds.
- `RLIMIT_FSIZE` for the worker is 64 MiB, matching the response cap.
- The ABI minor version is now 2.

## Not covered

- No ASan, MSan, or scheduled fuzz run: this host has no nightly toolchain.
- No multi-terabyte or long-duration soak test.
- No clean-VM, secondary-distribution, or ARM64 packaging run.
- No physical-device acquisition with a hardware write blocker.
