# OpenForensic Imager Architecture

Status: Proposed for Phase 0 review.

## 1. Goals

The architecture must:

- Preserve source evidence.
- Produce reproducible images and reports.
- Keep the user interface responsive while processing multi-terabyte data.
- Contain parser crashes and resource exhaustion away from the GUI and case
  database.
- Expose every integrity decision, timestamp assumption, and partial result.
- Use mature forensic libraries rather than homemade format parsers.
- Keep native dependency versions identical across supported Linux
  distributions.

## 2. Non-goals for release 0.1

- A copy of the FTK Imager UI or internal design.
- Windows support.
- Physical acquisition without an external hardware write blocker.
- Read-write mounting of evidence.
- EWF2 write support, encrypted EWF creation, AD1, L01, AFF, or S01 creation.
- Guaranteed recovery of deleted files whose data blocks have been reused.
- Silent conversion of filesystem timestamps to UTC when the source format
  cannot establish a unique instant.

## 3. Architecture alternatives

### Alternative A: C++ core and Rust helpers

This would make Qt integration direct, but Rust's ownership model, safe
streaming I/O, and test ecosystem are valuable for the integrity-critical core.
It would also duplicate domain models across C++ and Rust. Rejected.

### Alternative B: Pure Rust UI with a non-Qt toolkit

This reduces FFI, but it conflicts with the required Qt 6 and QML stack.
Rejected.

### Alternative C: Rust core, C ABI bridge, C++/Qt shell, isolated workers

This preserves a strongly typed Rust core, a narrow stable ABI, the required Qt
desktop stack, and process isolation for untrusted parsers. Chosen.

### Alternative D: In-process native parsers

This is simpler initially, but a TSK or libewf memory fault would terminate the
UI and could interrupt case writes. Rejected for the default analysis path.

## 4. System context

```text
                  +--------------------------------------+
                  |          Qt desktop process          |
                  |  QMainWindow, menus, toolbar, QML     |
                  |  item models, progress, task manager  |
                  +------------------+-------------------+
                                     |
                          narrow C ABI, UTF-8, callbacks
                                     |
                  +------------------v-------------------+
                  |       Rust application service        |
                  |  job orchestration, case database,    |
                  |  audit log, reporting, policy, IPC    |
                  +----------+----------------+-----------+
                             |                |
                   bounded RPC over Unix     | SQLite and JSONL
                             |                |
                  +----------v-----------+  +-v------------------+
                  | Rust analysis worker |  | case.db            |
                  | TSK, libewf, libyal  |  | audit.jsonl        |
                  | seccomp, Landlock,   |  | job manifests      |
                  | rlimits, watchdog    |  +--------------------+
                  +----------+-----------+
                             |
                 O_RDONLY pread / bounded writes
                             |
                  +----------v-----------+
                  | Evidence sources and |
                  | acquisition targets  |
                  +----------------------+
```

The GUI never calls TSK, libewf, or another forensic parser directly. The
service process owns source policy, job state, evidence metadata, audit order,
and report generation. The worker process performs parser-facing work and has
the smallest practical filesystem and system-call surface.

## 5. Process model

### Desktop process

- Owns the Qt GUI and QML engine.
- Holds only presentation state and model adapters.
- Calls the Rust service through a C ABI.
- Converts background callbacks into queued GUI-thread updates.
- Never opens evidence with write access.

### Rust application service

- Runs a multithreaded Tokio runtime.
- Validates all FFI inputs and catches Rust panics at the ABI boundary.
- Owns the case database and append-only audit writer.
- Starts one worker per job or a small bounded worker pool.
- Assigns job identifiers and cancellation tokens.
- Stores bounded progress snapshots rather than unbounded event streams.
- Writes reports from committed metadata, not transient worker memory.

### Analysis worker

- One process per active parser job initially.
- Opens source files or devices with `O_RDONLY | O_CLOEXEC`.
- Uses bounded read buffers and checked offset arithmetic.
- Loads pinned TSK and libewf builds.
- Returns typed results and parser diagnostics over a local Unix socket.
- Runs with `PR_SET_NO_NEW_PRIVS`, seccomp, Landlock, and applicable rlimits.
- May read only explicitly granted evidence paths and write only to a granted
  staging directory.
- Is killed on timeout or protocol violation without taking down the GUI.

### FFI callback rule

Worker callbacks may arrive on any Tokio worker thread. The C++ bridge must not
touch Qt objects directly from those callbacks. It enqueues the event with
`QMetaObject::invokeMethod(..., Qt::QueuedConnection)` and updates Qt on the GUI
thread. This matches Qt's requirement that an item model be used only from its
own thread.

## 6. Repository layout

```text
apps/
  desktop/                 C++ Qt shell and QML bootstrap
  service/                 Rust application service binary
  worker/                  Rust analysis worker binary
crates/
  forensic-core/           IDs, ranges, statuses, errors, time provenance
  acquisition/             Source opening, read loop, output commits
  hashing/                 Streaming MD5, SHA-1, SHA-256, SHA-512
  image-reader/            RAW and EWF readers behind one trait
  partition-analyzer/      TSK volume-system wrapper
  filesystem/              TSK filesystem wrapper and timestamp provenance
  viewer/                  Bounded hex, text, signature, and preview preflight
  evidence-manager/        SQLite schema, migrations, evidence records
  extraction/              Safe path construction and file export
  job-manager/             Tokio jobs, progress, cancellation, and snapshots
  reporting/               JSON, CSV, HTML report models and renderers
  audit-log/               Append-only hash-chained event log
  native-bindings/         C ABI, bindgen wrappers, worker RPC
ui/
  qml/                     Screens, dialogs, wizards, panels
  components/               Reusable QML controls and delegates
  resources/                Icons, themes, translations, resources.qrc
tests/
  fixtures/                 Generator definitions and manifests
  integration/              Cross-crate tests
  fuzz/                     Rust and protocol fuzz targets
  differential/             Tool comparison harnesses
docs/
packaging/
scripts/
```

`apps/service` and `apps/worker` are added to the suggested layout because the
threat model requires a process boundary between the UI and native parsers.

## 7. Core domain model

The core uses newtypes instead of raw strings and integers for identifiers.

```rust
pub struct CaseId(Id);
pub struct EvidenceId(Id);
pub struct SourceId(Id);
pub struct ArtifactId(Id);
pub struct JobId(Id);

pub struct ByteRange {
    pub offset: u64,
    pub length: u64,
}

pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Partial,
    Failed,
    Cancelled,
}

pub enum VerificationOutcome {
    Match,
    Mismatch,
    NotAvailable,
    Error,
}
```

All byte arithmetic uses `checked_add`, `checked_mul`, and explicit conversion
from `u64` to platform offsets. A `ByteRange` is always validated against the
source length before use.

### Error model

Every failure has:

- Stable machine-readable code.
- Human-readable summary.
- Operation and phase.
- Optional source range or artifact identifier.
- Whether retrying can help.
- Whether output is partial.
- Underlying parser detail, retained in the audit log but not rendered as rich
  text.

The ABI must not expose Rust trait objects, references with unstated lifetimes,
or native library ownership rules.

## 8. C ABI

`native-bindings` exports a C header generated from Rust source. All exported
functions:

- Accept an explicit ABI version.
- Validate pointer and length pairs.
- Treat zero length as a valid empty slice only where documented.
- Return an `ofi_status` code and allocate errors through an owned error handle.
- Never unwind across the ABI boundary.
- Document ownership and release functions.

Initial interface families:

```c
ofi_status ofi_runtime_create(const ofi_runtime_config*, ofi_runtime**);
void ofi_runtime_destroy(ofi_runtime*);

ofi_status ofi_case_create(ofi_runtime*, const uint8_t* request, size_t,
                           uint8_t** response, size_t*);
ofi_status ofi_evidence_add(ofi_runtime*, const uint8_t* request, size_t,
                            uint8_t** response, size_t*);

ofi_status ofi_job_start(ofi_runtime*, const uint8_t* request, size_t,
                         ofi_job_id*);
ofi_status ofi_job_cancel(ofi_runtime*, ofi_job_id);
ofi_status ofi_job_snapshot(ofi_runtime*, ofi_job_id,
                            uint8_t** response, size_t*);

void ofi_bytes_free(uint8_t*, size_t);
void ofi_error_free(ofi_error*);
```

The Qt bridge sends Serde-encoded request and response envelopes. Versioned
schemas are kept in `docs/specifications/`. Binary evidence pages use a
separate bounded frame API to avoid repeated base64 expansion.

## 9. Worker RPC

Control messages are length-prefixed, versioned, bounded JSON with a maximum
message size. Large binary pages use explicit payload frames with a maximum
frame size. The protocol supports:

- Open image and return immutable image metadata.
- Read a byte range.
- Read a bounded page for hex or text preview.
- Enumerate partitions.
- Open a filesystem at a partition offset.
- List a directory page.
- Read file metadata.
- Read a file byte range.
- Verify image.
- Begin or resume an acquisition.
- Cancel a job.
- Emit progress, warning, error, and integrity observations.

The service rejects unknown protocol versions, duplicate frames, out-of-order
sequences, oversized messages, and responses that exceed the declared payload
length.

## 10. Persistence

SQLite is used for case metadata and queryable inventory. It stores no evidence
image content.

Core tables:

```text
cases
evidence
sources
artifacts
acquisitions
hashes
verification_runs
partition_tables
partitions
filesystem_volumes
examinations
extractions
report_artifacts
job_history
```

Database rules:

- `PRAGMA foreign_keys = ON`.
- WAL mode for concurrent reads.
- `synchronous = FULL` for evidence-state commits.
- One service writer for state-changing transactions.
- Migrations are forward-only and tested against representative case files.
- All timestamps are stored as integer Unix nanoseconds plus explicit timezone
  provenance. Human-readable fields are derived, not authoritative.
- Source and artifact paths are stored as display metadata. Security decisions
  use open handles and capability-relative paths.

The case database is not the audit log. Losing or tampering with query indexes
must not silently rewrite examination history.

## 11. Audit log

The audit log is an append-only JSON Lines file:

```json
{
  "sequence": 42,
  "recorded_at_utc": "2026-10-30T12:34:56.123456789Z",
  "case_id": "...",
  "actor": "examiner",
  "action": "acquisition.completed",
  "subject_id": "...",
  "details": {},
  "previous_hash": "...",
  "record_hash": "..."
}
```

The record hash is SHA-256 over a canonical encoding of the record excluding
`record_hash`, including `previous_hash`. Records are appended with an exclusive
file lock, flushed, and synchronized before the operation is reported as
committed. The current chain head is also stored in the case database for quick
detection of truncation, but the JSONL file is authoritative.

A local hash chain detects accidental or unsophisticated modification. It does
not prevent an attacker with full control of the directory from rewriting the
whole chain. Reports record the final chain digest; an examiner can copy that
digest to an external system for stronger tamper evidence.

## 12. Image reader

The common interface exposes:

- Logical image length.
- Logical block size.
- Format and segment inventory.
- Random-access reads with checked ranges.
- Stored image metadata.
- Stored hashes, if any.
- Corruption or validation findings.

### RAW

Phase 1 uses direct read-only file I/O and `pread`. It never memory-maps the
whole image. A RAW image has no embedded stored hash, segment table, or
authoritative metadata beyond its bytes and optional OpenForensic manifest.

### E01/EWF

Phase 2 wraps a pinned libewf build. Configure media size, bytes per sector,
sectors per chunk, format, compression, maximum segment size, header values, and
stored MD5/SHA-1 before opening for write. Finalize before close. On read,
validate segment inventory, handle corruption state, and expose libewf metadata.

libewf handles are not assumed thread-safe. The first implementation serializes
access per handle or uses a bounded pool of read-only handles. This decision is
tested before enabling concurrent filesystem workers.

## 13. Acquisition pipeline

1. Validate the source path and destination relationship.
2. Open source `O_RDONLY`, record device or file identity, size, block size,
   read-only flag, and source metadata.
3. Refuse a mounted writable block device unless the examiner deliberately
   proceeds with an explicit warning and audit entry. The recommended workflow
   is an external write blocker.
4. Create a staging directory on the destination filesystem.
5. Write a job manifest with parameters and expected output names.
6. Stream source data in bounded chunks. Hash source bytes and output bytes
   with independent streaming hash contexts.
7. For a read error, record the exact failed range, retry a bounded number of
   times if configured, then write a documented zero-fill chunk and mark the
   acquisition partial. Never hide the error.
8. Finalize RAW or E01 output.
9. Flush and synchronize artifacts.
10. Reopen output read-only and verify its size, stored hashes, and independent
    hashes.
11. Write the final manifest, render the acquisition and verification reports,
    and append the completion record.
12. Rename or mark the staged set as committed. A missing commit manifest means
    the result is not a completed acquisition.

Hash scope is explicit:

- Source-read hash: bytes successfully read from the source, with zero-fill
  policy recorded for unreadable ranges.
- Output-logical hash: full logical output stream, including zero-filled ranges.
- Stored-format hash: hash value embedded by libewf, if present.
- Independent SHA-256 and SHA-512 always cover the output logical byte stream.

An unreadable source plus zero-fill cannot produce a truthful full-source hash.
The result is `Partial` and the report states the limitation.

## 14. Verification pipeline

Verification opens the image read-only, validates all segments, computes the
requested independent hashes, retrieves embedded hashes when available, and
returns one result per requested hash:

- `Match`.
- `Mismatch`.
- `NotAvailable`.
- `Error`.

The UI must distinguish "stored hash unavailable" from "matching hash". A
successful acquisition is not reported until the generated image passes its
configured verification policy.

## 15. Partition and filesystem analysis

The partition analyzer wraps TSK volume systems for MBR and GPT. It records:

- Table type and offset.
- Slot and partition identifier.
- Start sector, length in sectors, byte offset, and byte length.
- Type code or GUID.
- Allocation and metadata flags.
- Filesystem detection result.
- Overlap, gap, out-of-range, and validation findings.

The filesystem layer wraps TSK and returns a neutral tree model:

- Metadata address.
- Name and parent.
- Allocation status.
- File type and size.
- Attributes and alternate streams.
- Timestamp values with source format, raw precision, interpretation status, and
  timezone assumption.
- Content-read handle or failure.
- Deleted and orphan status.

Directory enumeration is paged. The model does not build an unbounded full-tree
in memory.

### Timestamp provenance

The core time value has two parts:

```rust
pub struct EvidenceTimestamp {
    pub raw_value: Option<u64>,
    pub utc_seconds: Option<i64>,
    pub nanos: Option<u32>,
    pub source_semantics: TimestampSemantics,
    pub timezone_assumption: Option<TimezoneAssumption>,
    pub validity: TimestampValidity,
}
```

Source semantics include at least:

- `Utc`.
- `LocalWallClockUnknownZone`.
- `ExplicitUtcOffset`.
- `Unknown`.

The system event time is always UTC. Evidence timestamps are not relabeled UTC
when the source format does not provide a unique UTC instant. Reports include
the interpretation and any examiner-supplied timezone.

## 16. Viewer and extraction

The hex viewer requests 4 KiB pages through a paged model. The text viewer reads
a bounded prefix and rejects or truncates decoding that exceeds configured
limits. Image preview checks dimensions and decoded byte budgets before
allocating a `QImage`. PDF preview is optional, bounded, and has no JavaScript or
network access.

Extraction uses a capability root:

1. Create a new output directory or require an empty one.
2. Open the root with `O_DIRECTORY | O_NOFOLLOW`.
3. For each path component, reject empty, `.`, `..`, NUL, separator injection,
   and overlong names.
4. Create each entry through `openat2` with `RESOLVE_BENEATH`,
   `RESOLVE_NO_MAGICLINKS`, and `RESOLVE_NO_SYMLINKS`.
5. Create regular files with `O_CREAT | O_EXCL | O_NOFOLLOW` and owner-only
   permissions.
6. Never restore setuid, setgid, device, socket, or writable executable bits.
7. Store the original evidence name and the output name mapping in the
   extraction report.
8. Hash each extracted file and record its metadata address, size, source
   allocation state, and file signature.

Name conflicts fail by default. Rename, skip, and suffix strategies are explicit
job options and are recorded.

## 17. Desktop UI

The desktop shell is a C++ `QMainWindow` hosting QML quick widgets in dockable
panes. This provides native menus, toolbars, status bar, keyboard shortcuts, and
window management while keeping the inspection surfaces in QML.

Primary layout:

```text
+-----------------------------------------------------------------------+
| Menu bar                                                              |
| Toolbar: Add Evidence | Acquire | Verify | Export | Tasks | Theme     |
+----------------------+-------------------------------+----------------+
| Evidence Tree        | File List                     | Properties     |
|                      |                               |                |
| Physical Devices     |                               | Source         |
| Logical Volumes      |                               | Metadata       |
| Disk Images          |                               | Hashes         |
| Partitions           |                               | Timestamps     |
| Filesystem           |                               |                |
+----------------------+-------------------------------+----------------+
| Viewer tabs: File | Hex | Text | Image | Metadata | Report            |
+-----------------------------------------------------------------------+
| Status | Job progress | Partial/error indicator | Source read-only     |
+-----------------------------------------------------------------------+
```

Models:

- `EvidenceTreeModel`: lazy tree, stable node IDs, `canFetchMore/fetchMore`.
- `FileListModel`: table model for the selected directory page.
- `HexPageModel`: fixed-size page cache around the selected offset.
- `PropertiesModel`: key/value groups with provenance badges.
- `TaskListModel`: active, completed, partial, failed, and cancelled jobs.

All model mutations occur on the GUI thread. Worker events enter through a
queued bridge.

Themes:

- Dark and light tokens share semantic names.
- Contrast is tested for normal and large text.
- Focus indicators are visible.
- No color-only status encoding.
- Reduced-motion preference disables nonessential animation.

## 18. Reporting

The reporting crate renders structured report data to JSON, CSV, and HTML. The
Qt layer converts the same escaped HTML or a print document to PDF using
`QPdfWriter`.

Each report records:

- Report schema version and creation time in UTC.
- Application version, pinned parser versions, and build hashes.
- Case, evidence, source, and artifact identifiers.
- Hash algorithm, scope, result, and available or missing status.
- Timestamp interpretation and timezone assumptions.
- Acquisition parameters and read-error ranges.
- Partial, failed, and cancelled operation states.
- Audit log chain head.

Evidence strings are always escaped for the destination format. CSV cells guard
against formula injection. HTML never embeds active evidence content as markup.

## 19. Build and dependency policy

- Rust toolchain pinned in `rust-toolchain.toml`.
- Cargo dependencies pinned in `Cargo.lock` and audited before release.
- Native forensic dependencies pinned by tag and commit or source hash.
- CMake builds TSK, libewf, and optional libyal libraries with recorded flags.
- Bindgen uses vendored headers matching the built binaries.
- One lockfile and one native dependency manifest drive every package.
- Reports include exact component versions.

The initial native dependency policy is:

- TSK: reviewed 4.15.0 or a pinned later stable commit.
- libewf: reviewed source commit, initially for EWF1 E01 read/write.
- Optional libyal libraries: build only after their feature tests and
  differential coverage exist.

## 20. Packaging

### DEB

Primary release package for Ubuntu and Debian. It can depend on a matching
OpenForensic native dependency package or bundle them under a private prefix.
Because Ubuntu and Debian package versions diverge, private pinned libraries are
preferred for forensic reproducibility.

### AppImage

Bundles Qt and the pinned native libraries. It runs without installation but
does not add a sandbox. Physical acquisition permissions come from the host.

### Flatpak

Suitable for analysis if evidence is granted through the document portal and the
worker can operate without broad device access. Physical acquisition requires
`--device=all` or equivalent and careful permission documentation. It must not
be presented as a default acquisition deployment until tested.

## 21. Quality attributes

| Attribute | Design mechanism |
|---|---|
| Integrity | Read-only source handles, staged outputs, independent hashes, verification before success |
| Reproducibility | Pinned dependencies, versioned schemas, deterministic fixture generation, recorded parameters |
| Availability | Worker isolation, bounded resources, cancellation, durable job state |
| Performance | Streaming I/O, paged models, bounded caches, no whole-image memory maps |
| Auditability | Append-only log, report provenance, explicit partial states |
| Security | Seccomp, Landlock, `openat2`, no auto-execution, bounded parsing |
| Accessibility | Native Qt controls, keyboard navigation, focus states, themes |

