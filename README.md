# OpenForensic Imager

Linux-native digital forensics acquisition and evidence analysis platform.

## Current status

Phase 0 is complete. Phase 1 core, Phase 2 acquisition/verification, Phase 3
partition/filesystem analysis, Phase 4 viewer/extraction, and Phase 5 desktop
workflows are implemented. Phase 6 has executable security, differential, audit,
fuzz, race, and capacity evidence. Phase 7 has DEB, AppImage, and analysis-only
Flatpak builds with local installation, content, and reproducibility tests.
Phase 8 moves every evidence-facing job into a sandboxed parser worker:

- Rust workspace and domain primitives.
- Streaming MD5, SHA-1, SHA-256, and SHA-512 hashing.
- Read-only RAW image access with range and concurrent-change checks.
- SQLite case and evidence metadata with UTC timestamps.
- Append-only, hash-chained audit log with exclusive writer locking.
- Minimal versioned C ABI for case and evidence requests.
- Deterministic RAW fixture generator.
- Phase 1 end-to-end integration test.
- RAW/DD acquisition with staged output and a commit manifest.
- EWF1 E01 acquisition through libewf, including segmented output.
- Multi-segment E01 random-access reading.
- MD5, SHA-1, SHA-256, and SHA-512 acquisition hashes.
- Post-acquisition verification with explicit match, mismatch, and unavailable
  outcomes.
- Cancellation and unreadable-range accounting with partial status.
- JSON, CSV, and HTML acquisition plus verification reports.
- MBR and GPT partition discovery through TSK.
- Read-only NTFS, FAT32, exFAT, and ext4 filesystem exploration through TSK.
- File metadata, bounded file reads, and deleted-name status.
- Explicit timestamp semantics for UTC and local wall-clock evidence times.
- Package-config discovery of TSK instead of hardcoded host include paths.
- Bounded hex and text previews with encoding detection.
- File-signature identification and image/PDF preflight limits.
- Capability-based extraction with traversal, symlink, conflict, and resource
  protection.
- Source/output SHA-256 verification for every successful export.
- Extraction metadata and JSON, CSV, and HTML reports.
- Tokio background jobs with progress, cancellation, partial outcomes, and
  panic containment.
- C ABI 1.1 job submission, snapshot, cancellation, partition analysis,
  filesystem listing, verification, acquisition, bounded preview, signature
  identification, single-file extraction, and directory extraction reports.
- Qt 6 desktop shell with QML evidence tree, file list, properties, viewer,
  task panel, case/acquisition panel, and working light/dark themes.
- Audit chain verification and JSON, CSV, and HTML audit reports.
- Audit-open preflight before case or evidence database commits.
- Desktop case and evidence writes use an adjacent `audit.jsonl` in the case
  database directory.
- Landlock and seccomp negative tests for a reusable analysis-worker sandbox.
- A bounded `ofi-parser-worker` process for image, partition, filesystem,
  acquisition, verification, preview, signature, and extraction jobs.
- Worker rlimits, Landlock grants, seccomp thread synchronization, response
  bounds, timeout enforcement, crash containment, and cancellation tests.
- Side-effect-free C ABI JSON and viewer fuzz targets plus stable property tests.
- MBR, GPT, ext4, RAW/E01, and SHA-256 differential checks.
- Source-open syscall tracing that proves the acquisition path uses read-only
  source descriptors.
- Repeated extraction-root replacement tests and sparse 8 GiB bounded-memory
  random-access reads.
- Release CMake install rules with a private native-binding directory and
  relocated runtime search path.
- A development DEB with resolved Ubuntu runtime dependencies, desktop entry,
  icon, AppStream metadata, legal notices, and SBOM.
- A portable AppImage with bundled Qt runtime, QML modules, forensic libraries,
  pinned build-tool checksum, and artifact self-test.
- An analysis-only Flatpak with no device access, isolated user installation,
  and runtime self-test.
- Two-build SHA-256 and size comparison for DEB and AppImage using a fixed
  source epoch.

Architecture and review gates:

- [ARCHITECTURE.md](ARCHITECTURE.md)
- [REQUIREMENTS.md](REQUIREMENTS.md)
- [TEST_STRATEGY.md](TEST_STRATEGY.md)
- [SECURITY_MODEL.md](SECURITY_MODEL.md)
- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)

The verified Phase 1 through Phase 6 results are documented in
[docs/testing/phase1-results.md](docs/testing/phase1-results.md),
[docs/testing/phase2-results.md](docs/testing/phase2-results.md), and
[docs/testing/phase3-results.md](docs/testing/phase3-results.md), plus
[docs/testing/phase4-results.md](docs/testing/phase4-results.md),
[docs/testing/phase5-ui-results.md](docs/testing/phase5-ui-results.md),
[docs/testing/phase6-results.md](docs/testing/phase6-results.md),
[docs/testing/phase7-results.md](docs/testing/phase7-results.md), and
[docs/testing/phase8-results.md](docs/testing/phase8-results.md). The detailed
feasibility findings are in [FEASIBILITY_REPORT.md](FEASIBILITY_REPORT.md) and
[docs/reverse-engineering/](docs/reverse-engineering/).

This project is independent. It does not copy FTK Imager source, assets, or
protected implementation details, and it does not claim FTK Imager
compatibility without differential test evidence.

## Feasibility decision

Proceed to Phase 1 after architecture, requirements, security, and test
strategy review.

The production release is technically feasible with these qualifications:

- RAW acquisition is straightforward and can be tested without third-party
  forensic parsers.
- E01 acquisition requires a pinned, tested build of libewf. Ubuntu, Debian,
  Arch, and Fedora currently package old libewf releases, and upstream marks
  the project experimental.
- Partition and filesystem exploration should use The Sleuth Kit. Distribution
  versions differ, so the application must carry or require an exact tested
  version.
- NTFS, FAT32, exFAT, and ext4 are parseable by The Sleuth Kit, but timestamp
  interpretation has material limitations that must be visible in the UI and
  reports.
- Qt 6, QML, Rust, SQLite, CMake, and Ninja are available on the primary host.
- Physical acquisition cannot be made defensible by software alone. An external
  hardware write blocker remains required for untrusted source media.

## Baseline environment

Observed on 2026-10-30:

| Component | Host state |
|---|---|
| OS | Ubuntu 26.04.1 LTS, Linux 7.0.0-38-generic, x86_64 |
| Rust | 1.93.1 |
| Qt | 6.10.2, base and declarative modules installed |
| CMake | 4.2.3 |
| Ninja | 1.13.2 |
| C/C++ | GCC 15.2.0, Clang 21.1.8 |
| Sleuth Kit runtime/tools | 4.12.1 |
| libewf | 20140816 legacy package |
| libfsntfs/libvshadow/libvhdi/libvmdk development headers | Missing |
| EWF command-line tools | `ewfacquire`, `ewfverify`, and `ewfinfo` installed |
| Qt PDF module and Flatpak/AppImage tools | Missing |
| REA | 6.8.0, 229 tools advertised, Hopper available |
| Ghidra | 12.1.4 (`/opt/ghidra`) |
| IDA, Kuna | Not configured |

The FTK Imager 8.3.0.27 installer supplied in the workspace is intentionally
untracked and excluded from the project. It is treated as authorized
investigation input only. It must not be committed or redistributed.

## Intended stack

- Rust core libraries with Tokio jobs, Serde contracts, SQLite persistence, and
  Tracing instrumentation.
- A C ABI between the Rust core and a C++/Qt desktop shell.
- QML for the inspection and acquisition surfaces, backed by Qt item models.
- Pinned native forensic libraries, linked through reviewed C ABI wrappers.
- A separate analysis worker process so malformed evidence cannot crash the UI
  or corrupt the case database.

## Implementation gate

Phases 1 through 8 are implemented at the local development level. The
production release gate is not yet passed. Every evidence-facing C ABI job now
runs through `ofi-parser-worker`; case database and audit operations remain in
the trusted process. Sanitizer and scheduled fuzz runs remain open. Packaging
artifacts are locally tested, but clean-VM, secondary-distribution, lintian,
and physical-device package tests have not run. The DEB and AppImage currently
resolve host TSK and libewf versions instead of the pinned release build
described in the architecture.

## Documentation map

- [FEASIBILITY_REPORT.md](FEASIBILITY_REPORT.md): evidence-backed technical
  assessment and release decision.
- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md): phased roadmap and detailed
  Phase 1 tasks.
- [ARCHITECTURE.md](ARCHITECTURE.md): component, process, data, and deployment
  design.
- [REQUIREMENTS.md](REQUIREMENTS.md): normative functional and integrity
  requirements.
- [TEST_STRATEGY.md](TEST_STRATEGY.md): fixtures, differential testing, fuzzing,
  GUI tests, and acceptance gates.
- [docs/testing/phase1-results.md](docs/testing/phase1-results.md): exact Phase
  1 verification commands, results, and limitations.
- [docs/testing/phase2-results.md](docs/testing/phase2-results.md): Phase 2
  acquisition, E01, verification, report evidence, and limitations.
- [docs/testing/phase3-results.md](docs/testing/phase3-results.md): Phase 3
  partition, filesystem, timestamp, and TSK dependency evidence.
- [docs/testing/phase4-results.md](docs/testing/phase4-results.md): Phase 4
  viewer, signature, extraction, report, and security evidence.
- [docs/testing/phase5-background-jobs.md](docs/testing/phase5-background-jobs.md):
  Phase 5 job-state, cancellation, progress, and panic-containment evidence.
- [docs/testing/phase5-ui-results.md](docs/testing/phase5-ui-results.md): Phase 5
  Qt shell, QML model, theme, keyboard, and CTest evidence.
- [docs/testing/phase6-results.md](docs/testing/phase6-results.md): Phase 6
  audit, fuzz smoke, differential, race, source-open, capacity, and CTest
  evidence.
- [docs/testing/phase7-results.md](docs/testing/phase7-results.md): Phase 7
  artifact, installation, SBOM, and reproducibility evidence.
- [docs/testing/phase8-results.md](docs/testing/phase8-results.md): Phase 8
  parser-worker isolation, crash, cancellation, timeout, and package evidence.
- [docs/packaging/INSTALL.md](docs/packaging/INSTALL.md): installation,
  removal, Flatpak limits, and non-destructive case recovery.
- [docs/packaging/REPRODUCIBLE_BUILD.md](docs/packaging/REPRODUCIBLE_BUILD.md):
  dependency set, fixed timestamp, build commands, and reproducibility scope.
- [docs/security/phase6-review.md](docs/security/phase6-review.md): Phase 6
  security findings, open release blockers, and residual risks.
- [docs/security/phase8-review.md](docs/security/phase8-review.md): parser-worker
  boundary review, resolved finding, and residual risks.
- [SECURITY_MODEL.md](SECURITY_MODEL.md): threat model and mandatory controls.
- [docs/reverse-engineering/](docs/reverse-engineering/): FTK Imager evidence,
  limitations, and compatibility status.
- [docs/architecture/decisions.md](docs/architecture/decisions.md): architecture
  decisions that require review.
- [docs/specifications/interfaces.md](docs/specifications/interfaces.md): initial
  ABI, worker, hash, timestamp, and audit contracts.
- [docs/testing/fixtures.md](docs/testing/fixtures.md): synthetic fixture policy.

## License

OpenForensic Imager is free software, licensed under the **GNU General Public
License, version 3 or later** (`GPL-3.0-or-later`). See [LICENSE](LICENSE) for
the full text.

Forensic tools are licensed this way on purpose: an examiner should be able to
audit the exact code that produced their evidence, and every recipient of a
binary keeps the right to do the same. Third-party components and their license
terms are listed in
[packaging/common/THIRD_PARTY.md](packaging/common/THIRD_PARTY.md).

# openforensic-imager
