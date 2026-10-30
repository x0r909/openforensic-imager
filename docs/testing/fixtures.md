# Synthetic Fixture Policy

Status: Proposed for test implementation.

## Rules

- Never use production evidence.
- Generate fixtures from public specifications and project-owned scripts.
- Use fixed identifiers, timestamps, file contents, and feature sets.
- Commit generator code and manifests, not large binary images.
- Store generated images under `tests/fixtures/generated/`.
- Treat a manifest mismatch as a test failure.

## Required generators

| Fixture family | Generator |
|---|---|
| RAW dense, sparse, split | Project Rust fixture generator |
| E01 single and segmented | Pinned libewf `ewfacquire` |
| MBR | `sfdisk` |
| GPT | `sgdisk` |
| NTFS | `mkntfs`, `ntfscp`, optional mount-based population |
| FAT32 | `mkfs.vfat`, `mcopy`, `mdel` |
| exFAT | `mkfs.exfat`, exFAT population tool, delete or rename probes |
| ext4 | `mke2fs`, `debugfs` |
| Corrupt structures | Project Rust mutation generator |

## Phase 0 exploratory results

The following were executed in disposable `/tmp` directories on 2026-10-30:

```text
mke2fs -q -F -t ext4 -L OFTEST ext4.raw
fsstat -f ext4 ext4.raw
fls -f ext4 -r ext4.raw

mkntfs -F -Q -L OFTEST ntfs.raw
fsstat -f ntfs ntfs.raw
fls -f ntfs -r ntfs.raw

mkfs.vfat -F 32 -n OFTEST fat32.raw
fsstat -f fat32 fat32.raw
fls -f fat32 -r fat32.raw

mkfs.exfat -L OFTEST exfat.raw
fsstat -f exfat exfat.raw
fls -f exfat -r exfat.raw
```

The installed TSK 4.12.1 parsed all four filesystem images. The ext4 deletion
probe used:

```text
debugfs -w -R "write payload.bin /deleted.txt" ext4.raw
debugfs -w -R "rm /deleted.txt" ext4.raw
fls -f ext4 -rd ext4.raw
```

It returned deleted metadata address 13 for `deleted.txt`.

Partition probes used:

```text
sfdisk -q mbr.raw
mmls mbr.raw

sgdisk -o gpt.raw
sgdisk -n 1:2048:+100M -t 1:8300 -c 1:LINUX gpt.raw
sgdisk -n 2:0:+64M -t 2:0700 -c 2:WINDOWS gpt.raw
mmls gpt.raw
```

The resulting `mmls` offsets and lengths matched the generators.

These exploratory outputs prove generator and parser availability. They are not
committed golden fixtures because filesystem identifiers and generation times
were not fixed.

## Deterministic metadata

Each generator must fix:

- Filesystem UUID, volume ID, volume serial, and GUID.
- Creation, modification, access, and deletion timestamps.
- File names, paths, sizes, contents, attributes, and allocation status.
- Block, cluster, and sector sizes.
- Compression, sparse, journal, and feature flags relevant to the test.

The fixture manifest records the generator commit, tool versions, command line,
input hashes, output hashes, and expected parser observations.

## Corruption corpus

Mutation recipes include:

- Header magic changes.
- Header checksum and CRC changes.
- Length and count fields set to 0, 1, maximum, and maximum plus one.
- Segment number mismatches.
- Missing middle and final segments.
- Truncation at section boundaries and inside chunks.
- Chunk offset table cycles and out-of-range entries.
- GPT primary and backup disagreement.
- MBR entries outside image bounds and overlapping entries.
- Filesystem metadata address cycles.
- Names with NUL, `/`, `..`, bidi controls, invalid UTF-8, and maximum length.

Every mutation stores its parent fixture hash and mutation bytes so the
reproducer is stable.

## Test host tools

The current host has the filesystem generators and partition tools needed for
the exploratory suite. Before Phase 2 and Phase 3 release gates, the reproducible
test image must add:

- The pinned libewf command-line tools.
- `mtools` for FAT and exFAT content manipulation.
- Development headers for enabled optional libyal libraries.
- Sanitizer-capable native build dependencies.

