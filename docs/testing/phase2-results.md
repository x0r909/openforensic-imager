# Phase 2 Verification Results

Date: 2026-10-30

Branch: local `main`

## Implemented scope

- Read-only RAW/DD acquisition from image sources.
- EWF1 E01 acquisition through libewf.
- Segmented E01 output and multi-segment random-access reading.
- Staged output with an acquisition manifest and final commit only after success.
- MD5, SHA-1, SHA-256, and SHA-512 streaming hashes.
- Independent output-logical hashes and file-artifact hashes.
- Post-acquisition SHA-256 verification of the output logical stream.
- Explicit match, mismatch, and not-available verification outcomes.
- Cancellation that leaves a cancelled staging manifest instead of a committed
  image.
- Read-error zero filling with exact unreadable ranges and partial status.
- JSON, CSV, and HTML acquisition and verification reports with HTML escaping and
  CSV formula neutralization.
- E01 stored MD5/SHA-1 reading for images created by `ewfacquire`.

## Verification evidence

The full workspace suite is built with:

```text
cargo test --workspace --no-run --message-format=json
```

Every emitted test executable is run with `--test-threads=1`.

Result: 24 test executables, 58 tests passed, 0 failed.

Formatting and lint:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Both exit 0.

E01 compatibility is checked with the installed independent tool:

```text
ewfverify <generated.E01>
ewfinfo <generated.E01>
```

The tests cover single-segment, segmented, and `ewfacquire`-generated images.

## Test coverage

| Area | Tests |
|---|---|
| RAW acquisition | Byte-for-byte output, source hash unchanged, manifest, SHA-256 |
| E01 acquisition | E01 extension, output logical hash equals source hash, `ewfverify` |
| Multi-segment E01 | More than one segment, cross-segment random reads, `ewfverify` |
| E01 reading | Random access, media size, sector size, read-only fingerprint |
| E01 stored hashes | MD5 and SHA-1 extracted from an `ewfacquire` image |
| Cancellation | No final artifact, cancelled manifest in staging |
| Read errors | Injected block failure, zero fill, partial status, range accounting |
| Verification | Match, mismatch, and unavailable expected hash |
| Reports | JSON, CSV, HTML, source escaping, CSV formula injection defense |

## Limitations

- The host uses libewf 20140816. The pinned current libewf build remains a
  release-engineering requirement.
- EWF encrypted images and EWF2 write are unsupported.
- The E01 writer does not currently set a stored SHA-256 value because EWF1 has
  no SHA-256 field. SHA-256 is an independent manifest and report value.
- Physical block-device acquisition was not executed on the host. Source paths
  were synthetic regular files.
- The report writer produces JSON, CSV, and HTML. PDF generation is deferred.
- Acquisition is not yet exposed through the desktop C ABI.
- A partial read error is represented by zero-filled output plus an explicit
  `partial` status. It must never be described as a complete source image.
- E01 cancellation leaves non-finalized staging data and a cancelled manifest;
  it is not a usable completed image.

