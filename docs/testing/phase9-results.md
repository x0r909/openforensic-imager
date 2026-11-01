# Phase 9 Validation Results

Status: sanitizer and fuzz-budget evidence collected on this host. The Phase 8
residual hardening is tracked in
[`docs/superpowers/plans/2026-11-01-remaining-work.md`](../superpowers/plans/2026-11-01-remaining-work.md);
what that hardening does not cover is listed under "Not covered" below.

## Host and toolchain

- Ubuntu 26.04.1 LTS, kernel 7.0.0-42-generic, 8 cores, 30 GiB RAM.
- Repo toolchain: `rustc 1.93.1` / `cargo 1.93.1` (pinned by
  `rust-toolchain.toml`).
- Sanitizer toolchain: `rustc 1.101.0-nightly (f9ceb4a4d 2026-10-31)`,
  `cargo 1.101.0-nightly (31d3246e0 2026-10-27)`, with `rust-src` and
  `llvm-tools`.

## AddressSanitizer

```
scripts/run-sanitizers.sh --sanitizer address \
    --skip acquisition_never_opens_the_source_for_write
```

The script exports `RUSTFLAGS="-Zsanitizer=address --cfg ofi_sanitizer"`,
`RUSTDOCFLAGS` likewise, `ASAN_OPTIONS=detect_leaks=1`, and
`LSAN_OPTIONS=suppressions=scripts/sanitizers/lsan.suppressions:print_suppressions=1`,
then runs `cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu
--workspace`. Log: `packaging/build/sanitizers/address.log` (git-ignored).

Result (2026-11-01): exit 0, 69 suites `test result: ok`, 166 tests passed, no
`ERROR: AddressSanitizer` line, no leak summary.

### Behaviour changes this required

- `ResourceLimits::parser_default` returns `u64::MAX` (`RLIMIT_INFINITY`) for
  `address_space_bytes` when `cfg(ofi_sanitizer)` is set. ASan and MSan reserve
  terabytes of shadow memory, so a 4 GiB `RLIMIT_AS` kills an instrumented
  worker with `failed to mmap` before it parses anything. Every normal build
  keeps the 4 GiB cap, and
  `address_space_limit_survives_the_sanitizer_requirement` pins both branches.
- `sandboxed_child_cannot_read_outside_allowlist_or_open_network` sets
  `ASAN_OPTIONS=detect_leaks=0` for the confined child. LeakSanitizer needs
  `fork` and `ptrace` to scan its child, and the child is confined by exactly
  the filter that denies both. Address sanitization still applies there; only
  the leak scan is off in that process.
- `worker_executable()` in `crates/native-bindings/tests/worker_isolation.rs`
  searches eight ancestors instead of three: cargo on this host emits test
  binaries below `target/<triple>/debug/build/<crate>/<hash>/out`.

### LeakSanitizer exceptions

`scripts/sanitizers/lsan.suppressions` silences `libtsk`, `libvmdk`, `libvhdi`,
`libewf`, and `libcdata`: TSK 4.12.1 keeps process-global allocations alive
after the handle closes and the C libraries are not instrumented, so LeakSanitizer
attributes their state to the test process. The 169 allocations it reported
before the suppression file existed all came from those libraries. Leaks
allocated by instrumented Rust still fail the run.

## MemorySanitizer (scoped)

```
scripts/run-sanitizers.sh --sanitizer memory \
    --crates "-p forensic-core -p hashing -p viewer -p audit-log -p worker-sandbox"
```

Result (2026-11-01): exit 0, 18 suites `test result: ok`, 53 tests passed, no
MemorySanitizer report. Log: `packaging/build/sanitizers/memory.log`.

MSan is claimed only for crates that link no C code, because its shadow tracking
stops at an uninstrumented library boundary: reads that TSK, libewf, or SQLite
perform are reported as uninitialized, which is a false positive about our
code, not a finding. Excluded for that reason: `acquisition`, `filesystem`,
`image-reader`, `libewf-bindings`, `partition-analyzer`, `tsk-bindings`,
`evidence-manager` (bundled SQLite), `extraction`, `native-bindings`,
`reporting` (depends on `acquisition`), and `job-manager` (`acquisition` is a
dev-dependency, so its test binaries link libewf too).

## Miri

```
cargo +nightly miri test -p forensic-core -p hashing -p viewer
```

Miri interprets the compiled MIR rather than running native code, so it detects
undefined behaviour the sanitizers cannot see: out-of-bounds pointer arithmetic
that happens to land in valid memory, uninitialized reads, aliasing violations,
and integer overflow in the abstract machine. It is a proof about *our* code
only — every C library the workspace links is opaque to it, and TSK, libewf, and
the bundled SQLite are exactly that. The scope is therefore the three crates
that are pure Rust end to end.

Result (2026-11-01): no undefined behaviour. `forensic-core` 7 tests ok,
`hashing` 3 tests ok. Log: `packaging/build/miri.log` (git-ignored).

`chunking_does_not_change_sha256` dominates the run at 2504 s; the other
suites finish in under a second each. It is a property test that hashes whole
buffers through every chunk boundary, and under interpretation each of those
SHA-256 rounds is executed step by step. The wall clock is the interpreter, not
the code under test.

### `viewer` aborts under isolation, in proptest rather than in our code

`viewer` compiles for Miri but its test binary aborts:

```
cargo +nightly miri test -p viewer
```

```
unsupported operation: `getcwd` not available when isolation is enabled
  ... proptest::test_runner::failure_persistence::file::absolutize_source_file_with_cwd
```

The call is not the crate's. It is proptest 1.12's *failure persistence* layer,
which writes a failing seed to `proptest-regressions/` and calls
`std::env::current_dir` to absolutize the source path
(`proptest-1.12.0/src/test_runner/failure_persistence/file.rs:220`, reached
from `runner.rs:592`). Miri's isolation refuses `getcwd`. Setting
`PROPTEST_DISABLE_FAILURE_PERSISTENCE=1` does not help, because isolation hides
the environment too — proptest cannot read the variable that would turn the
layer off.

Disabling isolation makes all 19 tests pass, including the two that
otherwise abort. Measured with `MIRIFLAGS=-Zmiri-disable-isolation`:

```
cargo +nightly miri test -p viewer     # MIRIFLAGS=-Zmiri-disable-isolation
```

Result: 19 passed, 0 failed (log
`packaging/build/miri-viewer-noisolation.log`). So the abort is the harness, not
the code: `viewer` is byte-clean under Miri once the interpreter is allowed the
syscalls proptest's own bookkeeping needs. The default-isolation run is
recorded here as the list of tests Miri cannot execute, and the no-isolation run
as the actual check.

No `crates/viewer` test is skipped by the guard that `known_vectors` needs;
the two crates abort for unrelated reasons — `known_vectors` on `open`, because
the test itself reads a file, and `viewer` on `getcwd`, because a dependency
does.

### Isolation and the repository README

`known_vectors` has one test that reads the repository README and compares the
crate's SHA-256 against `sha256sum`. Miri enforces isolation by default, and
`open` is unsupported under it, so that test aborted the run with
`unsupported operation: 'open' not available when isolation is enabled`. The
failure says nothing about the crate — it is the interpreter refusing a
syscall. The test now guards on `!cfg!(miri)` at runtime rather than
`#[cfg(miri)]`: Miri evaluates the same cfg-selected code path as a normal
build, so a compile-time attribute would not skip it.

Re-run after the guard (`cargo +nightly miri test -p hashing --test
known_vectors`, log `packaging/build/miri-known-vectors.log`): exit 0, 3 passed,
0 failed, 0 ignored, 1.53 s. The two vector tests that need no host filesystem
still run under Miri; only the `sha256sum` comparison is skipped.

For a run that exercises that comparison too, opt in to the host filesystem with
`MIRIFLAGS=-Zmiri-disable-isolation`. It is not the default because it also
grants the interpreted program access to the environment and the clock, which
gives up part of what Miri is being used to establish.

## Fuzzing (ASan budget)

```
scripts/fuzz-budget.sh --minutes 10
```

`cargo-fuzz` builds the target with AddressSanitizer, so this budget is the
ASan fuzz budget. `-rss_limit_mb=4096` bounds one runaway input. The script
fails on a non-zero target exit and on any file appearing under
`tests/fuzz/artifacts/<target>/`. Logs are git-ignored under
`packaging/build/fuzz/`.

Result (2026-11-01): exit 0, no artifacts, no crash.

| Target | Executions | Wall | Peak RSS | Final coverage |
|---|---|---|---|---|
| `c_abi_json` | 10,439,152 | 601 s | 529 MB | cov 1305, ft 5210 |
| `viewer_pages` | 32,820,070 | 601 s | 512 MB | cov 177, ft 525 |

Coverage had plateaued before the budget ended: the last three progress lines
in each log report the same `cov`/`ft` pair, after 2,112 reporting lines for
`c_abi_json` and 85 for `viewer_pages`. Executions kept accumulating with no new
coverage, which is what a fuzzer does once it has saturated its corpus — it is
evidence that the budget ran to completion, not that the targets are
exhaustively tested.

Both ran the full budget rather than being cut short, and both peaked at
roughly 512 MB RSS — an eighth of the 4 GiB ceiling. The ceiling was not
approached, so it constrains a pathological input, not normal execution.

Execution rates are 17.4k/s (`c_abi_json`) and 54.6k/s (`viewer_pages`). The
Phase 6 comparison figure of 645,347 executions in 21 seconds (≈30.7k/s) is not
directly comparable: it was a five-second smoke run against a smaller seed
corpus, and the rate depends on corpus size and input length. The `c_abi_json`
rate is lower here for that reason, not because the target regressed.

### Triage

No artifact was produced, so no input needed triage. Nothing was written to
`tests/fuzz/artifacts/`, and both artifact directories are empty. A run with an
artifact is a failed run: it is reproduced with
`cargo +nightly fuzz run --fuzz-dir tests/fuzz <target> tests/fuzz/artifacts/<target>/<file>`
and either becomes a fix task or is recorded here as a known non-crash.

### Scheduling

`TEST_STRATEGY.md` section 11 records the decision: the budget is a **nightly**
tier item, not a pull-request gate, because ten minutes per target is too long
to run in front of every pull request and the property it tests does not change
between two commits of an unrelated file. That section carries the exact
`systemd --user` timer and the `Nice`/`CPUSchedulingPolicy=batch` settings that
keep the run from competing with an examiner's acquisition. **Nothing executes
this budget automatically today** — the repository has no CI configuration and
no committed unit files, so a maintainer runs it by hand.

## Not covered

- `acquisition_never_opens_the_source_for_write` (the `strace` regression
  test) is skipped in the sanitizer run: LeakSanitizer cannot work under
  ptrace. It still runs, and passes, in `cargo test --workspace`.
- The Qt/QML desktop is outside the sanitizer run: it is a CMake target and no
  sanitizer flags are wired into that build.
- TSK, libewf, and the bundled SQLite are C libraries without instrumentation;
  ASan intercepts their allocator use, but their own reads and writes are not
  instrumented.

## Baseline commands on the same host

```
cargo test --workspace     # 70 suites ok, 182 tests passed
cargo clippy --workspace --all-targets -- -D warnings   # clean
```
