# Phase 9 Validation Results

Status: sanitizer evidence collected on this host; the fuzz budget and the
Phase 8 residual hardening are tracked in
[`docs/superpowers/plans/2026-11-01-remaining-work.md`](../superpowers/plans/2026-11-01-remaining-work.md).

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
cargo test --workspace     # 70 suites ok, 168 tests passed
cargo clippy --workspace --all-targets -- -D warnings   # clean
```
