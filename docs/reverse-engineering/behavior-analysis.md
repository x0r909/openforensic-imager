# FTK Imager Behavior Analysis

## Scope and confidence

No FTK Imager process was executed. Statements below are documented behavior
unless explicitly marked unknown or inferred. The supplied installer cannot be
used as runtime evidence.

Behavior terms:

- Documented: described by Exterro.
- Observed: directly seen in an artifact or host command.
- Runtime confirmed: recorded from execution.
- Inferred: derived from evidence.
- Unknown: not established.

## Behavior findings

### BEH-F-001: Acquisition wizard

- Finding ID: BEH-F-001
- Description: The documented workflow selects a source, selects an image type,
  enters evidence information, selects destination, fragment size, compression,
  and optional encryption, then starts the image.
- Evidence source: EV-006, "Creating Forensic Images".
- Reproduction procedure: Read the wizard steps for complete drive imaging and
  compare the visible fields described in the guide.
- Confidence: Documented, not runtime confirmed.
- Technical implications: Acquisition is a multi-stage operation with source,
  format, provenance, destination, and policy decisions.
- Implementation recommendation: Implement an explicit acquisition state machine
  with typed parameters. Do not hide format restrictions inside wizard code.
- Known limitations: Default values, validation messages, and current 8.3
  behavior were not observed.

### BEH-F-002: Image verification

- Finding ID: BEH-F-002
- Description: The documented verification workflow recomputes MD5 and SHA-1 for
  a drive or image, shows sector count and bad-sector count, and compares
  computed hashes with hashes stored inside E01/S01 images.
- Evidence source: EV-006, "Verifying Drives and Images".
- Reproduction procedure: Inspect the verification workflow and result fields
  in the official guide.
- Confidence: Documented, not runtime confirmed.
- Technical implications: A verification result must preserve computed, stored,
  and comparison states separately.
- Implementation recommendation: Use a typed result per algorithm and an
  explicit `NotAvailable` state. Add independent SHA-256 and SHA-512.
- Known limitations: Exact handling of missing segments, corrupt chunks, and I/O
  errors is unknown.

### BEH-F-003: Opening images and mounting

- Finding ID: BEH-F-003
- Description: The guide documents adding RAW/dd, E01, S01, AFF, AD1, and L01
  images as evidence and mounting supported images read-only or physically.
  Mounting can expose an image to host applications.
- Evidence source: EV-006, "Working with Evidence" and "Image Mounting".
- Reproduction procedure: Read the image mounting and supported-type sections.
- Confidence: Documented, not runtime confirmed.
- Technical implications: Mounting expands the attack surface and can alter a
  cache or host state if implemented carelessly.
- Implementation recommendation: Do not mount in release 0.1. Provide internal
  parsing and extraction. Never mount read-write.
- Known limitations: FTK's current mount implementation and cache behavior are
  unknown.

### BEH-F-004: Preview modes

- Finding ID: BEH-F-004
- Description: The guide documents automatic, text, and hex preview modes.
  Unsupported file types may be opened outside the application, and cached HTML
  can reference active Internet content.
- Evidence source: EV-006, "Preview Modes".
- Reproduction procedure: Inspect the preview chapter and its security warning.
- Confidence: Documented, not runtime confirmed.
- Technical implications: External open and active content are unsafe for
  untrusted evidence.
- Implementation recommendation: Keep all previews inside the application,
  render evidence as plain text, disable network access, and never auto-launch an
  external program.
- Known limitations: The exact 8.3 preview implementation and mitigation state
  were not observed.

### BEH-F-005: File and directory export

- Finding ID: BEH-F-005
- Description: The guide documents exporting selected files or folders and
  creating hash reports and directory listings.
- Evidence source: EV-006, "Exporting from FTK Imager".
- Reproduction procedure: Inspect the export and file-hash-list sections.
- Confidence: Documented, not runtime confirmed.
- Technical implications: Export is a read path followed by a write to a new
  destination, with hash and metadata reporting.
- Implementation recommendation: Use capability-based extraction, conflict
  policy, per-file hashes, and source-metadata recording.
- Known limitations: FTK's conflict handling and filename sanitation were not
  observed.

### BEH-F-006: Hash list and directory listing

- Finding ID: BEH-F-006
- Description: The guide documents MD5/SHA-1 file hash lists and a directory
  listing with path, creation date, deletion status, and other metadata in TSV.
- Evidence source: EV-006, "Exporting File Hash Lists" and "Exporting Directory
  Listings".
- Reproduction procedure: Inspect the output descriptions in the guide.
- Confidence: Documented, not runtime confirmed.
- Technical implications: Inventory output itself needs UTC and timestamp
  interpretation metadata.
- Implementation recommendation: Generate JSON and CSV with schema versions,
  UTC system times, and evidence timestamp provenance.
- Known limitations: Exact column order and date formatting were not observed.

### BEH-F-007: Corrupt image behavior

- Finding ID: BEH-F-007
- Description: The specific behavior for corrupt E01 headers, bad chunk tables,
  truncated segments, and missing segments is not established by available
  evidence.
- Evidence source: None sufficient.
- Reproduction procedure: Would require an authorized FTK runtime and a
  disposable corrupted fixture set.
- Confidence: Unknown.
- Technical implications: We cannot claim behavioral equivalence.
- Implementation recommendation: Define our own explicit corruption, partial,
  and verification-error states and test them heavily.
- Known limitations: No authorized Windows runtime was available.

### BEH-F-008: Inaccessible sector behavior

- Finding ID: BEH-F-008
- Description: FTK verification results include a bad-sector count, and the
  guide describes imaging sources through write blockers. Exact acquisition
  behavior on an unreadable sector is not established.
- Evidence source: EV-006, imaging and verification chapters.
- Reproduction procedure: Use a disposable block-device fault-injection setup
  in an authorized FTK test environment.
- Confidence: Bad-sector count fields are documented; policy is unknown.
- Technical implications: A bad-sector count alone is insufficient to reproduce
  the image or interpret the output hash scope.
- Implementation recommendation: Record exact unreadable ranges, retry policy,
  and zero-fill policy. Set the acquisition state to partial.
- Known limitations: Hardware-level error injection was not available.

### BEH-F-009: Error notification behavior

- Finding ID: BEH-F-009
- Description: The screenshots and prose indicate progress and result dialogs,
  but the complete error taxonomy and modal behavior are not documented in the
  8.2 guide.
- Evidence source: EV-006, progress and summary figures.
- Reproduction procedure: Would require runtime interaction with injected
  failures.
- Confidence: Unknown beyond the existence of result and progress dialogs.
- Technical implications: The UI cannot copy error precedence or exact
  messages.
- Implementation recommendation: Build a typed error model and a clear
  background-task state machine.
- Known limitations: No runtime capture.

### BEH-F-010: Case and chain-of-custody behavior

- Finding ID: BEH-F-010
- Description: FTK Imager can collect evidence-item information and generate an
  image summary. The guide does not establish a complete append-only
  chain-of-custody system for the free Imager workflow.
- Evidence source: EV-006, "Evidence Item Information" and "Image Summary".
- Reproduction procedure: Inspect the evidence information fields and summary
  documentation.
- Confidence: Evidence fields are documented; complete chain-of-custody
  capability is unknown.
- Technical implications: OpenForensic needs an explicit audit design beyond
  acquisition metadata.
- Implementation recommendation: Implement a separate append-only audit log and
  evidence inventory.
- Known limitations: FTK 8.3 and FTK suite behavior may differ.

## Runtime investigation checklist

If an authorized Windows test environment becomes available, capture:

1. Open RAW, E01, segmented E01, truncated E01, and missing-segment E01.
2. Record every visible error and operation state.
3. Acquire a disposable disk through a write blocker and inject read faults.
4. Verify with matching, mismatching, and absent hashes.
5. Export files with traversal-like names, Unicode names, and conflicts.
6. Capture process modules, loaded libraries, child processes, and file access
   with Process Monitor.
7. Compare resulting reports and hashes against OpenForensic output.

Nothing from this checklist is claimed as complete.

