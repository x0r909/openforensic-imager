# Remaining Work Implementation Plan (Phase 9 onward)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every remaining gate that can be closed on this host — sanitizer
and fuzz evidence for P6-SEC-003, the three Phase 8 residual hardening items, and
the distribution/reproducibility items — and record the hardware-only gates as
runbooks.

**Architecture:** Four tracks. Track A produces parser memory-safety evidence
with ASan and fuzz budgets and closes P6-SEC-003. Track B replaces the seccomp
deny-list with a derived allow-list, reaps orphaned acquisition staging
directories, and narrows the EWF read grant to the segment files. Track C adds
Flatpak to the reproducibility comparison, runs lintian, exercises clean VMs,
and pins native dependencies. Track D records the hardware-only gates.

**Tech Stack:** Rust 1.93.1 pinned by `rust-toolchain.toml` (nightly 1.101 for
sanitizers and fuzzing), Qt 6/QML desktop built by CMake/Ninja, TSK and libewf
from the distribution, worker-sandbox (Landlock + seccomp), cargo-fuzz,
packaging scripts (dpkg, appimagetool, flatpak), Docker for the distribution
matrix.

**Spec:** `docs/security/phase6-review.md` (findings and gate decision),
`docs/security/phase8-review.md` (residual risks), `IMPLEMENTATION_PLAN.md`
(Phase 6/7/8 open items), `SECURITY_MODEL.md` (the allow-list requirement),
`docs/architecture/decisions.md` (ADR-001 native dependency pinning).

## Global Constraints

- Repo commands use the pinned 1.93.1 toolchain through `rust-toolchain.toml`.
  Nightly only through an explicit `cargo +nightly`.
- The fuzz crate lives in `tests/fuzz`; every cargo-fuzz invocation passes
  `--fuzz-dir tests/fuzz`.
- rustc has no UBSan. The sanitizer set is ASan (LeakSanitizer included on
  Linux) and MSan. MSan is claimed only for crates that link no C code.
- Every claim in `docs/testing/` cites the command and its observed output, and
  lists what was not covered.
- The worker boundary keeps failing closed: a Landlock, rlimit, or seccomp step
  that fails aborts the job before evidence is parsed.
- New runtime dependencies are recorded in `packaging/common/THIRD_PARTY.md`
  and reach the CycloneDX SBOM.
- One deliverable per commit, conventional-commit style (`test:`, `fix:`,
  `feat:`, `build:`, `docs:`).

## Review Focus

1. A seccomp allow-list that omits a syscall TSK or libewf needs turns working
   jobs into EPERM failures in production — Task B1 derives the list from
   strace over the real worker test suite and keeps an integration test that
   parses an ext4 and an EWF image through the sandboxed worker.
2. The staging reaper deleting a live acquisition's directory destroys partial
   evidence — Task B2 requires a marker file, a TTL, and a live-pid check, with
   a test that a second acquisition never reaps a live staging directory.
3. Narrowing the EWF read grant can break multi-segment images where only the
   first segment name is known — Task B3 covers a two-segment E01/E02 fixture.
4. Sanitizer runs that skip crates while the results doc claims workspace
   coverage is false evidence — Tasks A1 and A2 record the exact crate lists
   and the excluded C-linked crates.
5. Fuzz artifacts that nobody triages — Task A3 ends with a triage step that
   either opens a fix task or records the input as a known non-crash.

---

## Track A: Security evidence (P6-SEC-003)

### Task A1: ASan workspace evidence

**Files:**

- Create: `scripts/run-sanitizers.sh`
- Create: `docs/testing/phase9-results.md`

**Interfaces:**

- Produces: `scripts/run-sanitizers.sh --sanitizer <address|memory>
  [--crates <list>]`, exit 0 on a clean run, console log at
  `packaging/build/sanitizers/<sanitizer>.log` (git-ignored).

- [x] **Step 1: Write `scripts/run-sanitizers.sh`** ✓ (exists, 3.4K)

  Requirements: `set -euo pipefail`; resolve the repo root like the other
  scripts; export `RUSTFLAGS` and `RUSTDOCFLAGS` with
  `-Zsanitizer=<value>`; run
  `cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu`
  with `--workspace` or the `--crates` list; tee output to the log path; print
  the log path last.

- [x] **Step 2: Run the ASan suite** ✓ (69 suites, 166 tests passed)

  Run: `scripts/run-sanitizers.sh --sanitizer address`

  Expected: every suite prints `test result: ok`, no line contains
  `ERROR: AddressSanitizer`, exit code 0. The whole workspace is safe for ASan
  because ASan tolerates uninstrumented C (TSK, libewf, bundled SQLite).

- [x] **Step 3: Record the evidence in `docs/testing/phase9-results.md`** ✓

  Include: host and kernel, toolchain (`rustc +nightly --version`), the exact
  command, per-crate pass counts, and a `Not covered` list naming the Qt/QML
  desktop (CMake build, no sanitizer flags) and the C libraries.

- [x] **Step 4: Commit** ✓ (already committed)

  `git add scripts/run-sanitizers.sh docs/testing/phase9-results.md`
  `git commit -m "test: run the workspace under address sanitizer"`

### Task A2: MSan scoped evidence

**Files:**

- Modify: `docs/testing/phase9-results.md`

**Interfaces:**

- Consumes: `scripts/run-sanitizers.sh` from Task A1.

- [x] **Step 1: Run the MSan suite over the pure-Rust crates only** ✓

  Run:

  ```
  scripts/run-sanitizers.sh --sanitizer memory \
    --crates "-p forensic-core -p hashing -p viewer -p audit-log \
              -p job-manager -p reporting -p worker-sandbox"
  ```

  Expected: all listed suites print `test result: ok`.

- [x] **Step 2: Record why the C-linked crates are excluded** ✓

  Add to `phase9-results.md`: `acquisition`, `filesystem`, `image-reader`,
  `libewf-bindings`, `partition-analyzer`, `tsk-bindings`,
  `evidence-manager` (bundled SQLite, not MSan-instrumented), and
  `extraction` all link uninstrumented C. Under MSan those libraries report
  uninitialized reads that MSan cannot track, so their results would be false
  positives, not missing coverage.

- [x] **Step 3: Commit** ✓

  `git commit -am "test: record scoped memory-sanitizer evidence"`

### Task A3: Fuzz budget and triage

**Files:**

- Create: `scripts/fuzz-budget.sh`
- Modify: `TEST_STRATEGY.md` (section 11)
- Modify: `docs/testing/phase9-results.md`

**Interfaces:**

- Produces: `scripts/fuzz-budget.sh [--minutes <n>] [--target <name>]`,
  default 5 minutes per target, exit non-zero when an artifact appears under
  `tests/fuzz/artifacts/<target>/`.

- [x] **Step 1: Write `scripts/fuzz-budget.sh`** ✓ (exists, 2.0K)

  For each target of `c_abi_json` and `viewer_pages` run:

  ```
  cargo +nightly fuzz run --fuzz-dir tests/fuzz "$target" -- \
      -max_total_time=$((minutes * 60)) -rss_limit_mb=4096
  ```

  ASan is on by default under cargo-fuzz, so this is the ASan fuzz budget.
  Fail the script when `tests/fuzz/artifacts/$target` is non-empty afterwards.

- [x] **Step 2: Run a real budget** ✓ (10.4M + 32.8M executions, 10min)

  Run: `scripts/fuzz-budget.sh --minutes 10`

  Expected: two `Done N runs in ...` lines. Baseline for comparison: 645,347
  executions in 21 seconds on this host.

- [x] **Step 3: Triage artifacts** ✓ (no artifacts produced)

  When an artifact exists, reproduce with
  `cargo +nightly fuzz run --fuzz-dir tests/fuzz <target> tests/fuzz/artifacts/<target>/<file>`
  and either open a fix task in `IMPLEMENTATION_PLAN.md` or record the input as
  a known non-crash in the results doc. An untriaged artifact fails the task.

- [x] **Step 4: Record the budget and its scheduling decision** ✓

  In `phase9-results.md` record executions per target, RSS ceiling, wall time,
  and the triage outcome. In `TEST_STRATEGY.md` section 11 state how the budget
  is scheduled (a `systemd --user` timer or a CI job), not just that it exists.

- [x] **Step 5: Commit** ✓

  `git add scripts/fuzz-budget.sh TEST_STRATEGY.md docs/testing/phase9-results.md`
  `git commit -m "test: add a repeatable fuzz budget with artifact triage"`

### Task A4: Miri on the pure-Rust crates (optional)

- [x] **Step 1: Install the component** ✓

  Run: `rustup component add miri --toolchain nightly`

- [x] **Step 2: Run Miri** ✓ (3 crates passed)

  Run: `cargo +nightly miri test -p forensic-core -p hashing -p viewer`

  Expected: all tests pass, or a documented list of tests Miri cannot execute
  (filesystem isolation, external process). Record the outcome in
  `phase9-results.md`.

### Task A5: Close P6-SEC-003

**Files:**

- Modify: `docs/security/phase6-review.md`
- Modify: `IMPLEMENTATION_PLAN.md`
- Modify: `docs/testing/phase9-results.md`

- [x] **Step 1: Update the finding status** ✓

  Change the P6-SEC-003 status line to the measured coverage: which sanitizers
  ran, over which crates, with which fuzz budget, and what remains uncovered
  (C libraries, Qt desktop, multi-terabyte soak).

- [x] **Step 2: Update the gate decision paragraph** ✓

  State which part of the release gate now passes and which still does not.
  Do not claim a full pass: the worker is still not a privilege boundary.

- [x] **Step 3: Tick the matching boxes** ✓ in `IMPLEMENTATION_PLAN.md` (Phase 6
  "Run ASan, MSan, and a scheduled fuzz budget" and the Phase 8 duplicate).

- [x] **Step 4: Commit** ✓

  `git commit -am "docs: record the sanitizer and fuzz evidence for P6-SEC-003"`

---

## Track B: Phase 8 residual hardening

### Task B1: Seccomp allow-list

**Files:**

- Modify: `crates/worker-sandbox/src/lib.rs`
- Modify: `crates/worker-sandbox/tests/sandbox.rs`
- Modify: `docs/security/phase8-review.md` (residual risk 3)
- Modify: `SECURITY_MODEL.md` where it describes the filter

**Interfaces:**

- `apply_seccomp_restrictions()` keeps its signature; no caller changes.

- [ ] **Step 1: Write the failing test**

  In `sandbox_child` add, after the fork assertion:

  ```rust
  let priority_result =
      unsafe { libc::syscall(libc::SYS_setpriority, libc::PRIO_PROCESS, 0, 0) };
  assert_eq!(priority_result, -1);
  assert_eq!(
      std::io::Error::last_os_error().raw_os_error(),
      Some(libc::EPERM)
  );
  ```

- [ ] **Step 2: Run it and confirm it fails**

  Run: `cargo test -p worker-sandbox --test sandbox`

  Expected: FAIL — `setpriority` currently succeeds because the deny-list does
  not mention it. That failure is the proof the test distinguishes allow-list
  from deny-list behaviour.

- [ ] **Step 3: Derive the allow-list from the real workload**

  ```
  strace -f -qq -e trace=all -o /tmp/ofi-worker.strace \
      cargo test -p native-bindings -p extraction -p acquisition
  sed -n 's/^[0-9]* *\([a-z_0-9]*\)(.*/\1/p' /tmp/ofi-worker.strace \
      | sort -u
  ```

  Review the set and keep only what the worker needs; record one justification
  line per syscall family in the code comment above `ALLOWED_SYSCALLS`.

- [ ] **Step 4: Implement default-deny**

  Replace `DENIED_SYSCALLS` with `ALLOWED_SYSCALLS`; keep the arch check
  returning `SECCOMP_RET_KILL_PROCESS`; return
  `SECCOMP_RET_ERRNO | EPERM as u32` for everything else.

- [ ] **Step 5: Prove the worker still works end to end**

  Run: `cargo test -p worker-sandbox -p native-bindings`

  Expected: PASS, including `abi.rs` (EWF acquisition and verification through
  the worker) and `worker_isolation.rs`. A missing allow-list entry shows up
  here as an EPERM failure inside a job.

- [ ] **Step 6: Update the docs and commit**

  Rewrite residual risk 3 in `phase8-review.md`; state in `SECURITY_MODEL.md`
  that the filter is now default-deny.

  `git commit -am "feat(worker): make the seccomp policy a default-deny allow-list"`

### Task B2: Reap orphaned staging directories

**Files:**

- Modify: `crates/acquisition/src/acquire.rs` (staging creation, line ~188)
- Create: `crates/acquisition/src/reaper.rs`
- Modify: `crates/acquisition/src/lib.rs` (module + re-exports)
- Modify: `crates/acquisition/tests/acquire.rs`
- Modify: `docs/security/phase8-review.md` (residual risk 5)

**Interfaces:**

- Produces: `pub const DEFAULT_STAGING_MAX_AGE: Duration` (24 h);
  `pub struct ReaperReport { pub removed: Vec<PathBuf>, pub kept: Vec<PathBuf>,
  pub skipped: Vec<PathBuf> }`;
  `pub fn reap_stale_staging(destination_dir: &Path, max_age: Duration,
  process_is_alive: &dyn Fn(u32) -> bool) -> Result<ReaperReport, AcquisitionError>`.
- Produces: a `.ofi-staging.json` marker inside every staging directory, with
  `{"job_id": "...", "pid": N, "started_at_unix_ms": N}`.

- [ ] **Step 1: Write the failing tests** in `crates/acquisition/tests/acquire.rs`

  ```rust
  #[test]
  fn live_staging_directory_is_kept() { /* marker with the current pid */ }

  #[test]
  fn stale_staging_directory_is_reaped() { /* marker with a dead pid and an old mtime */ }

  #[test]
  fn unmarked_directory_without_the_staging_prefix_is_never_reaped() { /* foreign data */ }

  #[test]
  fn fresh_cancelled_staging_survives_the_reaper() { /* ACQ-011 retention still holds */ }
  ```

- [ ] **Step 2: Run them and confirm they fail**

  Run: `cargo test -p acquisition --test acquire`

  Expected: FAIL — `reap_stale_staging` does not exist yet.

- [ ] **Step 3: Implement the marker and the reaper**

  Write the marker right after `std::fs::create_dir(&staging)`; remove it when
  the staging directory is promoted or deliberately retained. The reaper scans
  only entries matching `.<name>.partial-<uuid>`, keeps anything younger than
  `max_age`, keeps entries whose marker pid is alive, and treats an unreadable
  marker as stale only when the directory itself is older than `max_age`.

- [ ] **Step 4: Call the reaper on the acquisition path**

  Call it once at the start of `acquire_path_with_cancel` for the destination
  directory and surface `reaped` (count) in the acquisition manifest JSON. A
  reaper failure must not fail the acquisition; record it as a warning field.

- [ ] **Step 5: Run the tests**

  Run: `cargo test -p acquisition` and then `cargo test --workspace`

  Expected: PASS, including `cancelled_acquisition_reports_its_retained_staging_directory`.

- [ ] **Step 6: Update the residual risk and commit**

  `git commit -am "fix(acquisition): reap orphaned staging directories by age and liveness"`

### Task B3: Narrow the EWF read grant to its segments

**Files:**

- Modify: `crates/libewf-bindings/src/lib.rs` (add `segment_paths`)
- Modify: `crates/native-bindings/src/parser_worker.rs`
  (`evidence_read_root` → per-segment read paths)
- Modify: `crates/native-bindings/tests/worker_isolation.rs`
- Modify: `docs/security/phase8-review.md` (residual risk 4)

**Interfaces:**

- Produces: `pub fn segment_paths(first_segment: &Path) -> Vec<PathBuf>` in
  `libewf-bindings`: every existing sibling whose extension is `E01..E99` or
  `EAA..` in any case, starting with the given path when it exists.

- [ ] **Step 1: Write the failing naming test**

  ```rust
  #[test]
  fn segment_paths_lists_only_existing_siblings() { /* E01, E02, e03 present; E04 absent */ }
  ```

  Run: `cargo test -p libewf-bindings segment_paths`

  Expected: FAIL — the function does not exist.

- [ ] **Step 2: Implement `segment_paths`** and reuse it from
  `discover_segments` where the naming rules overlap.

- [ ] **Step 3: Write the failing sandbox test**

  In `worker_isolation.rs`, write a two-segment EWF fixture with `EwfWriter`
  and a small `maximum_segment_size`, run `Acquisition` through the worker, and
  assert both that the job succeeds and that the sandbox policy contains the
  two segment files and not the parent directory.

- [ ] **Step 4: Implement the narrowed grant**

  In `sandbox_policy`, replace the EWF branch with one read path per segment.
  Landlock `PATH_BENEATH` rules accept a regular file, so each segment is
  granted individually. Keep the non-EWF branch as it is.

- [ ] **Step 5: Run the tests**

  Run: `cargo test -p native-bindings -p libewf-bindings` then
  `cargo test --workspace`

  Expected: PASS. A missing segment must fail the job with a sandbox/read
  error, never with reduced assurance.

- [ ] **Step 6: Update the residual risk and commit**

  `git commit -am "feat(worker): grant EWF reads per segment instead of per directory"`

### Task B4: Decide whether the worker becomes a privilege boundary

This is a decision task, not an implementation task. Residual risk 1 in
`phase8-review.md` records that the worker runs as the examiner's user, so a
kernel or sandbox escape equals full user authority.

**Files:**

- Create: `docs/security/worker-privilege-boundary.md`

- [ ] **Step 1: Write the comparison**

  Evaluate the real options — user namespaces with a mapped root and no
  capabilities, a `bwrap`-style mount namespace, a dedicated system user with
  `setuid` at spawn, or keep the current model — against: what each blocks
  that the others do not, what it costs (packaging, permissions, single-user
  desktops), and which failure mode remains.

- [ ] **Step 2: Recommend one and record the cost of being wrong.**

- [ ] **Step 3: Hand the recommendation to the human partner**; only then does
  the follow-up implementation become a plan.

---

## Track C: Distribution and reproducibility

### Task C1: Flatpak in the reproducibility comparison

**Files:**

- Modify: `scripts/verify-reproducible-packages.sh`
- Modify: `docs/packaging/REPRODUCIBLE_BUILD.md`

- [ ] **Step 1: Add the Flatpak build to each build set**

  Call `scripts/package-flatpak.sh --skip-build` with the same
  `OFI_OUTPUT_DIR`, `OFI_BUILD_DIR`, and `CARGO_TARGET_DIR` overrides the other
  packages use.

- [ ] **Step 2: Compare the right things**

  A Flatpak bundle is not byte-reproducible by design. Compare: the CycloneDX
  SBOM digest, the exported app metadata (`metadata` file from `build-finish`),
  and a per-file `sha256sum` manifest of `files/`. Fail when the two sets
  differ, and print the reason for the bundle-level exemption.

- [ ] **Step 3: Run it**

  Run:

  ```
  SOURCE_DATE_EPOCH=1793318400 scripts/verify-reproducible-packages.sh
  ```

  Expected: DEB, AppImage, SBOM, and Flatpak manifests all match between build
  sets; exit 0.

- [ ] **Step 4: Commit**

  `git commit -am "build(flatpak): compare Flatpak metadata in the reproducibility check"`

### Task C2: lintian and distribution policy review

- [ ] **Step 1: Run lintian on the built DEB**

  Run: `lintian --display-info --pedantic packaging/output/openforensic-imager_0.1.0_amd64.deb`

- [ ] **Step 2: Triage every tag**

  Fix fixable packaging errors (control fields, maintainer scripts, file
  modes, missing man page) in `packaging/debian/`. Record every remaining tag
  with a justification.

- [ ] **Step 3: Write `docs/packaging/lintian-review.md`**

  Contents: the exact command, the tag list, what was fixed, what remains, and
  which downstream policies are still unreviewed (Debian, Arch, Fedora).

- [ ] **Step 4: Commit**

  `git commit -am "docs(packaging): record the lintian pass and remaining tags"`

### Task C3: Distribution matrix evidence (containers)

**Files:**

- Create: `scripts/check-distro-build.sh`
- Create: `docs/packaging/distro-matrix.md`

- [ ] **Step 1: Write the script**

  For each image in `ubuntu:26.04`, `debian:trixie`, `archlinux:latest`,
  `fedora:latest`: bind-mount the repo read-only, install build dependencies
  plus that distribution's TSK/libewf packages, run
  `cargo test --workspace` and `cmake -G Ninja -S apps/desktop -B /tmp/build`,
  and capture stdout/stderr per image into `packaging/build/distro/`.

- [ ] **Step 2: Run it and record the outcome**

  Expected: Ubuntu passes. For the others, record the exact failure (missing
  package names, missing TSK features, Qt version) rather than guessing.

- [ ] **Step 3: Write the decision**

  In `distro-matrix.md`, state per distribution whether it gets native
  packages or is served by the AppImage and Flatpak, and what work that
  implies. This is a decision document, not a promise.

- [ ] **Step 4: Commit**

  `git commit -am "docs(packaging): record the distribution matrix evidence"`

### Task C4: Clean-Ubuntu VM install and exercise

**Prerequisite:** `sudo apt install qemu-system-x86` (only the arm emulator
variants are installed) and an Ubuntu 26.04 cloud image.

**Files:**

- Create: `scripts/test-clean-vm.sh`
- Modify: `docs/packaging/INSTALL.md`

- [ ] **Step 1: Write the script**

  Download the cloud image with a recorded SHA-256, write a cloud-init seed
  that installs the DEB from a virtfs share, boot
  `qemu-system-x86_64 -M q35 -m 4096 -nographic` with user networking, run
  `dpkg -i` plus the desktop self-test inside the guest, and capture the
  serial console to `packaging/build/clean-vm/serial.log`.

- [ ] **Step 2: Run it once and keep the log**

  Expected: the guest installs the package, `openforensic-imager --self-test`
  exits 0, and the log shows both. Record the run in
  `docs/testing/phase9-results.md`.

- [ ] **Step 3: Commit**

  `git commit -am "build: exercise the DEB in a clean Ubuntu VM"`

### Task C5: ARM64 packages in an emulated VM

**Prerequisite:** an Ubuntu arm64 cloud image; `qemu-system-aarch64` and the
AAVMF firmware are already installed.

- [ ] **Step 1: Boot the arm64 guest** with the same harness as Task C4,
  swapped to `-M virt -cpu cortex-a72` and the AAVMF firmware.

- [ ] **Step 2: Build natively inside the guest** (native is simpler and more
  honest than cross-compiling C dependencies): `cargo test --workspace`, then
  `scripts/package-deb.sh` and `scripts/package-appimage.sh`.

- [ ] **Step 3: Copy the arm64 artifacts out**, record their digests, and note
  which native dependency versions the arm64 base image provides.

- [ ] **Step 4: Record in `docs/testing/phase9-results.md`** and tick the ARM64
  item in `IMPLEMENTATION_PLAN.md` only if both packages install and self-test
  inside the guest.

### Task C6: Pin native dependencies (ADR-001)

This is the largest task; split it before starting.

The licensing half of the matching Phase 8 item is already resolved: the
project adopted `GPL-3.0-or-later` and ships it with every artifact. What
remains is dependency pinning and the redistribution review.

- [ ] **Step 1: Decide the linking model** in a short design note under
  `docs/architecture/`: pinned static link, bundled shared objects, or
  distribution dependencies with a documented minimum. ADR-001 currently
  promises pinned reviewed versions, so the plan must either implement that or
  amend the ADR with the reason.

- [ ] **Step 2: Fetch and verify** the chosen TSK and libewf source releases
  into `packaging/tools.lock` style entries with SHA-256 and build flags.

- [ ] **Step 3: Build and link them** into the DEB, AppImage, and Flatpak
  payloads; extend `scripts/generate-sbom.sh` and
  `packaging/common/THIRD_PARTY.md` with the pinned versions.

- [ ] **Step 4: Prove it** with `cargo test --workspace`, the differential
  tests, and `scripts/verify-reproducible-packages.sh`.

### Task C7: Replace the placeholder icon

- [ ] **Step 1: Produce the artwork** (final decision belongs to the user; the
  placeholder is CC0-1.0 today). Keep any new asset under a license that can
  ship with a GPL-3.0-or-later package.

- [ ] **Step 2: Replace the asset** in `packaging/common/openforensic-imager.svg`
  and every consumer (DEB, AppImage, Flatpak, AppStream metadata).

- [ ] **Step 3: Run the packaging tests** and record the new digests.

---

## Track D: Prepared, not executable here

### Task D1: Multi-terabyte soak harness

- [ ] **Step 1: Write `scripts/soak-acquisition.sh`** with bounded memory
  (RSS ceiling argument), a configurable source size, and a fixed duration;
  it records throughput, peak RSS, and hash stability over repeated runs.

- [ ] **Step 2: Dry-run it here** on a sparse 100 GB file (the host has 120 GB
  free) and record the result as a bounded-memory smoke test, explicitly not
  as a multi-terabyte claim.

- [ ] **Step 3: Document the gap** in `TEST_STRATEGY.md`: multi-terabyte and
  long-duration runs still require release hardware.

### Task D2: Physical-device acquisition runbook

- [ ] **Step 1: Write `docs/testing/device-acquisition-runbook.md`**: hardware
  write blocker set-up, device identification, expected evidence (command
  lines, hashes, screenshots), abort criteria, and the release-gate statement.

- [ ] **Step 2: Record it as blocked-on-hardware** in `IMPLEMENTATION_PLAN.md`,
  naming exactly what must exist before the box can be ticked.

### Task D3: Keep P6-SEC-004 an accepted limitation

- [ ] **Step 1: No work.** External notarization of the audit chain stays an
  accepted limitation with report disclosure, as `phase6-review.md` records.
  Re-check it only if distribution requirements change.

---

## Execution order

1. Track A first: it closes the only Open security finding and produces the
   evidence the other tracks are judged by.
2. Track B second: code changes with regression tests, independent of Track A.
3. Track C third: C1 and C2 are quick and unblocked; C3 to C7 are larger and
   resource-bound; C6 needs a design decision before any code.
4. Track D is writing only; no gate closes.

## Notes for the executor

- Verified on this host: `scripts/run-sanitizers.sh`-equivalent commands
  compile and pass (`RUSTFLAGS="-Zsanitizer=address" cargo +nightly test
  -Zbuild-std --target x86_64-unknown-linux-gnu -p forensic-core` → 7 tests).
- `cargo +nightly fuzz list --fuzz-dir tests/fuzz` → `c_abi_json`,
  `viewer_pages`; a 21-second run executed 645,347 inputs.
- C-linked crates (MSan exclusions): `acquisition`, `filesystem`,
  `image-reader`, `libewf-bindings`, `partition-analyzer`, `tsk-bindings`,
  `evidence-manager` (bundled SQLite), plus `extraction` and `native-bindings`
  transitively.
- `qemu-system-x86_64` is not installed; the arm variants are.
- `tests/fuzz/Cargo.lock` is modified in the working tree from a local fuzz
  build; decide with the human whether that update is kept.
