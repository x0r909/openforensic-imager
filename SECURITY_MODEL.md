# OpenForensic Imager Security Model

Status: Proposed for Phase 0 review.

## 1. Security objectives

The highest-priority objective is evidence integrity. Safety of the examiner,
the host, the case database, and the audit record follows.

The application must protect against:

- Accidental or malicious modification of source evidence.
- Crashes or code execution caused by malformed images or filesystems.
- Case escape through extraction paths.
- Resource exhaustion from image, directory, file, or document content.
- Silent loss of errors or partial-result state.
- Modification or truncation of examination history.
- Exposure of secrets in logs or reports.

## 2. Assets

| Asset | Integrity need | Confidentiality need |
|---|---|---|
| Source evidence | Absolute, never modified | Case-sensitive |
| Acquired images | Cryptographic integrity and provenance | Case-sensitive |
| Case database | Transactional and recoverable | Case-sensitive |
| Audit log | Append-only and tamper-evident | Case-sensitive |
| Reports | Accurate, reproducible, and escaped | Case-sensitive |
| Extracted files | Exact bytes, safe names, safe modes | Case-sensitive |
| Parser configuration | Recorded and reproducible | Low |
| Credentials or keys | Never logged or persisted by default | Critical |

## 3. Adversary assumptions

The application assumes:

- Evidence and filenames are attacker-controlled.
- Image headers, partition tables, filesystems, metadata, and embedded documents
  may be malformed, inconsistent, or intentionally hostile.
- Native parser libraries can contain memory-safety or denial-of-service defects.
- The examiner's destination directory may contain preexisting symlinks or
  files.
- A local attacker may attempt to edit the audit log.
- The host kernel and hardware are not fully trusted for tamper-proof
  attestation.

The application does not assume:

- That source media is attached through a hardware write blocker.
- That a block device is unmounted.
- That the operating system honors advisory locks.
- That every evidence filename is valid UTF-8.
- That an E01 image's stored hash or metadata is authentic.

## 4. Trust boundaries

```text
Untrusted evidence
        |
        v
Isolated worker with read-only grants
        |
        | bounded typed messages
        v
Rust service process and SQLite/audit writers
        |
        | narrow validated C ABI
        v
Trusted desktop process
```

The desktop process is trusted for display but not for parser isolation. The
worker is untrusted in the sense that its native inputs are hostile and its
failure must not compromise the rest of the system. The case directory is
sensitive but not tamper-proof against an attacker with the same OS account.

## 5. Threat and control matrix

| Threat | Required controls | Verification |
|---|---|---|
| Source opened with write permission | Central source opener uses `O_RDONLY | O_CLOEXEC`; no write-capable source handle enters any parser or UI model | Static review and fail-on-write syscall test |
| Source is mounted writable | Parse `/proc/self/mountinfo`, `/sys/block`, device holders, and mount options; require an explicit audited decision | Integration test with loop devices |
| Destination overlays source | Resolve device identity and inode ancestry; reject destination on source device | Unit and integration tests |
| Source changes during acquisition | Record identity, size, block counts, and stat fields; verify before and after reads; final image verification | Concurrent-writer test |
| Path traversal in extraction | Capability root plus `openat2` resolution flags; reject unsafe components and symlinks | Traversal corpus and property tests |
| Symlink or hard-link attack in destination | `O_NOFOLLOW`, `O_EXCL`, `RESOLVE_NO_SYMLINKS`, owner-controlled new directory, device/inode checks | Adversarial directory tests |
| Malicious filename | Escape for UI and reports; plain text rendering; no shell use; bound length; retain original in structured metadata | Unicode and control-character tests |
| Malformed image crashes parser | Run parser in worker; seccomp, Landlock, rlimits, watchdog, protocol validation, core dumps disabled | Corrupt-image corpus and crash tests |
| Memory corruption in TSK or libewf | Worker isolation, no network, minimal filesystem access, no unnecessary privileges, ASLR, NX, RELRO, stack protector | Sanitizer CI and process-escape test |
| Integer overflow in offsets | Checked arithmetic for all offset, length, count, and multiplication operations | Property tests with boundary values |
| Infinite directory recursion | Depth limit, visited metadata/cycle tracking, page budgets, cancellation | Cyclic and deeply nested fixture tests |
| Huge directory or filename | Bounded page size, maximum name bytes, node cache limits, streaming | Large-tree and absurd-name tests |
| Decompression bomb | Declared and observed output budgets; no expanding archive extraction by default; native file-read caps | Compressed NTFS and document tests |
| PDF or image decoder attack | Optional bounded viewer, no JavaScript or network, decode budget, separate worker where feasible | Malicious PDF/image corpus |
| Evidence markup injection | HTML escape, no rich-text interpretation of evidence, strict CSP in generated reports | XSS and CSV formula tests |
| Audit truncation or editing | Append-only API, sequence check, previous-hash chain, fsync, external digest option | Tamper-detection tests |
| Audit race | Exclusive file lock and single writer | Concurrent append test |
| Log secrets | Structured allowlist; never log key material, passwords, file bodies, or full environment | Log redaction test |
| Worker privilege escalation | `PR_SET_NO_NEW_PRIVS`, seccomp deny exec/network, Landlock read-only grants, no root | Sandbox negative tests |
| Resource exhaustion | RLIMIT_AS, RLIMIT_FSIZE, message/page limits, timeouts, cancellation | Limit tests |
| Concurrent case DB access | WAL reads, single writer, transactions, busy timeout, corruption detection | Multi-process tests |
| Supply-chain substitution | Pinned sources and hashes, lockfiles, SBOM, reproducible build instructions | Build provenance test |
| Evidence executed by host | No shell, no auto-open, no external viewer launch | GUI automation and static review |

## 6. Read-only source handling

The application can reduce accidental writes but cannot create the same
guarantee as a hardware write blocker. The product must say this explicitly.

Source open rules:

- Use `openat` or `open` with `O_RDONLY | O_CLOEXEC`.
- Use `O_NOFOLLOW` for ordinary evidence files. Device aliases under
  `/dev/disk/by-id` require a deliberate, validated symlink resolution policy.
- Do not use `O_RDWR`.
- Do not call ioctls that change device state during analysis.
- Do not issue filesystem repair, mount, format, trim, discard, or write-same
  operations.
- Pass only preopened file descriptors or restricted paths to the worker where
  practical.
- Record the device's logical read-only flag and mount state. Do not change them
  silently.

For physical acquisition:

- The UI must recommend an external hardware write blocker.
- The acquisition report must state whether a write blocker was attested and
  how.
- A block device with a writable mount or active holder must require an explicit
  examiner decision and an audit record.
- Running as root is discouraged. If required for device access, privilege
  should be limited to opening the device or provided by group permissions.

## 7. Worker sandbox

The worker process is started with:

- `PR_SET_NO_NEW_PRIVS`.
- A seccomp filter that permits only required read, write, memory, time, signal,
  and local-socket operations. Network and process execution are denied.
- A Landlock ruleset that grants read access to the source and read/write access
  only to the job staging directory.
- `RLIMIT_AS` or a cgroup memory limit.
- `RLIMIT_FSIZE`, file descriptor limits, core dump suppression, and a
  configurable wall-clock timeout.
- No inherited environment beyond a minimal allowlist.

If Landlock is unavailable, the application must fail closed for hostile
evidence analysis or clearly enter a reduced-assurance mode that is recorded in
the report. It must not silently claim isolation.

The worker has no authority to update the case database or final audit log. The
service validates and commits its observations.

## 8. Extraction security

The destination is treated as hostile until opened and validated.

Required sequence:

1. Create a new directory with owner-only permissions or verify an empty,
   owner-controlled directory.
2. Open it with `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`.
3. Resolve each relative path under that descriptor with `openat2` and
   `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS`.
4. Reject absolute paths, `.` and `..` components, empty names, NUL, embedded
   separators, control-only names, and names over policy limits.
5. Create directories with `mkdirat` and files with `O_CREAT | O_EXCL |
   O_NOFOLLOW`.
6. Validate the opened object with `fstat` before writing.
7. Write bytes from the evidence reader only.
8. `fsync` the file and directory, close, reopen read-only, and hash.
9. Record original and sanitized names, policy decisions, and hashes.

The application must never preserve dangerous mode bits or create device nodes,
sockets, FIFOs, hard links, or symlinks in the output tree.

## 9. Malformed input and resource limits

Default limits are policy values, not claims of optimal performance:

| Resource | Default policy |
|---|---|
| Worker control message | 1 MiB |
| Binary RPC page | 4 MiB |
| Hex or text preview request | 64 KiB |
| Image preview pixels | 40 million decoded pixels |
| Maximum directory page | 4096 entries |
| Maximum path depth | 256 components |
| Maximum single name | 4096 UTF-8 bytes |
| Maximum extraction file | Configurable, with a safe default |
| Maximum extraction total | Configurable, bounded by free-space reserve |
| Maximum report rows per page | 100,000, with continuation metadata |
| Parser job timeout | Configurable, no silent infinite wait |

Every limit breach is a typed error or partial result. The parser may not return
a truncated result as if it were complete.

## 10. Time and provenance

Security and forensic accuracy depend on not inventing timestamp certainty.

- System event timestamps are captured from a UTC clock.
- Filesystem timestamps retain source semantics and validity.
- FAT and exFAT wall-clock values require an examiner timezone assumption.
- ExFAT UTC offset handling must be independently tested.
- NTFS values outside the pinned parser's range are marked unavailable.
- Ext4 epoch-bit support is verified, not assumed.
- Reports show both the interpretation and the raw or parser-derived value when
  available.

## 11. Audit-log adversarial model

The hash chain protects against:

- Accidental line modification.
- Deletion or reordering that breaks sequence and previous-hash checks.
- Simple edits that do not recompute the entire chain.

It does not protect against:

- An attacker who can rewrite every record and the database head.
- Filesystem rollback to an earlier consistent chain.
- Compromise of the examiner account or kernel.

Reports must state the chain head and the limitation. Exporting or recording the
head outside the case directory strengthens tamper evidence.

## 12. Privacy and secrets

- The application must not collect telemetry by default.
- Network access is not needed for core evidence analysis.
- Credentials for future encrypted-image support must use memory-protected,
  zeroized buffers and must never enter logs, reports, crash dumps, or audit
  records.
- Evidence paths and names are case-sensitive and may reveal sensitive
  information. Reports should include only fields necessary for the examination.

## 13. Secure defaults

- No auto-mount.
- No auto-execution.
- No external viewer launch.
- No overwrite during extraction.
- No recursive link following.
- No write support for evidence filesystems.
- Partial acquisition remains partial.
- Unavailable hash is not a match.
- Unknown timestamp is not UTC.
- Unsupported format is an error, not a best-effort guess.

## 14. Security acceptance gates

The initial release cannot ship with:

- A known critical or high severity memory-safety defect in an enabled parser
  path.
- Any extraction path traversal.
- Any source write path.
- A worker escape in the sandbox negative test suite.
- Silent audit corruption acceptance.
- A successful acquisition without verification evidence.
- A report claiming unavailable hash or timestamp values were verified.

Required tests include sanitizers, fuzzing, permanent negative tests, and
documented manual checks for controls that cannot be automated.

## 15. Phase 4 extraction status

The extraction crate now opens a newly created owner-only root and resolves
each relative name with `openat2` under `RESOLVE_BENEATH`,
`RESOLVE_NO_SYMLINKS`, and `RESOLVE_NO_MAGICLINKS`. Files are created with
`O_CREAT | O_EXCL | O_NOFOLLOW`, mode `0600`, synced, reopened read-only, and
hashed before success is recorded.

Executable tests cover traversal, absolute paths, destination symlinks,
evidence symlink objects, root-path replacement, conflicts, Unicode and control
characters, per-file and cumulative limits, source immutability, output
permissions, report injection, signature mismatch, invalid source byte counts,
truncated image headers, and decompression-bomb dimensions.

Actual image and PDF rendering is not yet enabled. A renderer must run in the
isolated worker, enforce the same pixel and byte limits, and receive only the
bounded preview plan or staged bytes. High-frequency race stress, fuzzing, and
worker sandbox verification remain Phase 6 gates.

