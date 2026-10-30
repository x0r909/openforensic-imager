# Phase 3 Verification Results

Date: 2026-10-30

Branch: local `main`

## Implemented scope

- Narrow C ABI wrapper around The Sleuth Kit (TSK) with Rust-owned handle
  lifetimes.
- MBR and GPT volume-system discovery through TSK.
- Checked partition offset and length conversion.
- Read-only NTFS, FAT12/16/32, exFAT, ext2, ext3, and ext4 filesystem access.
- Directory listing, metadata extraction, bounded file reads, and deleted-name
  status.
- Timestamp semantics that preserve FAT/exFAT local wall-clock uncertainty and
  mark unknown filesystems explicitly.
- Partition-to-filesystem integration using an ext4 filesystem embedded in an
  MBR partition.
- Native discovery through `pkg-config tsk`; the build no longer assumes
  `/usr/include`.

## Verification evidence

The entire workspace was formatted, linted, compiled, and executed:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-run --message-format=json
```

Every emitted test executable was run with `--test-threads=1`.

Result: 30 test executables, 69 tests passed, 0 failed.

## Differential and generator coverage

| Area | Reference used by the test |
|---|---|
| MBR | `sfdisk` generated layout; offsets and sector counts match |
| GPT | `sgdisk` generated layout; partition description and size match |
| ext4 | `mke2fs` and `debugfs` generated content |
| NTFS | `mkntfs` and `ntfscp` generated content |
| FAT32 | `mkfs.vfat` and `mcopy` generated content |
| exFAT | `mkfs.exfat` volume and root-directory parsing |
| Embedded partition | ext4 copied at MBR sector 2048 and reopened by parsed offset |
| Deleted name | `debugfs write` followed by `debugfs rm` retains deleted status |

## Host dependency

The host provides TSK 4.12.1 through `pkg-config`. Development is compiled
against the detected include and library paths. The release build still requires
the reviewed and pinned TSK 4.15.0 baseline from `ARCHITECTURE.md`.

## Limitations

- Filesystem directory listing currently materializes the complete directory.
  Paging and memory limits for very large directories remain required work.
- The tests cover the primary filesystem feature set, not every compression,
  sparse-file, encryption, alternate-stream, journal, or corruption case.
- Deleted names and metadata addresses are reported. Automated deleted-content
  recovery is not claimed; data-block reuse must be examined separately.
- NTFS timestamp range behavior and ext4 post-2038 epoch handling still need
  dedicated boundary fixtures. exFAT UTC-offset recovery is not available
  through the current TSK conversion path.
- FAT and exFAT timestamps are exposed as local wall-clock values. The UI must
  require an examiner timezone assumption before presenting a UTC equivalent.
- Corrupt partition tables and hostile filesystem mutations are not yet part of
  the native-parser fuzz corpus.
- TSK is linked dynamically through the host package. A production release must
  pin and record the native library build and verify ABI compatibility.
