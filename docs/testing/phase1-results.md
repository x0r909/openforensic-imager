# Phase 1 Verification Results

Date: 2026-10-30

Branch: local `main`

## Implemented scope

- Workspace and Rust toolchain manifest.
- Domain IDs, checked byte ranges, typed errors, and UTC timestamps.
- Streaming MD5, SHA-1, SHA-256, and SHA-512.
- Read-only RAW file or block-device access.
- Case and evidence SQLite persistence with migrations.
- Append-only hash-chained audit log.
- Minimal C ABI for case creation and evidence addition.
- Deterministic RAW fixture generation.
- End-to-end case, evidence, hash, read, and audit test.

## Verification evidence

### Rust tests

The host command wrapper terminates when `cargo test` launches a Rust test
binary. The equivalent two-step verification was used:

```text
cargo test --workspace --no-run --message-format=json
<run every emitted executable with --test-threads=1>
```

Result: 17 test executables, 43 tests passed, 0 failed.

### Formatting

`rustfmt` was not installed on the host. The matching Ubuntu 1.93.1 package was
downloaded to a temporary directory and executed without changing system
packages.

```text
cargo fmt --all -- --check
```

Result: exit 0.

### Lint

`clippy` was not installed on the host. The matching Ubuntu 1.93.1 package was
downloaded to the same temporary directory and executed without changing system
packages.

```text
cargo clippy --workspace --all-targets -- -D warnings
```

Result: exit 0.

### Compilation

```text
cargo check --workspace --all-targets
cargo test --workspace --no-run
```

Result: exit 0 with no compiler warnings.

### C ABI

```text
cargo build -p native-bindings
cc -std=c11 -Wall -Wextra -Werror \
  -Icrates/native-bindings/include \
  tests/integration/c-abi-smoke.c \
  -Ltarget/debug -lnative_bindings \
  -Wl,-rpath,$PWD/target/debug \
  -o target/c-abi-smoke
LD_LIBRARY_PATH=$PWD/target/debug ./target/c-abi-smoke
```

Result: the C harness compiled with warnings as errors and exited 0.

### Runtime memory check

AddressSanitizer is not available with the installed stable Rust toolchain.
Valgrind was run against the Phase 1 integration test instead:

```text
valgrind --quiet --error-exitcode=99 --leak-check=no \
  <phase1_core test binary> --test-threads=1
```

Result: exit 0.

## Test coverage

| Area | Tests |
|---|---|
| Domain IDs and ranges | UUID round trips, boundary ranges, zero length, overflow, out-of-bounds |
| Hashing | Empty and `abc` known vectors, randomized chunking property, coreutils SHA-256 comparison |
| RAW reader | Missing file, empty image, full read, out-of-range read, symlink refusal, concurrent modification |
| Evidence store | UTC timestamp, foreign key, duplicate case, duplicate evidence, reopen persistence |
| Audit log | Monotonic sequence, hash links, UTF-8 escaping, lock contention, reopen, edit/reorder/delete/truncate tamper |
| C ABI | Version, null and oversized inputs, invalid JSON, owned responses, panic containment |
| Fixtures | Empty, dense, sparse, split, deterministic hashes, committed manifest vector |
| Integration | Case and evidence creation, hash, RAW read, source unchanged, audit chain reopen |

## Limitations

- No Qt UI exists yet.
- No TSK, libewf, partition, filesystem, extraction, or acquisition workflow is
  implemented in Phase 1.
- The C header is hand-maintained because `cbindgen` is not installed. Rust ABI
  tests and the C smoke build guard the current surface.
- The local command wrapper cannot run `cargo test` directly after test binary
  launch. Tests were executed from the cargo-emitted binaries instead.
- The host's package installation requires interactive authentication. Matching
  formatter and linter binaries were downloaded and extracted temporarily.
- The RAW sector size defaults to 512 bytes because RAW has no authoritative
  sector-size metadata. Callers may supply a verified value in a later metadata
  layer.
- The audit hash chain is tamper-evident for ordinary edits, reordering, and
  truncation. It cannot defend against an attacker who can rewrite the entire
  chain.
- No production evidence was used.

## Exit status

Phase 1 core acceptance criteria are met. Phase 2 has not started.

