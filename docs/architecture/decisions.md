# Architecture Decisions

Status: Proposed for Phase 0 review.

## ADR-001: Pin native forensic libraries

Context: Ubuntu 26.04 ships TSK 4.12.1 and libewf 20140816. Debian, Arch, and
Fedora package different versions. libewf is marked experimental upstream and
the newest release is a pre-release.

Decision: Build and ship reviewed TSK and libewf versions from pinned source.
Record source hashes and build flags. Do not silently fall back to an arbitrary
distribution version.

Consequences:

- More release engineering work.
- Consistent parser behavior across supported distributions.
- Larger AppImage and DEB payloads unless a separate native dependency package
  is used.

## ADR-002: Rust core with a narrow C ABI

Context: The required stack combines Rust core libraries with Qt 6 and QML.

Decision: Expose the Rust service through a versioned C ABI. Generate the C
header from Rust. Keep ownership and error rules explicit.

Consequences:

- Stable ABI boundary.
- No Rust types leak into QML.
- Request and response schema versioning is required.

## ADR-003: The Sleuth Kit is the primary filesystem and partition engine

Context: TSK is mature and supports the required initial filesystems and
partition tables.

Decision: Wrap TSK volume and filesystem APIs rather than reimplementing MBR,
GPT, NTFS, FAT, exFAT, or ext4 parsers. Optional libfsntfs and libyal libraries
provide differential or specialized capabilities.

Consequences:

- Parser defects remain dependent on upstream.
- Timestamp and extent semantics must be adapted into an explicit provenance
  model.
- Version pinning is mandatory.

## ADR-004: Isolate native parsers in a worker process

Context: Evidence is untrusted and native forensic libraries can contain
memory-safety defects.

Decision: The GUI and service process never call TSK or libewf directly. A
worker process parses evidence under seccomp, Landlock, resource limits, and a
watchdog.

Consequences:

- More IPC complexity.
- Better fault containment and testability.
- Worker startup overhead must be measured.

## ADR-005: Staged acquisition with a commit manifest

Context: Cancelled, crashed, or partially written output must never look
successful.

Decision: Write output into a staging directory, synchronize and verify it, and
commit by writing a final manifest. A missing commit manifest means the set is
not a completed acquisition.

Consequences:

- Requires destination space accounting.
- Recovery can identify partial output.
- Rename semantics across filesystems must be checked.

## ADR-006: Explicit hash scopes

Context: EWF1 stores MD5 and SHA-1. The project requires SHA-256 and SHA-512.
Unreadable sectors make a single source hash ambiguous.

Decision: Store algorithm, scope, byte count, and zero-fill policy for every
hash. Independent SHA-256 and SHA-512 cover the output logical stream. Stored EWF
hashes remain separate.

Consequences:

- Reports are more verbose.
- Partial acquisitions cannot be misrepresented as complete source hashes.

## ADR-007: Evidence timestamp provenance

Context: TSK's FAT/exFAT conversion uses process local time. exFAT UTC offset
handling and ext4 epoch bits are not preserved by the current source path used
for conversion. NTFS conversion has range limits.

Decision: Do not normalize all evidence timestamps to UTC. Store raw or
parser-derived values with source semantics, validity, and timezone assumptions.
System event times remain UTC.

Consequences:

- More complex UI and report schemas.
- Examiners get truthful uncertainty instead of false precision.

## ADR-008: Capability-based extraction

Context: Evidence names can contain traversal sequences, symlinks, control
characters, or host-invalid names.

Decision: Use `openat2` beneath an owner-controlled directory descriptor with
`RESOLVE_BENEATH`, `RESOLVE_NO_MAGICLINKS`, and `RESOLVE_NO_SYMLINKS`. Never
follow evidence links or overwrite existing files by default.

Consequences:

- Linux 5.6 or newer is required. Ubuntu 26.04 satisfies this.
- Other platforms, if added, require an equivalent capability model.

## ADR-009: Append-only local audit chain

Context: The product requires an append-only examination log.

Decision: Use hash-chained JSONL with exclusive locking and fsync. The case
database holds a chain head but is not authoritative.

Consequences:

- Local tampering is detectable in common accidental cases.
- Strong tamper evidence requires external recording of the chain head.

## ADR-010: EWF1 E01 is the initial EWF scope

Context: libewf upstream lists experimental status and incomplete EWF2 work.

Decision: Release 0.1 creates and verifies EWF1 E01 only. EWF2 read may be
experimental after tests. Encrypted EWF and EWF2 write are unsupported.

Consequences:

- Clear compatibility boundary.
- EWF2 support can be added without changing the initial acceptance criteria.

