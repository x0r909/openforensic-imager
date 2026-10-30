# Format Analysis

## Sources

Public specifications and maintained implementations:

- [EWF specification](https://github.com/libyal/libewf/blob/main/documentation/Expert%20Witness%20Compression%20Format%20%28EWF%29.asciidoc)
- [EWF2 specification](https://github.com/libyal/libewf/blob/main/documentation/Expert%20Witness%20Compression%20Format%202%20%28EWF2%29.asciidoc)
- [libewf README](https://github.com/libyal/libewf/blob/main/README)
- [The Sleuth Kit](https://github.com/sleuthkit/sleuthkit)
- [ext4 on-disk layout](https://docs.kernel.org/filesystems/ext4/)
- [Microsoft exFAT specification](https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification)
- [UEFI specifications](https://uefi.org/specs/UEFI/)

The following findings combine documented format behavior with static source
review and synthetic fixture probes. They are not claims of complete format
coverage.

## FMT-F-001: RAW/DD

- Finding ID: FMT-F-001
- Description: A RAW image is a byte stream of the source. It has no mandatory
  header, embedded hash, segment table, or metadata. Split RAW naming is
  convention-driven.
- Evidence source: TSK image-format documentation and host `fsstat -i list`.
- Reproduction procedure: Create a deterministic file and open it as a RAW
  image with TSK `fsstat` or a direct reader.
- Confidence: Confirmed at format level.
- Technical implications: Format identification can be ambiguous. Integrity
  depends on an external manifest and computed hashes.
- Implementation recommendation: Support explicit RAW selection and content
  detection. Store the segment order and hashes in a sidecar manifest.
- Known limitations: No way to authenticate a RAW image from bytes alone.

## FMT-F-002: EWF1 E01

- Finding ID: FMT-F-002
- Description: EWF stores media data in one or more segments, with section
  descriptors, header and volume sections, chunk tables, sector data, optional
  hash sections, and optional error/session data. Compression may be zlib,
  bzip2 in supported contexts, or none depending on variant and library support.
  Stored hashes include MD5 and SHA-1.
- Evidence source: EV-010 and EV-011.
- Reproduction procedure: Read the EWF specification sections for segment
  files, section descriptors, header, volume, data, sectors, table, error, and
  hash sections.
- Confidence: Observed in public specification and libewf source.
- Technical implications: Segment and chunk validation is mandatory. An E01
  image can be internally consistent while still containing corrupted or
  unreadable source ranges.
- Implementation recommendation: Use libewf, validate the complete segment set,
  compute independent hashes, and mark stored hashes separately.
- Known limitations: Vendor variants and corrupt recovery cases require fixtures
  and differential tests.

## FMT-F-003: EWF2 Ex01

- Finding ID: FMT-F-003
- Description: EWF2 changes section layout, supports larger segment sizes, and
  optionally supports encryption and bzip2. libewf upstream marks EWF2 write
  work incomplete and does not support encryption.
- Evidence source: EV-010, EV-011, and the EWF2 specification.
- Reproduction procedure: Inspect the EWF2 specification and libewf README and
  ChangeLog.
- Confidence: Observed in documentation and source.
- Technical implications: Ex01 is not part of the initial creation claim.
- Implementation recommendation: Permit experimental read only after dedicated
  segment, compression, and corruption tests. Never claim EWF2 write in 0.1.
- Known limitations: Upstream support statements are inconsistent.

## FMT-F-004: MBR

- Finding ID: FMT-F-004
- Description: MBR partition tables use a 446-byte boot area, four 16-byte
  primary entries, a signature, and extended partition chains for additional
  logical volumes. CHS fields are legacy; LBA start and length are the reliable
  source for offsets when valid.
- Evidence source: TSK DOS volume-system source and the synthetic `sfdisk`/`mmls`
  probe.
- Reproduction procedure: Build a disposable MBR image with `sfdisk`, then
  compare `mmls` output with `sfdisk -d`.
- Confidence: Confirmed for the synthetic valid case.
- Technical implications: Entries can overlap, point outside the image, or form
  invalid chains.
- Implementation recommendation: Use TSK for parsing, validate all computed
  byte ranges, and retain the raw type code.
- Known limitations: CHS interpretation and hybrid edge cases require additional
  fixtures.

## FMT-F-005: GPT

- Finding ID: FMT-F-005
- Description: GPT uses a protective MBR, primary and backup headers, CRC32
  fields, partition entry arrays, GUID type identifiers, and GUID partition
  identifiers.
- Evidence source: UEFI GPT specification, TSK GPT implementation, and the
  synthetic `sgdisk`/`mmls` probe.
- Reproduction procedure: Build a disposable GPT image with `sgdisk`, compare
  `mmls` with `sgdisk -p`, then mutate CRCs and the backup header.
- Confidence: Confirmed for the synthetic valid case; corruption behavior needs
  tests.
- Technical implications: Backup-header disagreement affects trust and recovery.
- Implementation recommendation: Use TSK, expose primary and backup status, and
  test CRCs, gaps, overlap, and missing backup.
- Known limitations: Hybrid MBR and unusual block sizes need separate coverage.

## FMT-F-006: NTFS

- Finding ID: FMT-F-006
- Description: NTFS stores file records in the Master File Table, with
  attributes such as `$STANDARD_INFORMATION`, `$FILE_NAME`, `$DATA`,
  `$ATTRIBUTE_LIST`, and indexes. Deleted MFT entries can retain names, runs, and
  timestamps.
- Evidence source: TSK NTFS implementation and synthetic `mkntfs`/`fls` probe.
- Reproduction procedure: Create a synthetic NTFS image and list metadata,
  alternate streams, orphan entries, and deleted metadata.
- Confidence: Confirmed for basic parsing; deletion and compression semantics
  require more fixtures.
- Technical implications: There are multiple timestamp sets and data streams.
  Allocation state and parent references can be inconsistent.
- Implementation recommendation: Use TSK for parsing, expose attribute and
  stream identity, and preserve both standard-information and filename times.
- Known limitations: TSK's current NTFS conversion returns zero outside a
  limited epoch range. MFT reuse and corrupt attribute lists complicate deletion
  claims.

## FMT-F-007: FAT32

- Finding ID: FMT-F-007
- Description: FAT32 uses boot parameters, FSInfo, two FAT copies, a root cluster
  chain, and 32-byte directory entries. Directory entries can be deleted while
  cluster chains may still contain content.
- Evidence source: Public FAT documentation, TSK FAT implementation, and
  synthetic `mkfs.vfat`/`fls` probe.
- Reproduction procedure: Create a FAT32 image and list root, deleted, and
  metadata entries.
- Confidence: Confirmed for basic parsing.
- Technical implications: FAT timestamps are local wall-clock fields without
  timezone. FAT copies can disagree.
- Implementation recommendation: Use TSK, report FAT copy disagreement, and
  require a timezone assumption for timestamp interpretation.
- Known limitations: Recovery depends on unoverwritten clusters; FAT time range
  and local timezone semantics are not fully represented by TSK conversion.

## FMT-F-008: exFAT

- Finding ID: FMT-F-008
- Description: exFAT uses a VBR, FAT, cluster heap, allocation bitmap, upcase
  table, and directory entry sets. Timestamps include sub-second fields and a
  stored UTC offset for some entries.
- Evidence source: Microsoft exFAT specification, TSK implementation, and
  synthetic `mkfs.exfat`/`fls` probe.
- Reproduction procedure: Create a synthetic exFAT image and inspect metadata,
  allocation bitmap, root directory, and deleted entries.
- Confidence: Confirmed for basic parsing.
- Technical implications: The current TSK conversion path uses local `mktime`
  and does not establish the UTC offset interpretation in the inspected path.
- Implementation recommendation: Preserve the limitation and add tests or an
  independent raw-field correction before presenting UTC precision.
- Known limitations: Deleted and timezone edge cases need additional fixtures.

## FMT-F-009: ext4

- Finding ID: FMT-F-009
- Description: ext4 uses superblocks, group descriptors, inode tables, block and
  inode bitmaps, extents, directory entries, journals, and extended timestamp
  fields.
- Evidence source: ext4 kernel documentation, TSK implementation, and synthetic
  `mke2fs`, `debugfs`, `fsstat`, `fls`, and deletion probes.
- Reproduction procedure: Create a synthetic ext4 image, write and delete a
  file with `debugfs`, list allocated and deleted metadata with `fls`, and
  cross-check with `debugfs`.
- Confidence: Confirmed for basic parsing and deletion-name retention.
- Technical implications: Current TSK source uses the extra timestamp fields
  for nanoseconds but does not apply the epoch bits in the inspected path.
  Deleted inode pointers can be cleared.
- Implementation recommendation: Use TSK, test ext4 features and timestamp
  boundaries, and label recovery limits.
- Known limitations: Post-2038 times and some feature combinations require fixes
  or explicit unsupported status.

## FMT-F-010: Timestamp semantics

- Finding ID: FMT-F-010
- Description: System event times can be UTC. Filesystem timestamps cannot
  always be converted to a unique UTC instant. FAT and exFAT store local
  wall-clock values, NTFS stores UTC filetime, and ext4 stores epoch seconds with
  extra epoch and nanosecond fields.
- Evidence source: TSK `fatfs_utils.c`, `exfatfs_meta.c`, `ntfs.c`, and
  `ext2fs.c`; format specifications.
- Reproduction procedure: Review the cited conversion functions and run
  timezone-sensitive fixtures.
- Confidence: Confirmed at source level; exact pinned-version behavior still
  needs runtime tests.
- Technical implications: UTC-only display can mislead an examiner.
- Implementation recommendation: Store provenance, assumption, precision, and
  validity. Show the assumption in UI and reports.
- Known limitations: Additional independent parsing may be needed to recover
  exFAT offsets and ext4 epoch bits.

## FMT-F-011: Hash scope

- Finding ID: FMT-F-011
- Description: EWF1 stores MD5 and SHA-1 values, but the requested SHA-256 and
  SHA-512 are independent calculations. Unreadable sectors and zero-fill
  policies change what a source hash means.
- Evidence source: EWF specification hash sections, libewf API, and project
  integrity requirements.
- Reproduction procedure: Inspect EWF hash sections and perform acquisition
  with and without injected read errors.
- Confidence: Confirmed at format level.
- Technical implications: One hash label without scope is ambiguous.
- Implementation recommendation: Record algorithm, scope, byte count, stored
  versus computed status, and zero-fill policy.
- Known limitations: Independent verification cannot recover bytes that were
  never readable from the source.

