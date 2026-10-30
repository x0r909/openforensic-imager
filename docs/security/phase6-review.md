# Phase 6 Security Review

Date: 2026-10-30

## Review scope

This review covers the Phase 6 controls that now have executable evidence:

- Read-only source opening and syscall tracing.
- Landlock and seccomp policy behavior in a child process.
- Capability-root extraction under repeated path replacement.
- Append-only audit verification, report output, and write preflight.
- Arbitrary-input parsing in the C ABI and viewer.
- Differential validation against independent forensic tools.
- Dependency advisories and bounded-memory reads.

The review does not certify the complete product or FTK Imager compatibility.

## Verified controls

| Control | Evidence | Result |
|---|---|---|
| Source opened read-only | `strace` child test in `crates/acquisition/tests/acquire.rs` | Passed |
| Source symlink refusal | `crates/image-reader/tests/raw_reader.rs` | Passed |
| Source modification detection | RAW reader tests | Passed |
| Landlock read allowlist | `crates/worker-sandbox/tests/sandbox.rs` | Passed |
| Landlock write denial | sandbox child process | Passed |
| seccomp network denial | `socket` returns `EPERM` | Passed |
| seccomp process creation denial | `fork` returns `EPERM` | Passed |
| Extraction traversal and symlink refusal | extraction tests | Passed |
| Extraction root replacement containment | 64 replacement iterations | Passed |
| Extraction byte limits | per-file and total budget tests | Passed |
| Audit sequence and hash-chain tamper checks | audit-log tests | Passed |
| Audit append lock | concurrent writer test | Passed |
| Audit report safety | report escaping and limitation tests | Passed |
| Desktop case audit integration | Qt model test | Passed |
| Arbitrary ABI input | stable property test and fuzz smoke | Passed |
| Arbitrary viewer input | stable property test and fuzz smoke | Passed |
| Known Rust dependency advisories | `cargo-audit` | None reported |

## Findings

### P6-SEC-001: Parser process isolation is not wired

Severity: High

The `worker-sandbox` crate applies Landlock and seccomp correctly in its child
tests, but the job dispatch path in `crates/native-bindings/src/lib.rs` still
calls TSK, libewf, filesystem, and viewer code inside the desktop process. A
memory-safety defect in a native parser could terminate or compromise the
desktop process rather than being contained by the intended worker boundary.

Technical implication: the current application does not meet the architecture's
full parser crash and compromise isolation claim.

Recommendation: connect every parser-facing job to a dedicated worker process,
apply the sandbox before parsing, enforce rlimits and a watchdog, and add a
crash-isolation integration test plus a clean packaging test for the worker
binary.

Status: Open. This is a release blocker, not a passing Phase 6 security claim.

### P6-SEC-002: Database and audit writes are separate resources

Severity: Medium

Audit-open now happens before the case or evidence database row is committed.
This prevents the common failure where an unavailable or locked audit log leaves
a committed database row. A write failure after the audit file has opened can
still occur after the database commit.

The API exposes that state as `audit.append_after_commit` instead of reporting a
clean failure. The limitation is explicit, but there is no atomic cross-resource
transaction or replayable outbox.

Recommendation: add a durable outbox or reconciliation record that can append
the missing audit event after restart, or move audit state into a transactional
store with a documented recovery protocol.

Status: Open, mitigated by preflight and explicit error reporting.

### P6-SEC-003: Sanitizer and long-duration fuzz gates are not run

Severity: Medium

The host has stable Rust 1.93.1 without rustup or nightly. `cargo-fuzz` can run
with coverage instrumentation and `-s none`, but AddressSanitizer cannot start
because cargo-fuzz requires nightly `-Zsanitizer`. The two targets were run for
five seconds each and produced no artifacts.

Recommendation: run ASan, MSan where applicable, and a scheduled fuzz budget on
a pinned nightly toolchain. Include TSK, libewf, EWF metadata, image
identification, and worker RPC mutation targets.

Status: Open. No sanitizer or long-duration fuzz coverage is claimed.

### P6-SEC-004: Audit chain is not externally notarized

Severity: Low

The local SHA-256 chain detects ordinary edits, deletion, reordering, and
truncation. It cannot prevent an attacker with case-directory access from
rewriting the entire chain and recorded head.

Recommendation: record the head hash in a separate case record, export it to an
external system, or support signed checkpoints.

Status: Accepted limitation with report disclosure.

## Residual risks

- Native parser defects can still affect the desktop process until P6-SEC-001
  is fixed.
- Short fuzz smoke runs do not establish parser safety.
- The 8 GiB sparse capacity test does not establish multi-terabyte throughput
  or long-duration memory stability.
- TSK 4.12.1 and the host libewf package are validated only on this host.
- Extraction replacement tests use bounded relocation loops and do not exhaust
  scheduler interleavings.
- GUI accessibility and external viewer isolation remain outside this review.

## Gate decision

The tested Phase 6 controls have no failing test and `cargo-audit` reported no
known vulnerability. The release security gate is not fully passed because
parser process isolation is not wired, sanitizer coverage was not run, and the
database/audit two-resource failure window remains.
