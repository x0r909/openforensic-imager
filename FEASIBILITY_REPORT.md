# OpenForensic Imager Feasibility Report

Date: 2026-10-30

Status: Phase 0 complete, Phase 1 gated on review.

## 1. Executive decision

OpenForensic Imager is feasible as an independent Linux-native forensic
acquisition and analysis application. The recommended implementation should
proceed to Phase 1 only after this report, the architecture, the security model,
and the test strategy are reviewed.

The product can meet the requested core workflows with established native
libraries:

- The Sleuth Kit for partition and filesystem analysis.
- libewf for E01/EWF access and creation.
- Optional libyal libraries for NTFS-specific, shadow-copy, VHD, and VMDK
  support.
- Qt 6 for the desktop application.
- Rust and Tokio for the core, jobs, persistence, hashing, extraction, and
  reporting.

The project cannot honestly claim byte-for-byte FTK Imager compatibility. The
supplied FTK artifact is an opaque Windows InstallShield bootstrapper, no
authorized Windows runtime was available, and the requested product is
Linux-native. Compatibility must be expressed as tested workflow and format
coverage.

The first production release should claim:

- RAW acquisition and verification.
- E01 acquisition and verification against a pinned libewf version.
- MBR and GPT partition listing.
- Read-only NTFS, FAT32, exFAT, and ext4 navigation.
- Bounded preview, metadata inspection, extraction, reporting, and audit
  logging.

It should not initially claim:

- FTK Imager UI or API compatibility.
- AD1, L01, AFF, S01, or encrypted EWF creation.
- EWF2 write support.
- Recovery of every deleted file.
- Accurate conversion of every filesystem timestamp to UTC.
- A software-only substitute for a hardware write blocker.
- Physical acquisition from a sandboxed Flatpak without widening its device and
  filesystem permissions.

## 2. Scope and method

The assessment used four evidence classes:

1. Direct inspection of the supplied FTK Imager 8.3.0.27 installer with REA,
   PE parsers, byte inspection, and archive probes.
2. Official Exterro product, release-note, user-guide, and download pages.
3. Upstream forensic-library source, releases, and licenses.
4. Executable host checks and small synthetic filesystem and partition images
   generated in `/tmp`.

No production evidence was used. No Windows installer was executed. No
licensing, access-control, or encryption protection was bypassed. The embedded
installer payload was not decrypted or reverse engineered.

Evidence status terms:

- Confirmed: directly reproduced from an artifact, command, or executable test.
- Observed: directly observed in static artifact data, source code, or official
  documentation, without executing the described behavior.
- Inferred: a technical conclusion drawn from observed evidence.
- Unknown: not established by available evidence.

Detailed records are in
[docs/reverse-engineering/evidence-catalog.md](docs/reverse-engineering/evidence-catalog.md).

## 3. Findings

### F-001: FTK installer identity

- Description: The supplied installer is a 229,763,592-byte PE32 Windows
  installer named `Exterro_FTK_Imager_(x64).exe`, version 8.3.0.27.
- Evidence: SHA-256
  `1ec9500126b9b33e79e8178ffa12104377f2b77ddc8fca69d8fa1068d8e360d8`;
  `file` identified a PE32 GUI executable; PE resource metadata reported
  Exterro FTK Imager and InstallShield version 27.
- Reproduction: `sha256sum` and `file -k` on the supplied artifact; REA
  `inspect-pe-imports` with a 300,000,000-byte read budget.
- Confidence: Confirmed.
- Technical implication: The artifact is an installer launcher, not the
  application package itself.
- Implementation recommendation: Record the hash in the evidence catalog and
  exclude the artifact from version control.
- Known limitation: The artifact proves installer identity only.

### F-002: Installer is a native InstallShield bootstrapper

- Description: The launcher imports native Win32 APIs, dynamically loads
  `msi.dll`, `VERSION.dll`, `WININET.dll`, `CRYPT32.dll`, and `WINTRUST.dll`,
  and contains InstallShield setup strings.
- Evidence: REA evidence
  `ev_9cdd153b746177df28c7cb0c59b6765b86aa15c0e0e7f5890f97c4f7b8a3d41a`;
  18 imported DLLs, 366 imports, and 60 delay imports were parsed.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-imports <installer> --max-file-bytes 300000000`
- Confidence: Observed.
- Technical implication: Static analysis of this PE describes installer
  bootstrap behavior, not FTK's acquisition engine or UI.
- Implementation recommendation: Do not infer FTK application architecture
  from the bootstrapper imports.
- Known limitation: Runtime binding, reachability, and actual loaded modules
  were not observed.

### F-003: The installer contains a code-signing structure, not a verified trust decision

- Description: The PE certificate table contains one PKCS#7 signed-data entry
  with five certificates. The signer certificate subject names ACCESSDATA
  GROUP, INC.
- Evidence: REA evidence
  `ev_01f2ec8d1304d15134917653ba5231b2b7aef4cb3712413cce958c7f72d533f8`.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-certificates <installer> --max-file-bytes 300000000`
- Confidence: Observed.
- Technical implication: Certificate presence does not prove a valid
  Authenticode signature, trusted chain, timestamp, or unmodified PE digest.
- Implementation recommendation: If Windows provenance matters to a reviewer,
  verify Authenticode separately on an authorized Windows host.
- Known limitation: Signature cryptographic validity and chain trust were not
  verified.

### F-004: The embedded product payload is opaque

- Description: An `MSCF` signature starts at byte offset 73,031,674, but the
  following bytes do not form a usable cabinet header. `cabextract` rejects the
  carved payload, and 7-Zip cannot extract it. The first 1 MiB of the carved
  region has block entropy around 7.95 bits per byte.
- Evidence: Byte scan, `cabextract -l`, 7-Zip extraction probe, REA evidence
  `ev_8cc8b454279326b7e901090b4622615e6c3e018a638e12e353aef317517fef2c`.
- Reproduction: locate the `MSCF` signature, carve from that byte offset, probe
  with CAB tools, and run REA `inspect-byte-entropy`.
- Confidence: Confirmed for tool rejection; unknown for the exact payload
  transform.
- Technical implication: This artifact does not permit static analysis of the
  actual FTK application payload without additional authorized material or
  bypassing protection.
- Implementation recommendation: Stop at the packaging boundary. Do not attempt
  to defeat encryption or access controls.
- Known limitation: High entropy is consistent with encryption or compression
  and does not identify the transform. The product's internal architecture
  remains unknown.

### F-005: FTK Imager is officially Windows-only

- Description: Exterro's current FTK Imager page documents Windows
  installation and states that it can image a Linux device but cannot be
  installed on Linux.
- Evidence: [Exterro FTK Imager product page](https://www.exterro.com/digital-forensics-software/ftk-imager),
  accessed 2026-10-30; official download page lists Windows 10, Windows 11,
  Windows Server 2022, Windows Server 2025, and Windows Server 2019 for newer
  releases.
- Reproduction: Fetch the official product and download pages and inspect the
  operating-system and FAQ sections.
- Confidence: Confirmed by vendor documentation.
- Technical implication: OpenForensic Imager is an independent Linux product,
  not a port or compatibility layer.
- Implementation recommendation: Benchmark workflows, not implementation.
- Known limitation: Vendor pages can change and may be incomplete.

### F-006: FTK documents a dockable, multi-pane inspection UI

- Description: The official 8.2 user guide documents a menu bar, toolbar,
  Evidence Tree, File List, Viewer, Properties, Hex Value Interpreter, and
  Custom Content Sources panes. The panes are dockable and resizable.
- Evidence: Official
  [FTK Imager 8.2 User Guide](https://d1kpmuwb7gvu1i.cloudfront.net/Imager/8.2/Exterro%20FTK%20Imager%208.2%20-%20User%20Guide%20.pdf),
  SHA-256
  `f836ef3098a75236ea33246e6af2329177d12a57f52f839ed2ff2116a065cd7a`;
  see "The FTK Imager User Interface".
- Reproduction: Download the guide, extract text, and inspect the UI chapter
  and toolbar table.
- Confidence: Observed in official documentation.
- Technical implication: A professional inspection UI should preserve the
  evidence hierarchy, file list, viewer, properties, and status context.
- Implementation recommendation: Use a C++ `QMainWindow` shell with dockable
  QML panes and Qt item models.
- Known limitation: Pixel layout, interaction details, and current 8.3 UI
  changes were not observed.

### F-007: FTK acquisition supports physical and logical sources and multiple output formats

- Description: The guide documents complete drive or partition imaging,
  custom-content AD1 images, RAW/DD, E01, S01, AFF, segment sizing, compression,
  encryption, destination metadata, progress, and image summary.
- Evidence: FTK Imager 8.2 User Guide, "Creating Forensic Images"; official
  FTK Imager page also describes physical imaging, preview, and custom content.
- Reproduction: Inspect the guide's "Select Source", "Select Image Type",
  "Select Image Destination", and "Image Summary" sections.
- Confidence: Observed in official documentation.
- Technical implication: RAW and E01 creation are table stakes. AD1 and other
  formats are separate scope decisions.
- Implementation recommendation: Release 0.1 supports RAW and E01 only. Add
  other formats only after their writers and differential tests exist.
- Known limitation: Exact default parameters and error handling were not
  executed or confirmed.

### F-008: FTK verification recomputes hashes and compares embedded image hashes

- Description: The guide describes MD5 and SHA-1 calculation for a drive or
  image, comparison with a stored hash for E01/S01, sector count, and bad-sector
  count in the verification results.
- Evidence: FTK Imager 8.2 User Guide, "Verifying Drives and Images".
- Reproduction: Inspect the guide's verification workflow and results list.
- Confidence: Observed in official documentation.
- Technical implication: Verification is a distinct operation with explicit
  stored and computed values and a match result.
- Implementation recommendation: Use a verification result enum with
  `Match`, `Mismatch`, `NotAvailable`, and `Error`. Never treat an unavailable
  hash as a match.
- Known limitation: Exact handling of corrupt chunks, missing segments, and
  I/O errors was not observed.

### F-009: FTK's documented filesystem and image support is broader than the initial release scope

- Description: The 8.2 guide lists APFS, HFS, HFS+, NTFS, FAT, exFAT, ext2/3/4,
  ReiserFS3, XFS, UFS, ISO9660, and other file systems. It also lists EnCase,
  Expert Witness, Linux DD, SMART, AD1, AFF, and several optical image formats.
- Evidence: FTK Imager 8.2 User Guide, Appendix A.
- Reproduction: Inspect the Appendix A file-system and image-format tables.
- Confidence: Observed in official documentation.
- Technical implication: Format by format support cannot be inferred from a
  list alone.
- Implementation recommendation: Maintain a compatibility matrix that
  separates documented, implemented, differentially tested, and unsupported
  status.
- Known limitation: The vendor list is not an independent compatibility test.

### F-010: FTK's documented preview model includes external application execution risk

- Description: The guide states that unsupported file types may be displayed
  outside FTK Imager and warns about active Internet content when previewing
  cached HTML.
- Evidence: FTK Imager 8.2 User Guide, "Preview Modes" and its warning adjacent
  to the preview chapter.
- Reproduction: Inspect "Preview Modes" and the warning near the start of
  "Working with Evidence".
- Confidence: Observed in official documentation.
- Technical implication: Automatically opening evidence with host applications
  is unacceptable for untrusted input.
- Implementation recommendation: Never execute or automatically open evidence.
  Use built-in bounded text, hex, image, and optional PDF viewers with external
  network and file access denied.
- Known limitation: Current FTK 8.3 behavior was not observed.

### F-011: The primary host has a workable Rust and Qt toolchain

- Description: Ubuntu 26.04.1 with Linux 7.0, Rust 1.93.1, Qt 6.10.2, CMake
  4.2.3, Ninja 1.13.2, GCC 15.2.0, Clang 21.1.8, SQLite, and standard
  diagnostics are installed.
- Evidence: Direct host command inventory, 2026-10-30.
- Reproduction: `cargo --version`, `qmake6 -query QT_VERSION`, `cmake
  --version`, `ninja --version`, and `pkg-config --modversion Qt6Core`.
- Confidence: Confirmed.
- Technical implication: Phase 1 can begin without installing a compiler stack.
- Implementation recommendation: Pin Rust in `rust-toolchain.toml`; use CMake
  and Ninja for the native build.
- Known limitation: Qt PDF, Flatpak builder, AppImage tools, and native
  development headers for several optional libyal libraries are absent.

### F-012: Distribution forensic libraries are inconsistent and stale

- Description: Ubuntu 26.04 ships TSK 4.12.1 and libewf 20140816 from the legacy
  branch. Arch ships TSK 4.15.0 and libewf 20140816 marked out of date. Fedora
  Rawhide packages TSK 4.15.0 and libewf 20140608. Debian lists newer TSK and
  optional libyal versions than Ubuntu.
- Evidence: Host `apt-cache`, Arch package JSON, Fedora package pages and
  Rawhide specs, Debian source API.
- Reproduction: Query the package manager or distribution package metadata for
  `libewf`, `sleuthkit`, `libfsntfs`, `libvshadow`, `libvhdi`, and `libvmdk`.
- Confidence: Confirmed for package metadata on 2026-10-30.
- Technical implication: Depending on arbitrary distribution versions would
  produce different forensic results across supported systems.
- Implementation recommendation: Pin and bundle reviewed native dependency
  versions, record their hashes and build flags, and expose them in every
  report.
- Known limitation: Package state changes quickly; release engineering must
  refresh the matrix.

### F-013: libewf is active but experimental, and EWF2 write support is not a safe initial claim

- Description: Upstream libewf identifies itself as experimental. It supports
  EWF1 E01 and S01 reading/writing, lists EWF2 Ex01 support, but also lists
  EWF2 write support as work in progress, with bzip2 EWF2 and EWF2 encryption
  unsupported. The newest release is a pre-release.
- Evidence: [libewf README](https://github.com/libyal/libewf/blob/main/README)
  and [EWF2 specification](https://github.com/libyal/libewf/blob/main/documentation/Expert%20Witness%20Compression%20Format%202%20%28EWF2%29.asciidoc);
  latest package release tag `20260924`, published 2026-09-25.
- Reproduction: Fetch upstream README, release metadata, and format
  specifications.
- Confidence: Observed in upstream source and release metadata.
- Technical implication: E01 is feasible, but production acceptance requires a
  separately pinned build, corruption tests, and `ewfverify` differential tests.
- Implementation recommendation: Release 0.1 supports EWF1 E01 creation and
  read. Treat EWF2 and encrypted EWF as unsupported until tested.
- Known limitation: Upstream documentation contains contradictions about EWF2
  write maturity, so test evidence must decide.

### F-014: The Sleuth Kit covers the required initial partition and filesystem set

- Description: TSK 4.15 defines MBR/DOS and GPT volume systems and NTFS, FAT12,
  FAT16, FAT32, exFAT, ext2, ext3, and ext4 filesystem types. The installed TSK
  4.12.1 tools successfully parsed synthetic MBR, GPT, NTFS, FAT32, exFAT, and
  ext4 images.
- Evidence: Upstream `tsk/fs/tsk_fs.h`, `tsk/vs/tsk_vs.h`, and host tests.
- Reproduction: `fls -V`, `fsstat -i list`, `mmls` on synthetic tables, and
  `fsstat`/`fls` on synthetic filesystem images.
- Confidence: Confirmed for parsing the synthetic fixtures with 4.12.1;
  observed for 4.15 support declarations.
- Technical implication: TSK is the correct primary engine and avoids
  reimplementing mature filesystem parsers.
- Implementation recommendation: Build a pinned TSK version and wrap only its
  stable C API.
- Known limitation: Parsing is not proof of complete semantic or metadata
  accuracy.

### F-015: Current TSK timestamp conversion has material forensic limitations

- Description: Source inspection of the TSK 4.15 development branch found:
  FAT and exFAT DOS timestamps are converted with `mktime`, so the process local
  timezone and daylight-saving state affect the returned epoch; the exFAT UTC
  offset field is not applied by the conversion path; NTFS conversion returns
  zero for values before 1970 or after the 32-bit epoch limit; ext4 timestamp
  extra fields are shifted for nanoseconds but epoch bits are not applied.
- Evidence: TSK source files `tsk/fs/fatfs_utils.c`, `tsk/fs/exfatfs_meta.c`,
  `tsk/fs/ntfs.c`, and `tsk/fs/ext2fs.c`, reviewed 2026-10-30. The synthetic
  `fsstat` output also displayed host timezone WIB.
- Reproduction: Inspect `fatfs_dos_2_unix_time`, the exFAT metadata copy path,
  `nt2unixtime`, and the ext4 inode timestamp fields in the cited source.
- Confidence: Confirmed by source inspection; runtime boundary behavior still
  needs dedicated tests.
- Technical implication: Silent UTC conversion would be misleading.
- Implementation recommendation: Store raw and interpreted timestamps with
  provenance, timezone assumption, precision, and validity status. Mark FAT and
  exFAT evidence timestamps as requiring an examiner timezone assumption.
- Known limitation: The finding is based on upstream source and must be
  revalidated against the exact pinned TSK commit.

### F-016: Dependency licenses require a project-license decision

- Description: libewf and the optional libyal libraries are LGPL-3.0-or-later.
  TSK uses a mixture of CPL-1.0, IBM Public License, GPL, MIT, BSD, and Apache
  components depending on file and tool. Qt offers LGPL/GPL/commercial terms.
- Evidence: Upstream repository license metadata, TSK `licenses/`, Debian
  copyright files, and Fedora TSK license metadata.
- Reproduction: Inspect the cited upstream license directories and package
  copyright files.
- Confidence: Observed.
- Technical implication: Distribution and static-linking choices can change
  obligations.
- Implementation recommendation: Keep the project license undecided pending
  legal review. Use dynamic linking where it preserves compliance options and
  publish a dependency-bill-of-materials.
- Known limitation: This report is not legal advice.

### F-017: Packaging tools and the Qt PDF module are not yet installed

- Description: `dpkg-deb` and CPack are available, while `flatpak-builder`,
  `appimagetool`, `linuxdeploy`, and the Qt PDF development module are absent.
- Evidence: Host executable inventory and `apt-cache policy qt6-pdf-dev`.
- Reproduction: `command -v flatpak-builder appimagetool linuxdeploy`;
  `pkg-config --modversion Qt6Pdf`; `apt-cache policy qt6-pdf-dev`.
- Confidence: Confirmed.
- Technical implication: Packaging and PDF preview are Phase 4 and Phase 7
  setup work, not current blockers.
- Implementation recommendation: Keep PDF optional at build time. Package the
  native DEB first; validate Flatpak physical access separately.
- Known limitation: AppImage and Flatpak behavior is not yet tested on a
  clean system.

### F-018: Synthetic fixtures are practical and reproduce the required format coverage

- Description: Independent fixture images were generated in `/tmp` and parsed
  by TSK. Ext4 deletion was reproduced with `debugfs`; MBR and GPT layouts
  matched `sfdisk` and `sgdisk` output.
- Evidence: Feasibility commands on 2026-10-30, recorded in
  [docs/testing/fixtures.md](docs/testing/fixtures.md).
- Reproduction: Generate images with `mke2fs`, `mkntfs`, `mkfs.vfat`,
  `mkfs.exfat`, `sfdisk`, and `sgdisk`, then parse with `fsstat`, `fls`, and
  `mmls`.
- Confidence: Confirmed for the run.
- Technical implication: The test strategy can avoid production evidence.
- Implementation recommendation: Commit a deterministic fixture generator and
  manifest, not large generated binaries.
- Known limitation: The exploratory fixture hashes depend on generation time,
  identifiers, and tool versions. They are not release golden files.

## 4. Subsystem feasibility

| Module | Feasibility | Decision and qualification |
|---|---|---|
| A. Evidence Manager | Feasible | SQLite case database with foreign keys, migrations, source records, evidence identifiers, and immutable event history. |
| B. Disk Acquisition | Feasible with hardware dependency | RAW is low risk. E01 depends on a pinned libewf build. Physical sources require Linux device permissions and an external write blocker. Software can open sources read-only but cannot guarantee a write blocker. |
| C. Hash Engine | Feasible | Streaming Rust hash engines for MD5, SHA-1, SHA-256, and SHA-512. Hash scope and unreadable data handling must be explicit. |
| D. Image Reader | Feasible | RAW reader is Phase 1. E01 and segmented reads use libewf in Phase 2. EWF2 and encrypted EWF are not initial production claims. |
| E. Partition Analyzer | Feasible | Use TSK volume-system APIs behind a safe Rust wrapper. Cross-check MBR and GPT with `mmls`, `sfdisk`, and `sgdisk`. |
| F. Filesystem Explorer | Feasible with semantic limits | Use TSK for NTFS, FAT32, exFAT, and ext4. Deleted-file capabilities vary. Timestamp provenance must be preserved. |
| G. Evidence Viewer | Feasible | Hex and bounded text preview are low risk. Image preview needs decode and pixel limits. PDF preview is optional and must be isolated and bounded. No auto-execution. |
| H. File Extraction | Feasible | Use TSK file reads and capability-based output paths. Reject traversal, symlinks, special files, and ambiguous conflicts by default. |
| I. Reporting | Feasible | JSON, CSV, and HTML in the core. PDF through Qt `QPdfWriter` from generated HTML or structured text, with HTML escaping and bounded content. |
| Desktop UI | Feasible, large scope | C++ Qt shell with QML panes and item models. Non-blocking jobs and lazy tree models are mandatory. |
| Audit logging | Feasible with honest limits | Append-only hash-chained JSONL plus fsync. Tamper evidence is local and can be defeated by an attacker with full control of the case directory unless a final digest is externally recorded. |
| Packaging | Feasible with platform caveats | DEB first. AppImage can bundle pinned libraries. Flatpak is suitable for analysis, not unrestricted physical acquisition, unless device permissions are widened. |

## 5. Dependency assessment

| Dependency | Upstream status | Host availability | License | Recommendation | Key risk |
|---|---|---|---|---|---|
| libewf | Active, marked experimental; E01/S01 support; EWF2 write still described as work in progress | 20140816 legacy package installed; command tools absent | LGPL-3.0-or-later | Vendor a reviewed commit/release, build with CMake, expose version in reports | Very old distro package and evolving EWF2 support |
| The Sleuth Kit | Active; 4.15.0 released 2026-04-15 | 4.12.1 runtime and CLI installed; headers absent | Mixed CPL-1.0, IBM-PL, GPL, MIT, BSD, Apache by component | Pin 4.15.0 or newer reviewed commit; dynamic link; legal review | Cross-distro version drift and timestamp semantics |
| libfsntfs | Active upstream | Ubuntu package available; headers absent | LGPL-3.0-or-later | Optional NTFS differential and attribute engine | Additional parser surface and missing cross-distro packages |
| libvshadow | Active upstream | Ubuntu package available; headers absent | LGPL-3.0-or-later | Defer until shadow-copy scope is approved | Missing Fedora/Arch official packages |
| libvhdi | Active upstream | Runtime present; headers absent | LGPL-3.0-or-later | Optional image-reader backend in a later phase | Virtual-disk formats expand testing scope |
| libvmdk | Active upstream | Runtime present; headers absent | LGPL-3.0-or-later | Optional image-reader backend in a later phase | Virtual-disk formats expand testing scope |
| Qt 6 | Active | 6.10.2 base and declarative installed | LGPL/GPL/commercial | Use installed Qt for development; bundle for AppImage | Qt PDF missing; Flatpak/AppImage tooling absent |
| Rust crates | Active | Cargo 1.93.1 available | Per crate | Pin through `Cargo.lock`, audit before release | Supply-chain and maintenance review required |
| SQLite | Mature | Installed | Public domain | Use bundled or system SQLite through `rusqlite` with reviewed features | Concurrent audit and case database writes |

The exact native dependency commits, configure flags, and hashes must be
recorded in `THIRD_PARTY.md` before the first release build.

## 6. Platform assessment

### Ubuntu 26.04 LTS

Primary development and release target. The compiler and Qt stack are present.
The recommended delivery is a native DEB plus an AppImage. The application
should run as the examiner user, with explicit device-group access where
physical acquisition is required.

### Debian

Feasible. Debian package versions differ from Ubuntu, so native dependencies
must be bundled or built from pinned sources for consistent results.

### Arch Linux

Feasible. TSK 4.15 is available, but libewf is old and flagged out of date.
Optional libyal packages are not all in the official repositories.

### Fedora

Feasible. TSK 4.15 is packaged, libewf is old, and optional libyal libraries are
not confirmed in official repositories. The same pinned build strategy is
required.

### ARM64

Future work. Avoid architecture-specific integer widths, unaligned reads, and
assumptions about native endianness. Add an ARM64 build and differential test
matrix only after x86_64 release validation.

## 7. Required design corrections from feasibility

1. Pin native forensic dependencies rather than consuming arbitrary distro
   versions.
2. Make E01 Hash values distinct from independent SHA-256 and SHA-512 image
   verification.
3. Preserve timestamp provenance instead of claiming all filesystem times are
   UTC.
4. Separate analysis into a worker process to contain malformed-input crashes
   in native parsers.
5. Implement acquisition output as a staged, manifest-backed job with explicit
   `succeeded`, `partial`, `failed`, and `cancelled` states.
6. Use capability-relative extraction with `openat2` and no symlink following.
7. Keep the audit log append-only from the first evidence write, even if full
   audit reporting arrives in a later phase.
8. Treat physical acquisition as a documented privileged workflow that still
   requires an external write blocker.

## 8. Implementation roadmap

| Phase | Deliverable | Exit gate |
|---|---|---|
| 0 | Feasibility, architecture, requirements, security model, test strategy | Reviewer approves the documents and dependency policy |
| 1 | Workspace, core types, streaming hashes, RAW reader, evidence metadata | Unit and integration tests pass with deterministic RAW fixtures |
| 2 | RAW acquisition, pinned libewf E01 acquisition/read, verification, reports | Every acquisition has a matching verification test; corrupted and partial cases are explicit |
| 3 | MBR/GPT, TSK partition and filesystem wrappers, metadata, deleted-file status | Differential tests against TSK tools and independent filesystem tools |
| 4 | Viewer, hex model, bounded previews, extraction, extraction reports | Path traversal and resource limit tests pass |
| 5 | Qt desktop shell, QML panes, item models, jobs, evidence manager integration | Keyboard-only and GUI automation tests pass |
| 6 | Full audit reports, fuzzing, differential suite, security validation | No open critical/high findings; integrity scenarios pass |
| 7 | DEB, AppImage, Flatpak assessment, installation and reproducible build docs | Clean-system install and acquisition/verification smoke tests pass |

The detailed Phase 1 task plan is in
[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).

## 9. Unsupported or unresolved requirements

- No authorized runtime FTK installation was available, so exact current FTK
  behavior, error dialogs, wizard state transitions, and corrupt-image handling
  remain unknown.
- The installer payload could not be inspected without bypassing an opaque
  packaging layer, which this project will not do.
- SHA-256 is not part of the EWF1 stored MD5/SHA1 pair. It will be an
  independent sidecar and report value.
- E01 unreadable-sector handling must be proven with an injected read-error
  source. Ordinary files cannot simulate all physical read failures.
- FAT and exFAT timestamps cannot be assigned a unique UTC instant without an
  examiner timezone assumption. exFAT's stored UTC offset needs separate
  treatment outside the current TSK conversion path.
- TSK's NTFS and ext4 time span limitations require tests and explicit display
  status.
- True deleted-file recovery depends on filesystem state. The application may
  identify deleted metadata or recover content only when the necessary data
  still exists.
- A software guard cannot replace a hardware write blocker.
- Flatpak physical acquisition needs additional privilege and permission
  design.

## 10. Review decision requested

Approve or revise the following before Phase 1:

1. The pinned-native-library strategy.
2. The C ABI plus isolated worker process architecture.
3. The timestamp provenance model.
4. The initial scope of RAW and EWF1 E01 only.
5. The project license remains undecided pending legal review.

After approval, Phase 1 can be implemented without changing the evidence
integrity model.

