# OpenForensic Imager Requirements

Status: Proposed for Phase 0 review.

Normative words `MUST`, `MUST NOT`, `SHOULD`, and `MAY` are used in the usual
requirements sense. Every release claim requires executable evidence.

## 1. Supported release 0.1 scope

### Image formats

| Format | Read | Write | Verification | Notes |
|---|---|---|---|---|
| RAW/DD single file | Required | Required | Independent hashes | Phase 1 read, Phase 2 write |
| RAW/DD split files | Required | Optional | Independent hashes | Format detection must be explicit |
| EWF1 E01 | Required | Required | Independent hashes plus embedded MD5/SHA-1 when present | Phase 2, pinned libewf |
| EWF1 S01 | Optional | No | If read support passes tests | Not a release blocker |
| EWF2 Ex01 | Experimental read only | No | If tests pass | Not a release blocker |
| Encrypted EWF | No | No | N/A | Explicitly unsupported |
| AD1, L01, AFF | No | No | N/A | Explicitly unsupported |
| VHD, VMDK | Optional later | No | N/A | Requires separate backend tests |

### Volume systems and filesystems

| Item | Read | Write | Initial status |
|---|---|---|---|
| MBR | Required | No | TSK wrapper |
| GPT | Required | No | TSK wrapper |
| NTFS | Required | No | TSK wrapper, NTFS-specific limits documented |
| FAT32 | Required | No | TSK wrapper, timestamp assumption required |
| exFAT | Required | No | TSK wrapper, timestamp assumption required |
| ext4 | Required | No | TSK wrapper, timestamp limits documented |

No evidence filesystem is mounted read-write by the application.

## 2. Evidence Manager

| ID | Requirement | Priority |
|---|---|---|
| EM-001 | The application MUST create, open, and close investigation cases stored in SQLite. | Must |
| EM-002 | Each case MUST have a stable case identifier, title, examiner, creation time, and schema version. | Must |
| EM-003 | The application MUST add evidence sources with a unique evidence identifier and source description. | Must |
| EM-004 | The application MUST record source identifiers including device path, serial or volume identifier when available, capacity, sector size, source type, and observation time. | Must |
| EM-005 | The application MUST preserve the source identifier observed at acquisition and must not overwrite it with later enumeration data. | Must |
| EM-006 | The application MUST record acquisition start, end, parameters, tool version, parser versions, and output artifact identities. | Must |
| EM-007 | Examination history MUST be persisted independently of mutable evidence display names. | Must |
| EM-008 | Source and artifact paths are display metadata and MUST NOT be used as security authority during extraction or reopening. | Must |
| EM-009 | Evidence state changes MUST be transactional and MUST retain prior values in the audit log. | Must |
| EM-010 | The application MUST distinguish successful, partial, failed, and cancelled jobs. | Must |

## 3. Disk acquisition

| ID | Requirement | Priority |
|---|---|---|
| ACQ-001 | The application MUST support physical block-device acquisition when the operating system grants read access. | Must |
| ACQ-002 | The application MUST support logical volume or file-source acquisition. | Must |
| ACQ-003 | The application MUST support RAW/DD output. | Must |
| ACQ-004 | The application MUST support EWF1 E01 output with configurable compression and segment size. | Must |
| ACQ-005 | The source MUST be opened with read-only flags and MUST NOT be opened by the application with write access. | Must |
| ACQ-006 | The acquisition workflow MUST warn that an external hardware write blocker is required for defensible acquisition of untrusted media. | Must |
| ACQ-007 | The application MUST detect whether a source block device is mounted or has a writable mount and MUST require an explicit, audited decision before reading it. | Must |
| ACQ-008 | The application MUST NOT automatically mount source evidence. | Must |
| ACQ-009 | The acquisition output MUST be written to a staging location with a manifest. | Must |
| ACQ-010 | A completed acquisition MUST have a commit marker written only after successful finalization, flush, and verification. | Must |
| ACQ-011 | Cancellation MUST stop new reads, finalize or abandon output safely, retain partial artifacts, and record `cancelled`. | Must |
| ACQ-012 | Unreadable source ranges MUST be recorded exactly and included in the acquisition report. | Must |
| ACQ-013 | Zero-filling an unreadable range MUST mark the acquisition `partial`; zero-fill MUST NOT be represented as a successful source read. | Must |
| ACQ-014 | The application MUST record read retry policy, timeout, block size, buffer size, and hash algorithms. | Must |
| ACQ-015 | The progress UI MUST show bytes read, bytes written, elapsed time, throughput, current phase, and warnings without blocking the GUI. | Must |
| ACQ-016 | The output MUST NOT be labeled successful if verification is required and fails. | Must |
| ACQ-017 | Segment size and compression values MUST be validated against the selected image format and available space. | Must |
| ACQ-018 | A source and destination on the same writable block device MUST be rejected unless an explicit test-only override is enabled and audited. | Must |
| ACQ-019 | Acquisition MUST use bounded buffers and MUST NOT load the source into memory. | Must |
| ACQ-020 | Every acquisition completion test MUST pair successful creation with an independent verification. | Must |

## 4. Hashing and verification

| ID | Requirement | Priority |
|---|---|---|
| HASH-001 | The hashing engine MUST support MD5, SHA-1, SHA-256, and SHA-512. | Must |
| HASH-002 | Hashing MUST be streaming and bounded in memory. | Must |
| HASH-003 | Hashing MUST support image data, source data, and extracted files. | Must |
| HASH-004 | Each hash record MUST identify algorithm, scope, byte count, source-unreadable policy, and calculation time. | Must |
| HASH-005 | Verification MUST compare computed values against explicitly supplied expected values. | Must |
| HASH-006 | An unavailable expected value MUST produce `NotAvailable`, never `Match`. | Must |
| HASH-007 | Verification MUST support hashes embedded in E01 when the pinned libewf version exposes them. | Must |
| HASH-008 | Verification MUST compute independent SHA-256 and SHA-512 over the image logical byte stream. | Must |
| HASH-009 | Verification MUST validate segment inventory, expected lengths, and reported corruption status. | Must |
| HASH-010 | A mismatch MUST be a prominent failure with the expected and computed values recorded. | Must |
| HASH-011 | Hash strings in reports MUST include algorithm names and MUST NOT be truncated in machine-readable output. | Must |

## 5. Image reader

| ID | Requirement | Priority |
|---|---|---|
| IMG-001 | The image reader MUST identify supported formats by content, not by extension alone. | Must |
| IMG-002 | RAW images MUST support random-access reads with checked ranges. | Must |
| IMG-003 | E01 images MUST support segment discovery, metadata inspection, and random-access reads through libewf. | Must |
| IMG-004 | EWF segment sets MUST be validated as a coherent set before being treated as complete. | Must |
| IMG-005 | Missing, truncated, mismatched, or corrupt segments MUST produce explicit errors. | Must |
| IMG-006 | The reader MUST never memory-map or allocate the complete image length. | Must |
| IMG-007 | The reader MUST expose stored metadata and stored hashes without presenting them as independently verified. | Must |
| IMG-008 | Concurrent read safety MUST be documented and tested for each backend. | Must |
| IMG-009 | Unsupported formats MUST fail with a stable error code and no data corruption. | Must |
| IMG-010 | Image metadata MUST include format, logical length, sector size, segment count, parser version, and validation status. | Must |

## 6. Partition analysis

| ID | Requirement | Priority |
|---|---|---|
| PART-001 | The application MUST identify MBR and GPT volume systems. | Must |
| PART-002 | Partition listings MUST include slot, type, start sector, sector count, byte offset, byte length, and flags. | Must |
| PART-003 | Partition offsets and lengths MUST be validated against image length with checked arithmetic. | Must |
| PART-004 | GPT primary and backup headers SHOULD be checked when both are present. | Should |
| PART-005 | Overlapping, out-of-range, malformed, and unsupported partition entries MUST be reported. | Must |
| PART-006 | Filesystem identification SHOULD attempt supported filesystems at each valid partition offset. | Should |
| PART-007 | Partition results MUST be compared with `mmls` and a second partition tool in differential tests. | Must |

## 7. Filesystem exploration

| ID | Requirement | Priority |
|---|---|---|
| FS-001 | The application MUST browse NTFS, FAT32, exFAT, and ext4 read-only through pinned TSK. | Must |
| FS-002 | Directory listings MUST be paged or lazily loaded with bounded memory. | Must |
| FS-003 | File records MUST include name, parent, metadata address, allocation state, file type, size, and timestamps where available. | Must |
| FS-004 | Alternate data streams and named attributes MUST be represented separately where the filesystem and TSK expose them. | Must |
| FS-005 | Deleted names and unallocated metadata MUST be labeled distinctly from available-name deleted files and orphan files. | Must |
| FS-006 | Recovery availability MUST be described as dependent on retained metadata and data blocks. | Must |
| FS-007 | Timestamp records MUST preserve source semantics, precision, validity, and timezone assumption. | Must |
| FS-008 | FAT and exFAT wall-clock timestamps MUST NOT be presented as UTC unless an examiner timezone assumption is recorded. | Must |
| FS-009 | NTFS timestamps outside the exact pinned TSK representable range MUST be marked unavailable, not silently displayed as epoch zero. | Must |
| FS-010 | Ext4 timestamp epoch-bit support MUST be tested; unsupported values MUST be marked. | Must |
| FS-011 | Evidence names containing control characters, bidirectional overrides, or invalid UTF-8 MUST be escaped in the UI and reports while retained in metadata. | Must |
| FS-012 | Symlinks, reparse points, device nodes, and special files MUST NOT be followed for extraction. | Must |
| FS-013 | Corrupt filesystem structures MUST produce parser diagnostics and must not panic the UI. | Must |
| FS-014 | Filesystem results MUST be compared against TSK CLI and at least one independent tool where practical. | Must |

## 8. Evidence viewer

| ID | Requirement | Priority |
|---|---|---|
| VIEW-001 | The application MUST provide a hexadecimal viewer with offset, hex bytes, and printable ASCII. | Must |
| VIEW-002 | The hex viewer MUST load bounded pages and support seeking within large files and images. | Must |
| VIEW-003 | The application MUST provide bounded text preview with encoding detection. | Must |
| VIEW-004 | The application MUST provide metadata and properties views. | Must |
| VIEW-005 | The application MUST identify common file signatures by content. | Must |
| VIEW-006 | Image preview MUST validate dimensions and decoded byte budgets before allocation. | Must |
| VIEW-007 | PDF preview MUST be optional, bounded, and run without JavaScript or network access. | Must |
| VIEW-008 | Untrusted evidence MUST never be executed or automatically opened in an external application. | Must |
| VIEW-009 | Hyperlinks and markup from evidence MUST NOT be rendered as active content. | Must |
| VIEW-010 | Preview failures MUST be explicit and MUST NOT alter the source. | Must |

## 9. Extraction

| ID | Requirement | Priority |
|---|---|---|
| EXP-001 | The application MUST export an individual file and a directory tree. | Must |
| EXP-002 | The export destination MUST be a new or explicitly empty owner-controlled directory. | Must |
| EXP-003 | Paths MUST be constructed with `openat2` beneath a directory descriptor using `RESOLVE_BENEATH`, `RESOLVE_NO_MAGICLINKS`, and `RESOLVE_NO_SYMLINKS`. | Must |
| EXP-004 | Absolute paths, `..`, NUL, separator injection, overlong components, and symlink traversal MUST be rejected. | Must |
| EXP-005 | The application MUST NOT restore setuid, setgid, device, socket, FIFO, or writable executable modes. | Must |
| EXP-006 | Existing files MUST NOT be overwritten by default. | Must |
| EXP-007 | Conflict behavior MUST be explicit and recorded: fail, skip, rename, or suffix. | Must |
| EXP-008 | Each exported file MUST be hashed after writing and reopened for length validation. | Must |
| EXP-009 | Extraction metadata MUST include source evidence, partition, filesystem, metadata address, original name, output path, size, allocation state, timestamps, hashes, and result. | Must |
| EXP-010 | Extraction MUST never write to or modify the source image. | Must |
| EXP-011 | Resource limits MUST stop extraction when file count, total bytes, output size, or path depth exceeds configured policy. | Must |

## 10. Reporting

| ID | Requirement | Priority |
|---|---|---|
| RPT-001 | The application MUST generate acquisition, verification, evidence inventory, extraction, and examination activity reports. | Must |
| RPT-002 | Reports MUST be available in JSON, CSV, and HTML. | Must |
| RPT-003 | PDF reports MUST be produced from the same structured report model with bounded escaped content. | Must |
| RPT-004 | All system event times MUST be UTC and include an explicit offset or `Z`. | Must |
| RPT-005 | Evidence timestamp interpretation and timezone assumptions MUST be documented in each affected report. | Must |
| RPT-006 | Reports MUST include application and pinned parser versions. | Must |
| RPT-007 | Reports MUST include success, partial, failed, and cancelled states. | Must |
| RPT-008 | CSV output MUST neutralize spreadsheet formula injection. | Must |
| RPT-009 | HTML output MUST escape all evidence-controlled strings. | Must |
| RPT-010 | Reports MUST include the audit chain head observed at generation time. | Must |

## 11. Audit log

| ID | Requirement | Priority |
|---|---|---|
| AUD-001 | Examination activity MUST be appended to an audit log with monotonically increasing sequence numbers. | Must |
| AUD-002 | The log MUST support only append operations through the application. | Must |
| AUD-003 | Each record MUST include sequence, UTC time, actor, action, subject, details, previous hash, and record hash. | Must |
| AUD-004 | Append MUST acquire an exclusive lock and synchronize the file before reporting commit. | Must |
| AUD-005 | Audit records MUST NOT include plaintext credentials, key material, or entire evidence file contents. | Must |
| AUD-006 | The application MUST detect sequence gaps and previous-hash mismatches when reading the log. | Must |
| AUD-007 | Reports MUST state that a local hash chain is tamper-evident only against limited modification and not externally notarized. | Must |

## 12. Desktop UX

| ID | Requirement | Priority |
|---|---|---|
| UI-001 | The desktop MUST provide a menu bar, toolbar, evidence tree, file list, viewer, properties panel, status bar, and background task manager. | Must |
| UI-002 | Panels MUST be resizable; the evidence, properties, and task panels SHOULD be dockable. | Must |
| UI-003 | The evidence tree MUST support lazy loading and remain responsive with large directories. | Must |
| UI-004 | Long-running operations MUST run outside the GUI thread. | Must |
| UI-005 | Every job MUST expose progress, cancel, and task detail. | Must |
| UI-006 | The application MUST provide working dark and light themes. | Must |
| UI-007 | All controls MUST be keyboard reachable with visible focus. | Must |
| UI-008 | Text contrast MUST meet WCAG AA for normal and large text. | Must |
| UI-009 | Empty, loading, partial, error, and cancelled states MUST be designed for every data view. | Must |
| UI-010 | A partial acquisition or failed verification MUST be visually distinct from success without relying only on color. | Must |
| UI-011 | Evidence-controlled text MUST be rendered as plain text or escaped markup. | Must |
| UI-012 | Keyboard shortcuts MUST be documented and configurable where practical. | Must |
| UI-013 | The UI MUST not expose write actions that could modify source evidence. | Must |

## 13. Performance and reliability

| ID | Requirement | Priority |
|---|---|---|
| PERF-001 | Acquisition and hashing MUST use bounded streaming buffers. | Must |
| PERF-002 | Random-access image reads MUST avoid re-reading the whole image. | Must |
| PERF-003 | Directory listing memory MUST be bounded by page size and configured cache limits. | Must |
| PERF-004 | Worker processes MUST have configurable memory, file-size, and message limits. | Must |
| PERF-005 | A worker crash or timeout MUST leave the GUI usable and record a failed job. | Must |
| PERF-006 | Performance tests MUST measure acquisition throughput, hash throughput, image-open latency, directory latency, memory, and CPU. | Must |
| PERF-007 | Performance results MUST record hardware, storage, image size, block size, parser versions, and cache state. | Must |
| PERF-008 | The application MUST NOT claim a performance result that was not measured by the test harness. | Must |

## 14. Security and integrity

| ID | Requirement | Priority |
|---|---|---|
| SEC-001 | All evidence MUST be treated as untrusted input. | Must |
| SEC-002 | Native forensic parsers MUST run in an isolated worker process. | Must |
| SEC-003 | The application MUST open sources read-only and MUST NOT issue write, format, partition, or filesystem-repair operations against evidence. | Must |
| SEC-004 | The application MUST use checked arithmetic for offsets, lengths, counts, and conversions. | Must |
| SEC-005 | The application MUST enforce file count, directory depth, name length, message size, page size, preview size, and output size limits. | Must |
| SEC-006 | The application MUST reject decompression or decoded-output expansion beyond configured budgets. | Must |
| SEC-007 | The application MUST detect concurrent source modification during image verification and report the result as `Error` or `Mismatch`, not success. | Must |
| SEC-008 | The application MUST use `O_CLOEXEC`, `O_NOFOLLOW` where appropriate, and avoid shell interpretation of evidence names. | Must |
| SEC-009 | The application MUST not run as root for analysis work. Physical device access SHOULD use documented group membership or a minimal privilege helper. | Should |
| SEC-010 | Security checks MUST be covered by tests and fuzz targets. | Must |

## 15. Unsupported and deferred behavior

- FTK Imager UI compatibility.
- FTK Imager project, case, or report file compatibility.
- EWF encrypted images.
- EWF2 write.
- Writing AD1, L01, AFF, S01, VHD, or VMDK.
- Filesystem repair or write support.
- Automatic mounting with write access.
- Memory acquisition.
- Mobile extraction.
- Windows or macOS desktop releases.
- Claims of complete deleted-file recovery.
- A software-only hardware write-blocker replacement.

