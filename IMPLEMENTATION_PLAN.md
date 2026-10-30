# OpenForensic Imager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a tested Linux-native forensic acquisition and analysis
application, with Phases 1 through 7 implemented and their remaining release
blockers documented.

**Architecture:** Rust libraries provide integrity-critical domain logic,
persistence, hashing, image access, and reporting. A C ABI connects them to a
C++/Qt desktop shell. Native forensic parsers run in an isolated worker process.

**Tech Stack:** Rust, Tokio, SQLite, Serde, Tracing, Qt 6, QML, TSK, libewf,
optional libyal libraries, CMake, Ninja, Cargo.

**Spec:** [REQUIREMENTS.md](REQUIREMENTS.md)

## Global constraints

- Primary platform is Ubuntu 26.04 LTS x86_64.
- Secondary platforms are Debian, Arch Linux, and Fedora.
- Future architecture includes ARM64.
- The application MUST open source evidence read-only and MUST NOT modify it.
- The application MUST NOT automatically mount evidence read-write.
- Evidence is untrusted input.
- Every successful acquisition MUST have a corresponding verification test.
- Never load an entire disk image into memory.
- Use streaming I/O and bounded buffers.
- Use checked arithmetic for offsets and lengths.
- Use established forensic libraries for mature formats.
- Do not invent FTK behavior or claim compatibility without test evidence.
- Never report a feature complete without executable tests.
- Pin Rust, Qt, TSK, libewf, and optional native libraries in release builds.
- Keep the supplied FTK installer untracked.
- Preserve existing user changes in the worktree.

## Review focus

1. Source safety: a malformed or hostile path must never cause a write to
   evidence.
2. Arithmetic: every image range and file size combination must reject overflow
   and out-of-range access.
3. Result honesty: partial reads, unavailable hashes, and uncertain timestamps
   must remain visibly non-successful.
4. Durability: an evidence or audit write must not appear committed before its
   required flush and hash-chain update.
5. Resource exhaustion: a large image, directory, name, or page request must
   fail or page within a documented budget.

## Roadmap

| Phase | Scope | Acceptance gate |
|---|---|---|
| 0 | Environment, feasibility, architecture, requirements, security, tests | These documents reviewed and approved |
| 1 | Workspace, forensic core, RAW reader, streaming hashes, evidence metadata | Tested core with deterministic RAW fixtures |
| 2 | RAW and E01 acquisition, verification, acquisition reports | Paired acquisition and verification evidence |
| 3 | MBR/GPT, NTFS, FAT32, exFAT, ext4, metadata, deleted status | Differential parser evidence |
| 4 | Viewer, hex, preview, extraction | Security and resource tests |
| 5 | Qt UI, QML, models, jobs, evidence manager | GUI and accessibility tests |
| 6 | Full audit, fuzzing, differential, security validation | No open critical/high defects |
| 7 | DEB, AppImage, Flatpak assessment, reproducibility | Clean-system release tests |

## Phase 1 file map

```text
Cargo.toml
rust-toolchain.toml
crates/forensic-core/src/lib.rs
crates/forensic-core/src/id.rs
crates/forensic-core/src/error.rs
crates/forensic-core/src/range.rs
crates/forensic-core/src/time.rs
crates/hashing/src/lib.rs
crates/hashing/src/digest.rs
crates/hashing/src/stream.rs
crates/image-reader/src/lib.rs
crates/image-reader/src/source.rs
crates/image-reader/src/raw.rs
crates/evidence-manager/src/lib.rs
crates/evidence-manager/src/db.rs
crates/evidence-manager/src/migrations.rs
crates/audit-log/src/lib.rs
crates/audit-log/src/chain.rs
crates/native-bindings/src/lib.rs
crates/native-bindings/include/ofi.h
tests/fixtures/src/lib.rs
tests/fixtures/src/raw.rs
tests/integration/phase1_core.rs
```

## Phase 1 tasks

### Task 1: Workspace and forensic-core primitives

**Files:**

- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/forensic-core/Cargo.toml`
- Create: `crates/forensic-core/src/lib.rs`
- Create: `crates/forensic-core/src/id.rs`
- Create: `crates/forensic-core/src/error.rs`
- Create: `crates/forensic-core/src/range.rs`
- Test: `crates/forensic-core/tests/primitives.rs`

**Interfaces:**

- Produces `CaseId`, `EvidenceId`, `SourceId`, `ArtifactId`, `JobId` as UUID
  newtypes.
- Produces `ByteRange::new(offset: u64, length: u64, subject_length: u64) ->
  Result<ByteRange, CoreError>`.
- Produces `CoreError` with a stable `ErrorCode`, context, and retryability.

- [x] **Step 1: Create the Cargo workspace and empty crate**
- [x] **Step 2: Write failing tests for UUID round trips, zero-length ranges, boundary ranges, and overflow rejection**
- [x] **Step 3: Run the tests and verify they fail**
- [x] **Step 4: Implement newtype IDs, `ByteRange`, and `CoreError`**
- [x] **Step 5: Run `cargo test -p forensic-core` and verify all tests pass**
- [x] **Step 6: Commit the task**

### Task 2: Streaming hash engine

**Files:**

- Create: `crates/hashing/Cargo.toml`
- Create: `crates/hashing/src/lib.rs`
- Create: `crates/hashing/src/digest.rs`
- Create: `crates/hashing/src/stream.rs`
- Test: `crates/hashing/tests/known_vectors.rs`
- Test: `crates/hashing/tests/chunk_properties.rs`

**Interfaces:**

- Consumes `CoreError` from `forensic-core`.
- Produces `HashAlgorithm { Md5, Sha1, Sha256, Sha512 }`.
- Produces `HashScope { SourceRead, OutputLogical, StoredFormat, ExtractedFile }`.
- Produces `StreamingHasher::new(algorithms: &[HashAlgorithm])`.
- Produces `StreamingHasher::update(&mut self, bytes: &[u8])`.
- Produces `StreamingHasher::finish(self) -> BTreeMap<HashAlgorithm, HashDigest>`.

- [x] **Step 1: Add known-answer tests for empty input and `abc` for all four algorithms**
- [x] **Step 2: Run the tests and verify they fail**
- [x] **Step 3: Implement digest encoding and streaming contexts**
- [x] **Step 4: Add a property test that randomized chunking matches one-shot hashing**
- [x] **Step 5: Run `cargo test -p hashing` and verify all tests pass**
- [x] **Step 6: Compare SHA-256 of a generated file with `sha256sum`**
- [x] **Step 7: Commit the task**

### Task 3: Read-only RAW image reader

**Files:**

- Create: `crates/image-reader/Cargo.toml`
- Create: `crates/image-reader/src/lib.rs`
- Create: `crates/image-reader/src/source.rs`
- Create: `crates/image-reader/src/raw.rs`
- Test: `crates/image-reader/tests/raw_reader.rs`

**Interfaces:**

- Consumes `ByteRange`, `CoreError`, and `JobId`.
- Produces `ImageReader` trait:
  - `logical_length(&self) -> u64`
  - `sector_size(&self) -> u32`
  - `read_exact_at(&self, range: ByteRange, out: &mut [u8]) -> Result<(), ReadError>`
  - `metadata(&self) -> ImageMetadata`
- Produces `RawReader::open_read_only(path: &Path) -> Result<RawReader, ImageError>`.
- Produces `SourceFingerprint` containing device or file identity, length,
  modification identity, and read-only status.

- [x] **Step 1: Write failing tests for a missing file, an empty file, a full valid read, and an out-of-range read**
- [x] **Step 2: Run the tests and verify they fail**
- [x] **Step 3: Implement source opening with `O_RDONLY | O_CLOEXEC` and metadata capture**
- [x] **Step 4: Implement `pread`-based exact range reads with checked offsets**
- [x] **Step 5: Add a test that changes the file during a read session and requires a typed concurrent-modification error**
- [x] **Step 6: Run `cargo test -p image-reader` and verify all tests pass**
- [x] **Step 7: Commit the task**

### Task 4: Evidence metadata and migrations

**Files:**

- Create: `crates/evidence-manager/Cargo.toml`
- Create: `crates/evidence-manager/src/lib.rs`
- Create: `crates/evidence-manager/src/db.rs`
- Create: `crates/evidence-manager/src/migrations.rs`
- Test: `crates/evidence-manager/tests/cases_and_evidence.rs`

**Interfaces:**

- Consumes domain IDs and errors.
- Produces `CaseStore::open(path: &Path) -> Result<CaseStore, StoreError>`.
- Produces `CaseStore::create_case(input: CreateCase) -> Result<CaseRecord, StoreError>`.
- Produces `CaseStore::add_evidence(input: AddEvidence) -> Result<EvidenceRecord, StoreError>`.
- Produces `CaseStore::list_evidence(case_id: CaseId) -> Result<Vec<EvidenceRecord>, StoreError>`.

- [x] **Step 1: Write failing tests for schema creation, foreign keys, duplicate IDs, UTC timestamps, and transactional evidence insertion**
- [x] **Step 2: Run the tests and verify they fail**
- [x] **Step 3: Implement versioned migrations and the minimum Phase 1 tables**
- [x] **Step 4: Enable WAL and `synchronous = FULL` for evidence-state commits**
- [x] **Step 5: Add a test that reopening the database preserves source fingerprint and evidence identifiers**
- [x] **Step 6: Run `cargo test -p evidence-manager` and verify all tests pass**
- [x] **Step 7: Commit the task**

### Task 5: Append-only audit log

**Files:**

- Create: `crates/audit-log/Cargo.toml`
- Create: `crates/audit-log/src/lib.rs`
- Create: `crates/audit-log/src/chain.rs`
- Test: `crates/audit-log/tests/append_only.rs`

**Interfaces:**

- Consumes `CaseId`, `JobId`, and `CoreError`.
- Produces `AuditLog::open_append(path: &Path) -> Result<AuditLog, AuditError>`.
- Produces `AuditLog::append(event: AuditEvent) -> Result<AuditReceipt, AuditError>`.
- Produces `AuditLog::verify_chain(path: &Path) -> Result<AuditChainStatus, AuditError>`.
- Produces `AuditReceipt { sequence: u64, record_hash: Sha256Digest }`.

- [x] **Step 1: Write failing tests for monotonic sequence, previous-hash linkage, UTF-8 escaping, and fsync-before-return**
- [x] **Step 2: Run the tests and verify they fail**
- [x] **Step 3: Implement canonical record encoding and SHA-256 chain hashing**
- [x] **Step 4: Implement exclusive-file-lock append and flush**
- [x] **Step 5: Add tamper tests for edited, reordered, deleted, and truncated records**
- [x] **Step 6: Add a concurrent writer test with one accepted writer and typed rejection for competing writers**
- [x] **Step 7: Run `cargo test -p audit-log` and verify all tests pass**
- [x] **Step 8: Commit the task**

### Task 6: Minimal C ABI

**Files:**

- Create: `crates/native-bindings/Cargo.toml`
- Create: `crates/native-bindings/src/lib.rs`
- Generate: `crates/native-bindings/include/ofi.h`
- Test: `crates/native-bindings/tests/abi.rs`
- Create: `tests/integration/c-abi-smoke.c`

**Interfaces:**

- Produces `ofi_abi_version()`.
- Produces `ofi_case_create` and `ofi_evidence_add` request adapters.
- Produces owned byte and error release functions.
- Consumes Serde request and response types from `forensic-core`.

- [x] **Step 1: Write failing Rust tests for null pointers, invalid lengths, invalid UTF-8, and panic containment**
- [x] **Step 2: Run the tests and verify they fail**
- [x] **Step 3: Implement the ABI version check and owned response envelope**
- [x] **Step 4: Generate the C header and compile the C smoke harness**
- [x] **Step 5: Run the C harness and verify allocation and release ownership**
- [x] **Step 6: Run `cargo test -p native-bindings` and verify all tests pass**
- [x] **Step 7: Commit the task**

### Task 7: Deterministic RAW fixture generator

**Files:**

- Create: `tests/fixtures/Cargo.toml`
- Create: `tests/fixtures/src/lib.rs`
- Create: `tests/fixtures/src/raw.rs`
- Test: `tests/fixtures/tests/raw_manifest.rs`

**Interfaces:**

- Produces `generate_raw_fixture(seed: u64, layout: RawLayout) -> FixtureManifest`.
- Produces `FixtureManifest` with byte length, block size, segment sizes, file
  hashes, and generator version.

- [x] **Step 1: Write failing tests for empty, sparse, dense, and split RAW layouts**
- [x] **Step 2: Implement deterministic pseudorandom generation from a fixed seed**
- [x] **Step 3: Assert regenerated fixtures match their manifests**
- [x] **Step 4: Verify fixture hashes with `sha256sum`**
- [x] **Step 5: Commit the generator and manifests, not generated binaries**

### Task 8: Phase 1 integration test

**Files:**

- Create: `tests/integration/phase1_core.rs`
- Modify: `Cargo.toml`

**Interfaces:**

- Consumes all Phase 1 crates.

- [x] **Step 1: Write a test that creates a case, hashes a RAW fixture, opens it read-only, reads selected ranges, records evidence metadata, and appends audit events**
- [x] **Step 2: Assert the source fingerprint and source hash before and after are unchanged**
- [x] **Step 3: Assert the database and audit log reopen successfully**
- [x] **Step 4: Run `cargo test --workspace` and verify all tests pass**
- [x] **Step 5: Run `cargo clippy --workspace --all-targets -- -D warnings`**
- [x] **Step 6: Run the integration test under AddressSanitizer where supported**
- [x] **Step 7: Commit the Phase 1 integration**

## Phase 2 execution outline

Before code:

- Review the pinned libewf commit and license.
- Add `ewf-tools` to the reproducible test image.
- Define acquisition and verification state machines and schemas.
- Write failing tests for RAW acquisition, E01 round trip, cancellation, partial
  acquisition, read errors, and verification mismatch.

Implementation order:

1. RAW acquisition with staged output and manifest.
2. Independent source and output hash contexts.
3. Pinned libewf wrapper and E01 writer.
4. E01 reader and metadata.
5. Verification engine and report models.
6. Acquisition and verification reports.
7. Worker isolation and cancellation integration.
8. Differential tests against `ewfverify`, `ewfinfo`, `img_cat`, and TSK.

Phase 2 is not accepted until every successful acquisition is paired with
verification evidence and partial or failed outputs are unmistakable.

## Phase 3 execution outline

1. Pin and build TSK.
2. Wrap MBR and GPT volume systems.
3. Add partition differential tests.
4. Wrap filesystem open, directory pages, metadata, and file reads.
5. Add timestamp provenance handling for NTFS, FAT32, exFAT, and ext4.
6. Add deleted-name, unallocated metadata, and orphan status.
7. Differential tests per filesystem.
8. Document all parser-specific limitations.

## Phase 4 execution outline

1. Hex paging model.
2. Bounded text viewer and encoding detection.
3. File signature identification.
4. Image and optional PDF preview.
5. Capability-based extraction with `openat2`.
6. Extraction metadata and report.
7. Path traversal, symlink race, resource, and decompression tests.

Phase 4 core implementation is complete and recorded in
[`docs/testing/phase4-results.md`](docs/testing/phase4-results.md). Actual
image/PDF decoding belongs behind the Phase 5 worker and Qt renderer boundary;
the core currently supplies validated render plans and resource limits.

## Phase 5 execution outline

1. C++ `QMainWindow` shell.
2. QML quick widgets and theme tokens.
3. Evidence tree and file list models.
4. Properties and viewer tabs.
5. Acquisition wizard and verification view.
6. Background task manager.
7. Keyboard, accessibility, theme, and large-model tests.

The background job manager is implemented and documented in
[`docs/testing/phase5-background-jobs.md`](docs/testing/phase5-background-jobs.md).
The Qt shell, QML workbench, models, themes, and offscreen GUI tests are
implemented and documented in
[`docs/testing/phase5-ui-results.md`](docs/testing/phase5-ui-results.md).
The case/acquisition panel, acquisition wizard path, verification path, and
report writing are implemented. Evidence browsing screens, extraction report
views, and full accessibility-tree testing remain open.

## Phase 6 execution outline

1. Full audit reports and chain verification.
2. Differential suite expansion.
3. Native fuzzing and sanitizer runs.
4. Worker sandbox negative tests.
5. Source-write and extraction-race validation.
6. Capacity and soak tests.
7. Security review and limitation report.

### Phase 6 status

- [x] Add JSON, CSV, and HTML audit reports with chain status and an explicit
  non-notarization limitation.
- [x] Add `audit_append` and `audit_verify` ABI jobs.
- [x] Preflight audit-log open before case and evidence database commits.
- [x] Add MBR, GPT, ext4, RAW/E01, and SHA-256 differential tests.
- [x] Replace the side-effecting ABI fuzz target with a side-effect-free request
  parser target.
- [x] Run five-second `cargo-fuzz` smoke tests for the viewer and ABI JSON
  targets with sanitizers disabled.
- [x] Add Landlock and seccomp negative tests in a forked child.
- [x] Add a `strace` regression test proving acquisition opens the source with
  `O_RDONLY`.
- [x] Add 64 repeated extraction-root replacement iterations.
- [x] Add a sparse 8 GiB bounded-memory random-read test.
- [x] Record the 45-binary, 143-test workspace result, CTest result, vulnerability
  audit, and limitations in
  [docs/testing/phase6-results.md](docs/testing/phase6-results.md).
- [x] Record open security findings and residual risk in
  [docs/security/phase6-review.md](docs/security/phase6-review.md).
- [ ] Wire the tested worker sandbox into parser dispatch. This is a high
  security release blocker.
- [ ] Run ASan, MSan, and a scheduled fuzz budget on nightly Rust. This host has
  no rustup or nightly toolchain.
- [ ] Run multi-terabyte acquisition and long-duration soak tests on release
  hardware.

Phase 6 validation is complete at the executable coverage level. The production
security gate remains open.

## Phase 7 execution outline

1. Reproducible DEB build.
2. AppImage with pinned libraries.
3. Flatpak analysis build and documented acquisition limits.
4. SBOM and third-party notices.
5. Clean-system installation and smoke tests.
6. Installation, recovery, and reproducibility documentation.

### Phase 7 status

- [x] Standardize the desktop, icon, AppStream, and Flatpak application ID as
  `io.github.x0r909.OpenForensicImager`.
- [x] Add Release Cargo integration and CMake install rules for the executable,
  native bindings, desktop entry, icon, AppStream metadata, and notices.
- [x] Build a development DEB with Ubuntu runtime dependencies and an extracted
  root self-test.
- [x] Build an AppImage with Qt plugins, QML modules, native bindings, a pinned
  AppImage build-tool checksum, and an artifact self-test.
- [x] Build an analysis-only Flatpak with no `/dev` access, test it in the build
  sandbox, and install and run it with an isolated user data directory.
- [x] Generate a CycloneDX 1.6 SBOM and SHA-256 files for package artifacts.
- [x] Add layout tests for desktop metadata, AppStream metadata, scripts, and
  required notices.
- [x] Add a two-build SHA-256 and size comparison for DEB and AppImage using
  `SOURCE_DATE_EPOCH=1793318400`.
- [x] Document installation, removal, case-data recovery, and reproducible
  build commands.
- [ ] Install and exercise the DEB on a clean Ubuntu VM.
- [ ] Verify Debian, Arch Linux, Fedora, and ARM64 packaging.
- [ ] Run `lintian` and complete downstream distribution policy review.
- [ ] Replace the placeholder icon, select the project license, and complete
  native dependency redistribution review.
- [ ] Include Flatpak and SBOM output in the reproducibility comparison.
- [ ] Run a packaged physical-device acquisition with a hardware write blocker.

Phase 7 is complete as a locally tested development packaging increment. A
production distribution gate remains open until the clean-system,
cross-distribution, licensing, native dependency pinning, and device tests pass.

## Self-review

- Spec coverage: Phase 1 covers workspace, core, RAW read, streaming hashes, and
  evidence metadata. Later phases map the remaining modules.
- Step scan: Each Phase 1 task has a failing test, implementation, verification,
  and commit.
- Type consistency: IDs, `ByteRange`, `ImageReader`, `HashAlgorithm`, and
  `AuditReceipt` names are stable across tasks.
- Review focus: Source safety, overflow, result honesty, durability, and
  resource limits each have owning tests.
- Proportion: Phase 1 is intentionally limited. Qt, TSK, and libewf are not
  implemented before their review gates.

