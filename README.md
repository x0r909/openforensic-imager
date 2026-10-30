# OpenForensic Imager

Linux-native digital forensics acquisition and evidence analysis platform.

## Current status

Phase 0 is complete. Phase 1 core and Phase 2 acquisition/verification are
implemented and verified on the local workspace:

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

Phase 3 has not started. Its parser work remains gated on:

- [ARCHITECTURE.md](ARCHITECTURE.md)
- [REQUIREMENTS.md](REQUIREMENTS.md)
- [TEST_STRATEGY.md](TEST_STRATEGY.md)
- [SECURITY_MODEL.md](SECURITY_MODEL.md)
- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)

The verified Phase 1 and Phase 2 results are documented in
[docs/testing/phase1-results.md](docs/testing/phase1-results.md) and
[docs/testing/phase2-results.md](docs/testing/phase2-results.md). The detailed
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
| EWF command-line tools | Missing |
| Qt PDF module and Flatpak/AppImage tools | Missing |
| REA | 6.8.0, 229 tools advertised, Hopper available |
| Ghidra, IDA, Kuna | Not configured |

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

Phases 1 and 2 are complete at the core-library level. The next code phase is
partition and filesystem analysis through a pinned TSK build.

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
- [SECURITY_MODEL.md](SECURITY_MODEL.md): threat model and mandatory controls.
- [docs/reverse-engineering/](docs/reverse-engineering/): FTK Imager evidence,
  limitations, and compatibility status.
- [docs/architecture/decisions.md](docs/architecture/decisions.md): architecture
  decisions that require review.
- [docs/specifications/interfaces.md](docs/specifications/interfaces.md): initial
  ABI, worker, hash, timestamp, and audit contracts.
- [docs/testing/fixtures.md](docs/testing/fixtures.md): synthetic fixture policy.

# openforensic-imager
