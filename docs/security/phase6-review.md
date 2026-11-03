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

Status: Resolved in Phase 8. Every evidence-facing job now starts
`ofi-parser-worker` under Landlock, seccomp, rlimits, a timeout, and a bounded
JSON pipe protocol. See
[phase8-review.md](phase8-review.md) for the current evidence and residual
risks. The finding remains recorded here because it was open when the Phase 6
gate was reviewed.

### P6-SEC-002: Database and audit writes are separate resources

Severity: Medium

Audit-open now happens before the case or evidence database row is committed.
This prevents the common failure where an unavailable or locked audit log leaves
a committed database row.

A write failure after the audit file has opened could still occur after the
database commit. That window is now closed by a durable outbox: the case or
evidence row and its audit event are inserted in the same SQLite transaction,
and the event is replayed into the hash-chained log afterwards. An event that
cannot be appended stays queued and is replayed by the next audited operation, so
a committed row always has a recoverable route into the authoritative history.

The API still reports `audit.append_after_commit` when the append fails, because
the state genuinely is committed at that point. The difference is that the event
is no longer lost, and the error message states how many events remain queued.

Recommendation implemented: durable outbox plus replay
(`crates/evidence-manager/src/migrations.rs` version 3,
`app_audit_outbox` table, `replay_outbox` in
`crates/native-bindings/src/lib.rs`).

Status: Resolved. Covered by
`audit_append_failure_is_recovered_from_the_durable_outbox` in
`crates/native-bindings/tests/abi.rs`.

### P6-SEC-003: Sanitizer and long-duration fuzz gates are not run

Severity: Medium

The host has stable Rust 1.93.1 without rustup or nightly. `cargo-fuzz` can run
with coverage instrumentation and `-s none`, but AddressSanitizer cannot start
because cargo-fuzz requires nightly `-Zsanitizer`. The two targets were run for
five seconds each and produced no artifacts.

Recommendation: run ASan, MSan where applicable, and a scheduled fuzz budget on
a pinned nightly toolchain. Include TSK, libewf, EWF metadata, image
identification, and worker RPC mutation targets.

Status: Resolved on the tested host. The toolchain condition this finding named
is gone: nightly Rust is installed with `rust-src` and `llvm-tools`, and both
fuzz targets build under `-Zsanitizer`. Measured coverage is recorded in
[`../testing/phase9-results.md`](../testing/phase9-results.md):

- AddressSanitizer over the whole workspace: exit 0, 69 suites, 166 tests, no
  ASan report, leak checking on with third-party C allocations suppressed.
- MemorySanitizer over the five crates that link no C code
  (`forensic-core`, `hashing`, `viewer`, `audit-log`, `worker-sandbox`): exit 0,
  18 suites, 53 tests, no report.
- A ten-minute-per-target fuzz budget on `c_abi_json` and `viewer_pages`, both
  to completion with no artifact: 10,439,152 and 32,820,070 executions, peak RSS
  512–529 MB against a 4 GiB ceiling.

What this does not cover, and why it is not claimed: TSK, libewf, and the
bundled SQLite are uninstrumented C, so ASan intercepts their allocator use but
not their own reads and writes; MSan is scoped to the crates above because its
shadow tracking stops at an uninstrumented library boundary and would otherwise
report false positives; the Qt/QML desktop is a CMake target with no sanitizer
flags wired in; and a fixed fuzz budget is a floor, not a proof — coverage had
plateaued at the end of both runs, which shows the budget was spent, not that
the parsers are exhaustively tested. The multi-terabyte soak and long-duration
memory stability check is still unrun: it needs release hardware with
multi-terabyte storage and remains open in `IMPLEMENTATION_PLAN.md`, and it stays
on the residual-risk list below.

### P6-SEC-004: Audit chain is not externally notarized

Severity: Low

The local SHA-256 chain detects ordinary edits, deletion, reordering, and
truncation. It cannot prevent an attacker with case-directory access from
rewriting the entire chain and recorded head.

Two cases that previously escaped detection are now closed:

- opening an existing log for append re-verifies the whole chain and requires the
  tail to be the verified head, so a forged or rewritten tail can no longer
  re-anchor the history;
- a log whose final record is not newline-terminated is rejected, so a
  byte-truncated prefix is not accepted as a shorter valid chain.

The remaining limitation is unchanged: an attacker who can rewrite **every**
record and the head still produces an internally consistent chain. Closing that
needs external notarization.

Recommendation: record the head hash in a separate case record, export it to an
external system, or support signed checkpoints. The case database now stores a
per-case `audit_head_hash`, which is the first half of that recommendation.

Status: Accepted limitation with report disclosure; the reopen and truncation
defects are resolved.

## Residual risks

- Native parser defects are contained by a separate worker process, but the
  worker is not a privilege boundary against kernel or sandbox escapes.
- A fixed fuzz budget does not establish parser safety. Ten minutes per target
  with no artifact bounds the defect classes those two targets can reach; TSK,
  libewf, and the bundled SQLite are uninstrumented C, and the Qt desktop is not
  sanitized at all.
- The 8 GiB sparse capacity test does not establish multi-terabyte throughput
  or long-duration memory stability.
- TSK 4.12.1 and the host libewf package are validated only on this host.
- Extraction replacement tests use bounded relocation loops and do not exhaust
  scheduler interleavings.
- GUI accessibility and external viewer isolation remain outside this review.
- The worker response cap bounds retained bytes, not read work; a response that
  crosses it is discarded entirely.
- The seccomp policy is now a default-deny allow-list (Task B1), so an
  unlisted syscall gets `EPERM` rather than being permitted. This bullet used to
  read the opposite and was corrected during Phase 9.
- Cancellation kills the worker process group, leaving an orphan staging
  directory that no reaper surfaces *at the time of this review*. Phase 9 added
  a `.ofi-staging.json` marker and `acquisition::reap_stale_staging`, so an
  orphan is now reaped once its process is gone and it is older than 24 h. That
  reaper's first liveness probe used `kill`, which the worker's own seccomp
  filter refuses with `EPERM` — indistinguishable from a live process — so it
  removed nothing until Phase 9 replaced the probe with `getpgid`; see the
  correction under residual risk 5 in [phase8-review.md](phase8-review.md).

## Gate decision

The tested Phase 6 controls have no failing test and `cargo-audit` reported no
known vulnerability. Phase 8 resolves the parser dispatch portion of
P6-SEC-001. The database/audit two-resource window (P6-SEC-002) is resolved by
the durable outbox. P6-SEC-003 is now resolved on the tested host: ASan, a
scoped MSan, and a ten-minute fuzz budget per target all completed with no
report, no artifact, and no memory growth toward the ceiling, and
`scripts/fuzz-budget.sh` makes the budget repeatable rather than a one-off.

Two parts of the gate still do not pass, and neither is closed by the sanitizer
evidence:

- **The worker is not a privilege boundary against a kernel or sandbox escape.**
  The seccomp allow-list, the age-and-liveness reaper, and per-segment EWF read
  grants all narrow what a compromised parser can reach, but a compromise still
  runs as the examiner's uid. `docs/security/worker-privilege-boundary.md`
  assesses four options and recommends a dedicated system user for the DEB;
  that recommendation is **not implemented**, so the gate stays open.
- **Long-duration acquisition on release hardware is unrun.** A fixed fuzz
  budget and a sanitizer pass say nothing about multi-terabyte throughput or
  memory stability over hours.

Sanitizer coverage is therefore evidence that a class of defect was looked for
and not found. It is not a certification, and this paragraph is not a
declaration that the release gate has passed.
