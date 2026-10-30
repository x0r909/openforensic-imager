# FTK Imager Architecture Analysis

## Scope

This document analyzes only what can be established from the supplied
8.3.0.27 installer and official documentation. It does not reconstruct
proprietary source or protected payload contents.

## Established facts

### ARC-F-001: The visible bootstrapper is native Win32 code

- Finding ID: ARC-F-001
- Description: The 32-bit PE imports standard Win32, COM, graphics, networking,
  MSI, and trust APIs. It is an InstallShield setup launcher.
- Evidence source: REA `inspect-pe-imports`, evidence
  `ev_9cdd153b746177df28c7cb0c59b6765b86aa15c0e0e7f5890f97c4f7b8a3d41a`.
- Reproduction procedure: Run REA PE import inspection with a 300 MiB read
  budget and inspect the reported import and delay-import tables.
- Confidence: Confirmed for static imports.
- Technical implications: The launcher can verify signatures, start MSI,
  download prerequisites, and display setup UI. This does not describe FTK.
- Implementation recommendation: Treat the launcher as distribution plumbing.
- Known limitations: Runtime reachability and loaded modules are unobserved.

### ARC-F-002: The package payload is opaque to standard tools

- Finding ID: ARC-F-002
- Description: An embedded signature and two hundred megabytes of payload exist
  after the PE image, but the payload does not parse as a CAB or supported
  InstallShield package.
- Evidence source: EV-001 through EV-004.
- Reproduction procedure: Compute the installer hash; scan for `MSCF`; carve the
  payload; run CAB extraction tools; run byte-entropy inspection.
- Confidence: Confirmed extraction failure; transform unknown.
- Technical implications: The actual FTK module graph, dependencies, and
  resource layout remain unknown.
- Implementation recommendation: Do not infer architecture from unresolved
  payload bytes.
- Known limitations: The exact packaging scheme, keys, and access requirements
  were not investigated.

### ARC-F-003: Managed versus native product code is unresolved

- Finding ID: ARC-F-003
- Description: REA identified possible managed or native follow-up paths from
  the PE root, but the 229 MiB installer exceeded the managed inspection
  snapshot budget, and the product payload was not extractable.
- Evidence source: REA `inspect-artifact` evidence
  `ev_a4a24779cd173bd3f863336a4d12a1d2da6bec60d5d93447a8f9a7aa15012e17`.
- Reproduction procedure: Run `inspect-artifact`, then attempt
  `inspect-managed-artifact` within its documented limits.
- Confidence: Observed for the inspection boundary.
- Technical implications: No claim about FTK's implementation language is
  justified.
- Implementation recommendation: Record the application architecture as
  unknown.
- Known limitations: A smaller authorized extracted assembly or active
  installation would be required for further static analysis.

### ARC-F-004: The documented application is organized around evidence panes and modal workflows

- Finding ID: ARC-F-004
- Description: Official documentation describes a main window with dockable
  Evidence Tree, File List, Properties, Hex Value Interpreter, Custom Content
  Sources, and Viewer, plus toolbar actions and acquisition/verification wizards.
- Evidence source: EV-006.
- Reproduction procedure: Read the official user guide chapters "The FTK
  Imager User Interface" and "Creating Forensic Images".
- Confidence: Observed in official documentation.
- Technical implications: The application is task-oriented around evidence
  hierarchy, inspection, acquisition, export, and verification.
- Implementation recommendation: Model these as separate domain services and
  UI surfaces, not one monolithic window controller.
- Known limitations: Current implementation structure and UI framework are
  unknown.

### ARC-F-005: The installer declares Windows prerequisites

- Finding ID: ARC-F-005
- Description: Installer resources mention MSI, .NET Framework redistributables,
  WinInet, and Windows compatibility checks.
- Evidence source: EV-002 and InstallShield string extraction.
- Reproduction procedure: Extract UTF-16 strings and inspect PE resources.
- Confidence: Observed.
- Technical implications: The distribution targets Windows and carries
  prerequisite logic.
- Implementation recommendation: Do not use these strings to assert FTK's
  current runtime dependencies.
- Known limitations: Prerequisite strings can be generic InstallShield content.

## Architecture conclusions

| Layer | Established state |
|---|---|
| Distribution wrapper | Native Win32 InstallShield launcher |
| Product package | Opaque payload, format and protection unresolved |
| Product module graph | Unknown |
| Product UI toolkit | Unknown |
| Acquisition engine | Unknown |
| Filesystem engine | Unknown |
| Persistence model | Unknown |
| Error model | Unknown |

## Recommended independent architecture

OpenForensic Imager does not need FTK's internal architecture. The recommended
design is:

- Rust core with explicit domain and error types.
- C ABI to a C++/Qt desktop shell.
- QML inspection surfaces backed by Qt item models.
- Isolated worker process for TSK and libewf.
- Pinned native dependencies.
- SQLite case metadata and append-only hash-chained audit log.
- Staged acquisition with a commit manifest.

See [ARCHITECTURE.md](../../ARCHITECTURE.md) and
[docs/architecture/decisions.md](../architecture/decisions.md).

