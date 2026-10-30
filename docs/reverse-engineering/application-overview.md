# FTK Imager Application Overview

Investigation date: 2026-10-30

Scope: Authorized inspection of the supplied FTK Imager 8.3.0.27 installer and
review of official Exterro documentation. No FTK process was executed.

## Summary

The supplied artifact is an InstallShield launcher for a Windows application.
Official Exterro documentation describes FTK Imager as a Windows-only preview
and imaging tool for disk and logical evidence. The installer's embedded
product payload is opaque to standard archive tooling and REA package
inspection, so the actual FTK application's module structure and runtime
behavior remain unestablished.

OpenForensic Imager therefore uses FTK only as a documented workflow reference.
It is an independent Linux implementation built on public format
specifications, public libraries, and synthetic test fixtures.

## Product identity

| Fact | Value | Status |
|---|---|---|
| Vendor | Exterro | Observed in PE metadata and official site |
| Product | Exterro FTK Imager | Observed |
| Supplied version | 8.3.0.27 | Observed |
| Current download page version | 8.3.1.6 | Observed on 2026-10-30 |
| Installer SHA-256 | `1ec9500126b9b33e79e8178ffa12104377f2b77ddc8fca69d8fa1068d8e360d8` | Confirmed |
| Installer size | 229,763,592 bytes | Confirmed |
| Installer format | PE32, InstallShield Setup Launcher Unicode | Confirmed |
| Official OS support | Windows install only | Observed in official documentation |
| Supplied artifact runtime | Not executed | Unknown behavior |

## Findings

### APP-F-001: The supplied artifact is an installer, not the FTK application

- Description: The PE contains InstallShield setup strings and imports MSI,
  network, certificate, and trust APIs. It is a native bootstrap layer.
- Evidence: [FEASIBILITY_REPORT.md](../../FEASIBILITY_REPORT.md) F-001 and
  F-002; evidence EV-002 and EV-003.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-imports <installer> --max-file-bytes 300000000`
  and inspect InstallShield resources and strings.
- Confidence: Confirmed for static artifact identity and imports.
- Technical implications: Imports describe the installer, not FTK's acquisition
  engine.
- Implementation recommendation: Do not use installer imports as an FTK
  architecture specification.
- Known limitations: No runtime observation of loaded modules or reachable code.

### APP-F-002: No managed application payload was exposed by the launcher

- Description: Resource and archive probes exposed the native bootstrapper and
  resource assets but not a directly inspectable managed FTK assembly.
- Evidence: REA artifact inventory EV-002; 7-Zip PE listing; InstallShield
  string extraction.
- Reproduction: List the PE with `7z l -slt`; run REA `inspect-artifact`; inspect
  resources.
- Confidence: Observed for the bootstrapper; application payload is unknown.
- Technical implications: The implementation language and module graph of FTK
  cannot be established from this layer.
- Implementation recommendation: Mark the FTK application architecture unknown.
- Known limitations: The payload may be inside an opaque container that was not
  accessed.

### APP-F-003: The embedded payload is not a normal readable CAB

- Description: The `MSCF` marker at offset 73,031,674 is followed by bytes that
  do not parse as a normal cabinet. `cabextract` and 7-Zip reject the carved
  region.
- Evidence: EV-004; REA byte entropy evidence `ev_8cc8...`; CAB extraction
  probes.
- Reproduction: Find the marker, carve to the end of the installer, and run
  `cabextract -l` and `7z l -tCAB`.
- Confidence: Confirmed for extraction failure; transform is unknown.
- Technical implications: Standard package tooling cannot inventory the product
  payload.
- Implementation recommendation: Stop at the package boundary and do not bypass
  protection.
- Known limitations: High entropy does not distinguish compression from
  encryption.

### APP-F-004: The installer carries a code-signing certificate table

- Description: The PE certificate table has one PKCS#7 entry and five
  certificates, including an ACCESSDATA GROUP signer certificate.
- Evidence: REA evidence
  `ev_01f2ec8d1304d15134917653ba5231b2b7aef4cb3712413cce958c7f72d533f8`.
- Reproduction:
  `npx -y rea-agents@6.8.0 inspect-pe-certificates <installer> --max-file-bytes 300000000`
- Confidence: Observed.
- Technical implications: This is structural signature data, not proof that the
  image signature and chain are valid.
- Implementation recommendation: Perform Authenticode verification separately
  if an examiner needs provenance evidence.
- Known limitations: Signature validity was not verified.

### APP-F-005: Official FTK documentation defines the workflow reference

- Description: Exterro documents physical and logical preview, imaging,
  evidence tree navigation, file list, properties, hex interpretation, export,
  mounting, hash verification, and several output formats.
- Evidence: EV-006, EV-007, and EV-008.
- Reproduction: Download the official 8.2 user guide and 8.3 release notes; read
  the UI, evidence, imaging, export, and verification chapters.
- Confidence: Observed in official documentation.
- Technical implications: These are legitimate public workflow references.
- Implementation recommendation: Implement equivalent workflows independently
  and test each one.
- Known limitations: The guide version is 8.2, while the supplied installer is
  8.3.0.27 and the current vendor page offers 8.3.1.6.

### APP-F-006: FTK is a Windows product, not a Linux application

- Description: Exterro states that FTK Imager can image a Linux drive but cannot
  be installed on Linux.
- Evidence: EV-007.
- Reproduction: Read the official FTK Imager product FAQ.
- Confidence: Confirmed by vendor documentation.
- Technical implications: There is no Linux FTK Imager runtime to replicate.
- Implementation recommendation: Build a Linux-native architecture and compare
  workflows, not implementation details.
- Known limitations: Vendor marketing and FAQ wording can change.

### APP-F-007: The current FTK version differs from the supplied artifact

- Description: The official 8.3.1 download page listed version 8.3.1.6 and MD5
  `9b709146cf59825981e622b8ea11b762` on 2026-10-30.
- Evidence: EV-009.
- Reproduction: Fetch `https://www.exterro.com/ftk-downloads/ftk-imager-8-3-1`.
- Confidence: Observed.
- Technical implications: Behavior found for 8.3.0.27 may not describe 8.3.1.6.
- Implementation recommendation: Label all FTK findings with an exact version.
- Known limitations: No 8.3.1.6 runtime was available.

## Ethical and legal boundary

This investigation:

- Used the supplied installer as authorized input.
- Did not execute the Windows installer on the Linux host.
- Did not decrypt or defeat the embedded package.
- Did not extract proprietary source or secrets.
- Did not use protected assets in OpenForensic Imager.
- Did not claim unobserved behavior as confirmed.

The FTK installer is excluded from git.

