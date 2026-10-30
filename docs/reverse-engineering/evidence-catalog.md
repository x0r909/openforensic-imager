# Evidence Catalog

All observations were collected on 2026-10-30 unless noted. No production
evidence was used.

## EV-001: Supplied FTK installer identity

- Evidence type: Cryptographic artifact identity and file type.
- Source: `FTK-Imager-8.3.0.27/Exterro_FTK_Imager_(x64).exe`.
- SHA-256:
  `1ec9500126b9b33e79e8178ffa12104377f2b77ddc8fca69d8fa1068d8e360d8`.
- Size: 229,763,592 bytes.
- Reproduction: `sha256sum <installer>` and `file -k <installer>`.
- Supports: F-001.
- Limitations: Identity does not establish product behavior.

## EV-002: REA artifact inventory

- Evidence type: Provider-neutral static artifact graph.
- Evidence ID:
  `ev_a4a24779cd173bd3f863336a4d12a1d2da6bec60d5d93447a8f9a7aa15012e17`.
- Source: REA 6.8.0 `inspect-artifact`.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-artifact <installer>`.
- Result: One PE root node, SHA-256 verified, child container entries not
  examined.
- Supports: F-001, F-004, ARC-F-003.
- Limitations: Root-only inventory; no application semantics.

## EV-003: PE import and delay-import tables

- Evidence type: Static PE import inventory.
- Evidence ID:
  `ev_9cdd153b746177df28c7cb0c59b6765b86aa15c0e0e7f5890f97c4f7b8a3d41a`.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-imports <installer> --max-file-bytes 300000000`.
- Result: 18 DLLs, 366 imports, 60 delay imports, import hash
  `4de8ff83d82218e2af0f4f03d788686b`.
- Supports: F-002, ARC-F-001, ARC-F-005.
- Limitations: Imports do not prove runtime reachability.

## EV-004: PE certificate table

- Evidence type: Static PKCS#7 certificate structure.
- Evidence ID:
  `ev_01f2ec8d1304d15134917653ba5231b2b7aef4cb3712413cce958c7f72d533f8`.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-certificates <installer> --max-file-bytes 300000000`.
- Result: One entry, five certificates, signer `ACCESSDATA GROUP, INC.`.
- Supports: F-003, APP-F-004.
- Limitations: Signature, chain, timestamp, and digest coverage were not
  verified.

## EV-005: PE load configuration

- Evidence type: Static PE TLS and load-configuration structure.
- Evidence ID:
  `ev_2497cd21e6cf6b1740f3edaab1853cb8bb250d0db4ed39b12d04e62cd624a947`.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-load-config <installer> --max-file-bytes 300000000`.
- Result: SafeSEH table reported, no TLS directory observed.
- Supports: Installer-platform observations.
- Limitations: Does not establish mitigation enforcement or runtime behavior.

## EV-006: Embedded payload entropy and extraction failure

- Evidence type: Byte-level scan, archive probe, and entropy summary.
- Evidence ID:
  `ev_8cc8b454279326b7e901090b4622615e6c3e018a638e12e353aef317517fef2c`.
- Source: Installer bytes starting at offset 73,031,674.
- Reproduction: Locate `MSCF`, carve from that offset, use `cabextract` and
  7-Zip, then run REA `inspect-byte-entropy` over a bounded window.
- Result: CAB tools reject the data; first 1 MiB has entropy around 7.95.
- Supports: F-004, APP-F-003, ARC-F-002.
- Limitations: Entropy does not distinguish compression from encryption.

## EV-007: Official FTK Imager product page

- Evidence type: Vendor product and workflow documentation.
- URL: https://www.exterro.com/digital-forensics-software/ftk-imager
- Last observed HTTP modification: 2026-10-29 16:55:04 GMT.
- Reproduction: Fetch the page and inspect product overview, capabilities, and
  FAQ.
- Supports: F-005, F-006, F-007, F-008.
- Limitations: Marketing content is not an implementation specification.

## EV-008: Official FTK Imager 8.2 User Guide

- Evidence type: Vendor user guide, 118 pages.
- URL:
  https://d1kpmuwb7gvu1i.cloudfront.net/Imager/8.2/Exterro%20FTK%20Imager%208.2%20-%20User%20Guide%20.pdf
- SHA-256:
  `f836ef3098a75236ea33246e6af2329177d12a57f52f839ed2ff2116a065cd7a`.
- Last modified per HTTP: 2025-08-18.
- Reproduction: Download the PDF and use `pdftotext -layout`.
- Supports: UI, acquisition, verification, preview, export, and format findings.
- Limitations: Guide is 8.2, not the supplied 8.3.0.27 or current 8.3.1.6.

## EV-009: Official FTK Imager 8.3 release notes

- Evidence type: Vendor release notes, four pages.
- URL:
  https://d1kpmuwb7gvu1i.cloudfront.net/8.x/8.3.0/Exterro%20FTK%20Imager%208.3%20-%20Release%20Notes.pdf
- SHA-256:
  `78cfdc726df6176a4003e382efbb64aeedfcb2e0d297943292999715628e345a`.
- Reproduction: Download and extract text.
- Result: Mentions UFDR XML exfiltration fix, VeraCrypt/TrueCrypt error handling,
  and legacy driver cleanup.
- Supports: Security context, current-version context.
- Limitations: Release notes do not document the core acquisition behavior.

## EV-010: Official FTK download pages

- Evidence type: Vendor release identity.
- URLs:
  - https://www.exterro.com/ftk-downloads/ftk-imager-8-3
  - https://www.exterro.com/ftk-downloads/ftk-imager-8-3-1
- Reproduction: Fetch pages and inspect release information.
- Result: 8.3 installer ZIP and release notes; 8.3.1.6 with MD5
  `9b709146cf59825981e622b8ea11b762`.
- Supports: F-005, F-007, APP-F-007.
- Limitations: Download availability and hashes can change.

## EV-011: libewf upstream status

- Evidence type: Maintained source repository and releases.
- URL: https://github.com/libyal/libewf
- Latest pre-release observed: `20260924`, published 2026-09-25.
- Previous tag: `20240506`, also named pre-release.
- License metadata: LGPL-3.0.
- Reproduction: Query GitHub repository, releases, README, and tags.
- Result: EWF1 E01/S01 read/write, EWF2 read/write claims with explicit
  work-in-progress notes, no EWF2 encryption, no EWF2 bzip2 support in README.
- Supports: F-013, FMT-F-002, FMT-F-003.
- Limitations: README and ChangeLog contain conflicting maturity statements.

## EV-012: EWF format specification

- Evidence type: Public format specification.
- URL:
  https://github.com/libyal/libewf/blob/main/documentation/Expert%20Witness%20Compression%20Format%20%28EWF%29.asciidoc
- Reproduction: Inspect segment, section, chunk, compression, error, and hash
  sections.
- Supports: FMT-F-002, FMT-F-011.
- Limitations: Public specification does not cover every vendor extension.

## EV-013: EWF2 format specification

- Evidence type: Public format specification.
- URL:
  https://github.com/libyal/libewf/blob/main/documentation/Expert%20Witness%20Compression%20Format%202%20%28EWF2%29.asciidoc
- Reproduction: Inspect EWF2 sections, compression, encryption, sector table,
  error table, and hashes.
- Supports: FMT-F-003.
- Limitations: Contains explicit TODO and uncertainty markers.

## EV-014: The Sleuth Kit current capabilities

- Evidence type: Maintained source and release metadata.
- URL: https://github.com/sleuthkit/sleuthkit
- Release: `sleuthkit-4.15.0`, published 2026-04-15.
- Reproduction: Inspect `tsk/fs/tsk_fs.h`, `tsk/vs/tsk_vs.h`, and release
  metadata.
- Result: MBR/DOS and GPT volume systems; NTFS, FAT12/16/32, exFAT, ext2/3/4 and
  other file systems.
- Supports: F-014, FMT-F-004 through FMT-F-009.
- Limitations: Type constants prove implementation presence, not semantic
  correctness.

## EV-015: TSK timestamp conversion source

- Evidence type: Upstream source review.
- URLs:
  - https://github.com/sleuthkit/sleuthkit/blob/develop-4.1x/tsk/fs/fatfs_utils.c
  - https://github.com/sleuthkit/sleuthkit/blob/develop-4.1x/tsk/fs/exfatfs_meta.c
  - https://github.com/sleuthkit/sleuthkit/blob/develop-4.1x/tsk/fs/ntfs.c
  - https://github.com/sleuthkit/sleuthkit/blob/develop-4.1x/tsk/fs/ext2fs.c
- Reproduction: Inspect `fatfs_dos_2_unix_time`, exFAT metadata conversion,
  `nt2unixtime`, and ext4 extra timestamp handling.
- Result: Local `mktime` for DOS-style times; exFAT offset not applied in the
  inspected path; NTFS epoch clamp; ext4 epoch bits not applied.
- Supports: F-015, FMT-F-010.
- Limitations: Must be revalidated against the exact pinned commit and runtime
  fixtures.

## EV-016: Host toolchain inventory

- Evidence type: Executable environment observation.
- Reproduction: `cargo --version`, `rustc --version`, `qmake6 -query QT_VERSION`,
  `cmake --version`, `ninja --version`, compiler versions, and package queries.
- Result: Ubuntu 26.04.1, Rust 1.93.1, Qt 6.10.2, CMake 4.2.3, Ninja 1.13.2,
  GCC 15.2.0, Clang 21.1.8.
- Supports: F-011.
- Limitations: Environment changes over time.

## EV-017: Installed forensic packages and missing tools

- Evidence type: Package-manager observation.
- Reproduction: `dpkg-query`, `apt-cache policy`, `ldconfig -p`, and `command -v`.
- Result: libewf 20140816, TSK 4.12.1, libvhdi/libvmdk runtime present, libfsntfs
  and libvshadow absent, EWF tools absent, Qt PDF absent.
- Supports: F-012, F-017.
- Limitations: Host-specific.

## EV-018: Cross-distribution package state

- Evidence type: Distribution package metadata.
- Repositories:
  - Debian Sources API.
  - Arch Linux package JSON.
  - Fedora Packages and Rawhide specs.
- Reproduction: Query package metadata for the selected dependencies.
- Result: TSK newer on Debian/Arch/Fedora than Ubuntu 26.04; libewf old or
  out-of-date across distributions; optional libyal packages not consistently
  available.
- Supports: F-012.
- Limitations: Mirrors and release branches can lag or change.

## EV-019: Synthetic filesystem and partition probes

- Evidence type: Executed local fixture generation and parser observation.
- Reproduction: Generate with `mke2fs`, `mkntfs`, `mkfs.vfat`, `mkfs.exfat`,
  `sfdisk`, and `sgdisk`; parse with `fsstat`, `fls`, and `mmls`.
- Result: Valid ext4, NTFS, FAT32, exFAT, MBR, and GPT structures were parsed;
  deleted ext4 name retained after `debugfs rm`; empty image returned a bounded
  error.
- Supports: F-014, F-018, FMT-F-004 through FMT-F-009.
- Limitations: Generated images were disposable and not deterministic golden
  fixtures.

## EV-020: Qt model/view and threading documentation

- Evidence type: Official framework documentation retrieved with Context7.
- URLs:
  - https://doc.qt.io/qt-6/qabstractitemmodel.html
  - https://doc.qt.io/qt-6/qml-qtquick-treeview.html
- Reproduction: Context7 query on `QAbstractItemModel` thread safety and QML
  `TreeView`.
- Result: Item models are not thread-safe and must be called from their owning
  thread; background updates must be queued.
- Supports: Architecture model and process design.
- Limitations: API behavior must be tested against installed Qt 6.10.2.

## EV-021: Dependency license sources

- Evidence type: Upstream and package licensing metadata.
- Sources:
  - libewf repository license metadata.
  - libyal optional repositories.
  - TSK `licenses/` directory.
  - Debian copyright files.
  - Fedora TSK spec.
- Reproduction: Query repository license APIs and inspect package copyright
  files.
- Result: libyal libraries LGPL-3.0-or-later; TSK mixed CPL-1.0, IBM, GPL, MIT,
  BSD, Apache; Qt multi-license.
- Supports: F-016.
- Limitations: Not a legal opinion.

