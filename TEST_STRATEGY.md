# OpenForensic Imager Test Strategy

Status: Proposed for Phase 0 review.

## 1. Principles

- Every capability claim needs an executable test.
- Every successful acquisition has a corresponding verification test.
- Synthetic evidence is generated from documented sources, never copied from
  production.
- Tests distinguish parser success from semantic correctness.
- Corrupt, partial, unavailable, and unsupported results are first-class test
  outcomes.

## 2. Test layers

| Layer | Scope | Tooling |
|---|---|---|
| Unit | IDs, ranges, errors, hashes, state machines, path validation, report escaping | Rust test, `proptest` |
| Integration | RAW read, acquisition, E01 round trip, SQLite, audit, extraction, reports | Rust integration tests |
| Native ABI | Null pointers, ABI mismatch, ownership, panic containment, callbacks | C harness, C++ bridge tests, sanitizers |
| Worker sandbox | seccomp, Landlock, rlimits, crash isolation, protocol validation | Negative process tests |
| Differential | Partitions, filesystems, hashes, E01, extraction against independent tools | Python or Rust harness |
| Fuzz | Image identification, RPC framing, path components, report fields, preview parsers | `cargo-fuzz` |
| GUI | Navigation, keyboard, themes, job states, large models | Qt Quick Test and accessibility checks |
| Performance | Throughput, latency, memory, CPU | `criterion` plus system benchmarks |
| Packaging | Clean install, library resolution, smoke acquisition | VM or container matrix |

## 3. Synthetic fixture program

The initial fixture manifest covers:

- Empty zero-length image.
- Sparse RAW image.
- Dense RAW image with deterministic pseudorandom bytes.
- Split RAW image with segment boundaries at block and non-block offsets.
- Single-file E01.
- Segmented E01, including more than 100 segments in a reduced-size stress set.
- E01 with MD5 and SHA-1.
- E01 with no stored hash.
- MBR with primary, extended, gaps, overlap, out-of-range entry, and corrupt
  entry.
- GPT with valid primary and backup headers, missing backup, corrupt CRC,
  overlapping entries, and out-of-range entries.
- Empty NTFS, FAT32, exFAT, and ext4 filesystems.
- Files with ASCII, Unicode, very long, control-character, and bidi-override
  names.
- Deleted file metadata with retained and reused data blocks.
- Compressed NTFS files and sparse files.
- Alternate data streams on NTFS.
- Hard links and symbolic links where supported.
- Directories with large entry counts.
- Files larger than 4 GiB in a sparse test set.
- Corrupt EWF headers, invalid chunk tables, bad checksums, missing segments,
  truncated segments, and mismatched segment sets.

Fixtures are generated in `tests/fixtures/generated/`, which is ignored by git.
Committed files contain:

- Generator source.
- Tool version requirements.
- Stable UUID, serial, label, and source-date inputs.
- Expected file inventory.
- Expected SHA-256 hashes for generated artifacts.
- Expected parser summaries.

Large binary fixtures must not be committed. CI regenerates or restores them
from a content-addressed cache. A test must fail when a regenerated fixture does
not match its manifest.

## 4. Determinism

Filesystem generators must use fixed:

- Volume identifiers and UUIDs.
- Filesystem UUIDs and hash seeds.
- Timestamps derived from a fixed source epoch.
- Feature sets and block sizes.
- File contents and names.

If a generator cannot be made byte-for-byte deterministic, it must emit a
canonical semantic manifest instead of an exact binary hash, and the test must
state why.

Test output must not depend on the host timezone or locale. The test runner sets
`TZ=UTC` and `LC_ALL=C.UTF-8` unless a test explicitly exercises timezone
behavior. Timezone-sensitive tests launch a controlled child with an explicit
timezone.

## 5. Feasibility fixture evidence

The Phase 0 exploratory run used small images in `/tmp`. This established that
the required generators are present and that installed TSK can parse the
targeted structures:

| Structure | Generator | Parser result |
|---|---|---|
| ext4 | `mke2fs` | `fsstat -f ext4` and `fls` succeeded |
| NTFS | `mkntfs` | `fsstat -f ntfs` and `fls` succeeded, including orphan entries |
| FAT32 | `mkfs.vfat -F 32` | `fsstat -f fat32` and `fls` succeeded |
| exFAT | `mkfs.exfat` | `fsstat -f exfat` and `fls` succeeded |
| MBR | `sfdisk` | `mmls` matched `sfdisk -d` offsets and lengths |
| GPT | `sgdisk` | `mmls` matched `sgdisk -p` offsets and lengths |
| Deleted ext4 name | `debugfs write` then `debugfs rm` | `fls -rd` retained the deleted name and metadata address |
| Empty image | `truncate -s 0` | `mmls` returned a bounded error, not a crash |

These exploratory images were generated with current time and random
identifiers. Their hashes are not release golden files. See
[docs/testing/fixtures.md](docs/testing/fixtures.md).

## 6. Hash tests

Required vectors and checks:

- Known-answer vectors for MD5, SHA-1, SHA-256, and SHA-512.
- Empty input.
- One byte, block boundary minus one, exact boundary, boundary plus one.
- Multi-gigabyte sparse stream without proportional memory growth.
- Random chunking property test: segmented updates equal one-shot hash.
- Cancellation leaves no partial hash presented as complete.
- Hash scope recorded for source-read, output-logical, stored-format, and
  extracted-file data.
- Corrupted expected hash returns `Mismatch`.
- Missing expected hash returns `NotAvailable`.
- Independent SHA-256 compared with `sha256sum` and OpenSSL.

## 7. Acquisition tests

### RAW

- Acquire a deterministic source file.
- Compare output bytes and SHA-256.
- Confirm source mtime, size, and hash are unchanged.
- Cancel at every state boundary and inspect partial manifest.
- Inject bounded read failures and verify exact range accounting.
- Confirm output is not reported successful before verification.

### E01

- Acquire deterministic source with compression levels and segment sizes.
- Read back with the pinned libewf build.
- Verify with `ewfverify`.
- Inspect with `ewfinfo`.
- Open with TSK and compare filesystem output with the RAW equivalent.
- Compare independent SHA-256 of the logical E01 stream and RAW stream.
- Test single and multi-segment sets.
- Repeat with stored MD5/SHA-1 absent, present, and mismatched.

### Failure behavior

- Destination full.
- Permission denied.
- Source disappears during acquisition.
- Source size changes.
- Read timeout.
- Cancellation.
- Process crash and restart.
- Segment creation failure.
- Finalization failure.
- Output changed after hash calculation.

Each case must yield `failed`, `partial`, or `cancelled` with a corresponding
manifest and audit record.

## 8. Partition differential tests

Compare:

- TSK `mmls` through the wrapper.
- `sfdisk -d` for MBR.
- `sgdisk -p` for GPT.
- `parted -m unit B print` as a third view where useful.

Assertions:

- Table type.
- Entry count.
- Start byte and length.
- Type description or GUID.
- Gaps and unallocated regions.
- Overlap and out-of-range detection.
- Corrupt backup and CRC behavior.

No differential test should compare only the wrapper's string formatting.
Normalize to structured bytes and sectors.

## 9. Filesystem differential tests

| Filesystem | Primary engine | Independent references |
|---|---|---|
| NTFS | TSK | `ntfsls`, `ntfsinfo`, `ntfscat`, libfsntfs tools when enabled |
| FAT32 | TSK | `mdir`, `mcopy`, `fsck.fat -n`, mount only on disposable fixtures |
| exFAT | TSK | `exfat`/exfatprogs tools, `fsck.exfat -n`, mount only on disposable fixtures |
| ext4 | TSK | `debugfs`, `e2fsck -fn`, `dumpe2fs` |

Comparison fields:

- Directory tree and names.
- Allocation state.
- Metadata address.
- File size.
- Alternate streams.
- Timestamps with interpretation status.
- Extracted bytes and hashes.
- Deleted and orphan classification.

Mount-based reference checks use disposable loopback fixtures with read-only
mounts and must be skipped when the test host lacks permission. They must never
run against real evidence.

## 10. Timestamp tests

Dedicated tests cover:

- NTFS timestamps before 1970, normal range, and after the parser limit.
- FAT timestamps at 1980, DST transitions, 2038 boundaries, and invalid fields.
- exFAT timestamps with zero, positive, and negative UTC offsets.
- ext4 pre-1970, post-2038, nanoseconds, and missing extra inode fields.
- Host timezone changes between `UTC`, `Asia/Jakarta`, and a DST zone.
- Report output showing source semantics and assumptions.
- No code path converting an unknown wall clock to UTC silently.

If TSK cannot supply the raw exFAT offset or extended ext4 epoch bits, the test
must record the limitation and the UI must mark the derived time as limited.

## 11. Corruption and fuzz strategy

Seed corpus sources:

- Valid generated fixtures.
- Empty and minimal structures.
- Known boundary values.
- Mutations produced by `cargo-fuzz` and an EWF mutation script.

Fuzz targets:

- Image format identification.
- RAW segment discovery.
- EWF metadata conversion wrapper.
- Worker RPC framing.
- C ABI request decoding.
- Extraction path components.
- Report escaping.
- File-signature identification.

External parser crashes are expected to terminate the worker, not the test
driver. The harness records the exact input hash and minimized reproducer.
Native libraries are also tested under sanitizer builds where supported.

## 12. Security tests

- Attempt to open a source for write through every public path.
- Attempt extraction with `..`, absolute paths, encoded separators, Unicode
  lookalikes, NULs, long components, and symlink swaps.
- Race a symlink replacement between validation and open.
- Verify `openat2` denies traversal above the output root.
- Verify worker network and process creation are denied.
- Verify worker cannot read unrelated files.
- Verify parser crash isolation.
- Verify oversized RPC frames and binary pages are rejected.
- Verify audit sequence and hash-chain tamper detection.
- Verify HTML escaping and CSV formula neutralization.
- Verify memory and output budgets.
- Verify no shell interpretation of evidence names.

## 13. GUI tests

Automated:

- Application startup with empty case.
- Add evidence and lazy tree expansion.
- Keyboard traversal of menus, tree, tabs, properties, and dialogs.
- Focus indicators in dark and light themes.
- Job start, progress, cancel, failure, partial, and completion.
- Verification mismatch presentation.
- Hex viewer page navigation and selection.
- Large fixture model responsiveness.
- Accessible names for controls and images.

Manual accessibility checks:

- Screen reader labels for key workflows.
- High contrast theme.
- 200 percent scaling.
- Reduced motion.
- Multi-monitor dock state.

## 14. Performance benchmarks

Measure and record:

- RAW acquisition throughput at 512 KiB, 1 MiB, 4 MiB, and 16 MiB buffers.
- E01 acquisition throughput by compression level and segment size.
- SHA-256 and SHA-512 throughput.
- Image-open latency for RAW, single E01, and segmented E01.
- First-page and deep-page directory latency.
- Peak RSS for image open, directory listing, hex paging, and preview.
- CPU utilization during acquisition and idle UI.

Each result includes:

- CPU model.
- Memory.
- Storage device and filesystem.
- Kernel.
- Source and destination cache state.
- Application, Rust, Qt, TSK, and libewf versions.
- Fixture manifest hash.

There are no fabricated performance targets in this document. Baseline
acceptance is:

- Memory remains bounded when image size grows.
- Acquisition throughput is compared with the same source read by
  `dd` and libewf command-line tools on the same host.
- Image open does not scan the full image unless format validation requires it.
- The UI remains responsive during all measured jobs.

## 15. Differential-tool inventory

Required on the test host:

- TSK command-line tools.
- `ewfinfo`, `ewfverify`, and `ewfexport` matching the pinned libewf build.
- `sha256sum` or OpenSSL `dgst`.
- `sfdisk`, `sgdisk`, and `parted`.
- `debugfs`, `dumpe2fs`, and `e2fsck`.
- `ntfsls`, `ntfsinfo`, `ntfscat`, and optionally libfsntfs tools.
- FAT and exFAT inspection tools such as `fsck.fat -n`, `mdir`/`mcopy`, and
  `fsck.exfat -n`.

The host currently lacks `ewf-tools`, `mtools`, and several optional libyal
development headers. These must be added to the reproducible test image before
Phase 2 and Phase 3 gates.

## 16. CI gates

### Pull request

- Formatting and lint.
- Unit tests.
- Integration tests that do not require privileged devices.
- Small deterministic fixture tests.
- ABI ownership tests.
- Dependency and license inventory.

### Nightly

- Extended property tests.
- Fuzzing time budget.
- Sanitizer native builds.
- Differential filesystem suite.
- GUI automation.
- Performance trend.

### Release

- Full fixture suite.
- Every successful acquisition paired with verification.
- Corrupt and partial corpus.
- Clean Ubuntu, Debian, Arch, and Fedora package tests.
- Reproducible build check.
- SBOM and pinned dependency digest report.
- No open critical or high security findings.

## 17. Definition of test completion

A feature is complete only when:

- Its requirements map to automated tests.
- A failing test was observed before implementation where TDD applies.
- The final test run is recorded.
- Limitations are documented in the compatibility matrix.
- Relevant differential evidence exists.
- No test result is inferred from a partial or skipped run.

