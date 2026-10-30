# FTK Imager Feature Inventory

Status values:

- Documented: described in official Exterro material.
- Observed: directly visible in the installer or host evidence.
- Runtime confirmed: directly executed and recorded. None for FTK in this phase.
- Unknown: not established.

## Documented features

| ID | Feature | FTK evidence status | OpenForensic target | Phase |
|---|---|---|---|---|
| FTK-UI-01 | Dockable evidence tree, file list, viewer, properties, and hex interpreter | Documented, not runtime confirmed | Equivalent QML panes and models | 5 |
| FTK-UI-02 | Menu bar and toolbar | Documented | Native Qt shell | 5 |
| FTK-UI-03 | Automatic, text, and hex preview modes | Documented | Bounded text and paged hex viewers | 4 |
| FTK-UI-04 | Background progress with throughput, elapsed, and remaining time | Documented | Task manager and typed progress events | 5 |
| FTK-UI-05 | Image summary | Documented | Acquisition report and summary view | 2, 5 |
| FTK-ACQ-01 | Physical drive and partition imaging | Documented | Read-only source acquisition | 2 |
| FTK-ACQ-02 | Logical drive or source imaging | Documented | File or logical source acquisition | 2 |
| FTK-ACQ-03 | RAW/DD output | Documented | Required | 2 |
| FTK-ACQ-04 | E01 output | Documented | Required EWF1 E01 | 2 |
| FTK-ACQ-05 | S01 output | Documented | Not initial | Deferred |
| FTK-ACQ-06 | AFF output | Documented | Not initial | Deferred |
| FTK-ACQ-07 | AD1 custom-content output | Documented | Not initial | Deferred |
| FTK-ACQ-08 | Compression selection | Documented | Required for E01 | 2 |
| FTK-ACQ-09 | Segment sizing | Documented | Required for RAW split and E01 | 2 |
| FTK-ACQ-10 | Multiple simultaneous destinations | Documented | Optional later | Deferred |
| FTK-ACQ-11 | AD or AFF encryption | Documented | Explicitly unsupported initially | Deferred |
| FTK-ACQ-12 | CD/DVD imaging | Documented | Not initial | Deferred |
| FTK-HASH-01 | MD5 file and image hashes | Documented | Required | 1, 2 |
| FTK-HASH-02 | SHA-1 file and image hashes | Documented | Required | 1, 2 |
| FTK-HASH-03 | SHA-256 and SHA-512 independent hashes | Not found in 8.2 guide workflow | Required extension | 1, 2 |
| FTK-HASH-04 | File hash list export | Documented | Inventory and extraction hashes | 4 |
| FTK-VER-01 | Verify drive or image | Documented | Required | 2 |
| FTK-VER-02 | Compare computed and embedded E01/S01 hash | Documented | Required for available embedded hashes | 2 |
| FTK-VER-03 | Count bad sectors | Documented | Read-error range accounting | 2 |
| FTK-IMG-01 | Open RAW/dd images | Documented | Required | 1 |
| FTK-IMG-02 | Open E01 images | Documented | Required | 2 |
| FTK-IMG-03 | Open S01, AFF, AD1, L01 | Documented | Not initial | Deferred |
| FTK-IMG-04 | Mount image read-only or physical/logical | Documented | Not initial; no automatic mount | Deferred |
| FTK-FS-01 | NTFS browsing | Documented | Required | 3 |
| FTK-FS-02 | FAT12/16/32 browsing | Documented | FAT32 required | 3 |
| FTK-FS-03 | exFAT browsing | Documented | Required | 3 |
| FTK-FS-04 | ext2/3/4 browsing | Documented | ext4 required | 3 |
| FTK-FS-05 | APFS, HFS, XFS, UFS, ReiserFS, ISO9660 | Documented | Not initial | Deferred |
| FTK-FS-06 | Deleted names and Recycle Bin recovery | Documented | Deleted metadata and recoverable content where supported | 3 |
| FTK-FS-07 | Orphan metadata files | Documented indirectly by TSK and visible in feasibility probes | Required as TSK exposes | 3 |
| FTK-EXP-01 | Export files and folders | Documented | Required | 4 |
| FTK-EXP-02 | Export AD1 logical image | Documented | Not initial | Deferred |
| FTK-EXP-03 | Export directory listing | Documented | Required | 4 |
| FTK-REP-01 | Acquisition report and image summary | Documented | Required | 2 |
| FTK-REP-02 | Verification report | Documented | Required | 2 |
| FTK-REP-03 | Examination history | Not confirmed as a complete chain-of-custody feature | Required OpenForensic audit extension | 6 |
| FTK-SEC-01 | Treat source as unchanged | Documented as a claim and write-blocker requirement | Required with executable safety tests | 1, 2 |
| FTK-SEC-02 | Evidence execution risk from preview | Documented warning | No auto-execution or external viewer | 4 |

## Features outside the requested core

| Feature | Reason for exclusion |
|---|---|
| RAM capture | Not one of the 15 requested objectives |
| Registry capture | Not one of the requested objectives |
| Mobile/iOS extraction | Not one of the requested objectives |
| MetaCarve/deep carving | Not required for initial filesystem support |
| SID-based export | Not in the initial compatibility boundary |
| EFS encryption detection | Deferred security-sensitive feature |
| Whole-disk decryption | Deferred and requires credential handling |
| CD/DVD optical acquisition | Deferred |

## Confidence limits

- The feature inventory uses the official 8.2 user guide and current product
  pages. FTK 8.3 runtime behavior was not observed.
- Presence in documentation does not prove correctness on corrupted evidence.
- No listed FTK feature has been graded as OpenForensic-compatible.
- OpenForensic status is "planned" until the corresponding tests pass.

