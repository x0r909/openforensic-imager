# Phase 6 Validation Results

Date: 2026-10-30

## Scope

Phase 6 adds security, differential, audit, fuzz, race, and capacity evidence to
the Phase 1 through Phase 5 implementation. The main additions are:

- Audit report output for JSON, CSV, and HTML with the local-chain limitation
  stated in each report.
- Audit-open preflight before case or evidence rows are committed.
- Desktop case and evidence operations pass an adjacent `audit.jsonl` path.
- Landlock and seccomp negative tests for the worker-sandbox crate.
- Side-effect-free C ABI JSON fuzz entry point and viewer fuzz entry point.
- Stable property tests for arbitrary viewer input and job-request bytes.
- MBR, GPT, ext4, RAW/E01, and SHA-256 differential checks.
- Syscall evidence that acquisition opens the source only for reading.
- Repeated extraction-root replacement against the capability root.
- Sparse 8 GiB image capacity and bounded-memory reads.

## Environment

| Component | Observed value |
|---|---|
| OS | Ubuntu 26.04.1 LTS |
| Kernel | Linux 7.0.0-38-generic x86_64 |
| Rust | 1.93.1 stable |
| TSK | 4.12.1 |
| libewf | Distribution package and `ewf-tools` |
| Fuzz runner | cargo-fuzz 0.13.2 |
| Qt | 6.10.2 |

## Executable evidence

### Rust workspace

The test binaries were compiled with:

```bash
cargo test --workspace --no-run --message-format=json > target/phase6-build.json
```

All 45 emitted test executables were then run with `--test-threads=1`. Two of
those tests also launch exact-name child test processes for sandbox and syscall
isolation.

```text
45 test executables
143 top-level tests passed
0 tests failed
0 tests ignored
```

The Phase 6-specific results were:

| Test suite | Result |
|---|---|
| `native-bindings` ABI tests | 16 passed |
| `worker-sandbox` tests | 6 passed |
| `acquisition` round trip, cancellation, and source-open trace | 7 passed |
| `extraction` traversal, symlink, budget, and replacement tests | 13 passed |
| `phase6_differential` | 4 passed |
| `phase6_capacity` | 2 passed |
| Viewer unit and property tests | 19 passed |
| Audit log and audit report tests | 12 passed across their test binaries |
| Desktop case audit integration | Qt model test passed |

The ABI suite includes an arbitrary-byte property test that calls the same job
request parser used by the C ABI and a test proving request validation does not
create an audit file. The viewer property test calls hex, text, and signature
entry points with arbitrary bytes and out-of-range offsets.

### Security and race evidence

The worker-sandbox child test verifies all of the following in a separate
process:

- A file in the read allowlist can be read.
- A file outside the allowlist returns `EACCES`.
- Creating a file without a write grant returns `EACCES`.
- Opening a network socket returns `EPERM`.
- Calling `fork` returns `EPERM`.

The acquisition source-open test runs the acquisition in a child process under:

```bash
strace -f -qq -e trace=open,openat,openat2 -o source-open.trace \
  <test-binary> --exact source_write_syscall_child --nocapture --test-threads=1
```

Every traced open of the synthetic source contains `O_RDONLY` and none contains
`O_WRONLY` or `O_RDWR`.

The extraction replacement test performs 64 iterations of moving the opened
root and replacing its original path with a symlink to an outside directory.
All exported bytes remain under the moved root and the outside directory stays
empty.

### Audit evidence

Audit-open failure now occurs before the case row is created. The regression
test first failed because the error code was the generic `operation_failed`,
then passed after the implementation returned `audit.open` and allowed the same
case identifier to be committed on retry.

The desktop case-creation test first failed because no audit log was written,
then passed after the controller supplied the case-directory `audit.jsonl` path.

The audit report test first failed because the JSON status omitted the `valid`
field, then passed after a custom serializer emitted `valid`, `record_count`,
`head_hash`, and `reason`.

The remaining post-commit append gap and the non-notarized hash-chain limitation
are documented in [phase6-review.md](../security/phase6-review.md).

### Differential evidence

The differential suite compares independent tools on disposable synthetic
fixtures:

| Feature | Reference |
|---|---|
| MBR start sectors and lengths | `sfdisk -d` |
| GPT start sector, length, and name | `sgdisk -i` |
| ext4 directory content and file bytes | `debugfs` |
| RAW logical bytes versus E01 logical bytes | `ewfacquire`, `ewfexport` |
| SHA-256 | `sha256sum` |

All four differential tests passed.

### Capacity evidence

The capacity child opens a sparse 8 GiB RAW file and performs 1,024 deterministic
64 KiB reads from different offsets. `/usr/bin/time -v` measured peak resident
memory below 128 MiB. This demonstrates bounded reads for this fixture; it is
not a multi-terabyte soak test.

### Fuzz smoke

Both fuzz targets were built on stable Rust with coverage instrumentation and
run with sanitizers disabled:

```bash
~/.cargo/bin/cargo-fuzz run viewer_pages --fuzz-dir tests/fuzz -s none -- \
  -max_total_time=5 -rss_limit_mb=1024

~/.cargo/bin/cargo-fuzz run c_abi_json --fuzz-dir tests/fuzz -s none -- \
  -max_total_time=5 -rss_limit_mb=1024
```

Both commands exited 0. No crash artifacts were produced. Generated corpora and
fuzz build output are ignored by git.

AddressSanitizer was attempted with:

```bash
~/.cargo/bin/cargo-fuzz run viewer_pages --fuzz-dir tests/fuzz -s address
```

It could not start because the host has only stable Rust without rustup or a
nightly toolchain:

```text
error: the option `Z` is only accepted on the nightly compiler
```

No sanitizer result is claimed.

### Static and dependency checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --manifest-path tests/fuzz/Cargo.toml --bins
/home/augie/.cargo/bin/cargo-audit audit
git diff --check
```

All commands exited 0. `cargo-audit` loaded 1,395 advisories, scanned 116 crate
dependencies, and reported no vulnerability finding.

### Qt and QML

```bash
cmake --build build/root
ctest --test-dir build/root --output-on-failure
/usr/lib/qt6/bin/qmllint *.qml
```

CTest passed 4 of 4 tests: model tests, light self-test, dark self-test, and QML
tests. `qmllint` exited 0 without diagnostics.

## Limitations

- The worker-sandbox crate has negative tests, but parser dispatch still runs
  in-process in the desktop application. Production parser process isolation
  remains an open architecture and release blocker.
- Fuzzing was a short smoke run without ASan, MSan, or a nightly fuzz budget.
  The native TSK, libewf, and libyal parser surfaces were not mutation-fuzzed in
  this pass.
- The capacity test uses an 8 GiB sparse file. No multi-terabyte, sustained
  throughput, or long-duration soak result is claimed.
- Differential coverage is limited to MBR, GPT, ext4, one E01 stream, and
  SHA-256 on this host. NTFS, FAT32, exFAT, segmented E01 edge cases, and
  feature matrices remain additional coverage work.
- Audit and SQLite writes are separate resources. If the audit append fails
  after the database commit, the API returns
  `audit.append_after_commit`; it does not claim a cross-resource transaction.
- The local audit hash chain is not externally notarized.
- The extraction replacement test is bounded deterministic stress, not a
  scheduler-driven high-frequency race campaign.
