# Interface Specifications

Status: Phase 0 design baseline for review.

## Versioning

Every C ABI, worker protocol, database schema, and report schema carries a
major and minor version.

- Major changes may break compatibility.
- Minor changes add optional fields or operations.
- Unknown optional fields must be ignored by a tolerant reader only where the
  schema explicitly permits it.
- Unknown required fields, message types, or enum values fail closed.

## C ABI conventions

- Strings are UTF-8 and are passed with an explicit byte length.
- A null pointer is invalid unless the function contract says otherwise.
- Every returned allocation has a matching release function.
- Rust panics are caught and converted to a generic internal error before the
  ABI boundary.
- Errors contain a stable code, operation, summary, retryability, and optional
  structured context.
- Callbacks may run on non-GUI threads.
- Callback payloads are copied for the receiver and remain valid only for the
  duration of the callback unless ownership is explicitly transferred.

## Evidence worker protocol

Evidence-facing jobs run in `ofi-parser-worker`. The process receives one JSON
job request on stdin and emits one JSON response on stdout. The parent closes
stdin after writing the request and bounds both directions:

- Request: 1 MiB.
- Response: 64 MiB.
- Diagnostics: 64 KiB.
- Default wall-clock deadline: 1 hour.
- Deadline override: `OFI_WORKER_TIMEOUT_SECS`, capped at 24 hours.

The parent starts each worker in a new process group. Cancellation and timeout
send `SIGKILL` to the group, then reap the worker. The job manager returns
`cancelled`, `worker.timeout`, `worker.crashed`, `worker.protocol`, or
`worker.sandbox` rather than treating an incomplete response as success.

The worker applies the following before parsing:

- Core dumps disabled.
- Address space capped at 4 GiB.
- CPU time capped at 1 hour.
- Open descriptors capped at 256.
- File size capped at 16 TiB.
- Landlock read grants for the evidence path and timezone data.
- Landlock write grants for selected output and report directories.
- `PR_SET_NO_NEW_PRIVS`.
- seccomp filters synchronized across threads, denying network, process
  creation, execution, and ptrace syscalls.

EWF jobs receive read access to the image directory so sibling segments can be
opened. Other image jobs receive read access to the canonical image path. The
worker and desktop process run under the same Unix user; this protocol provides
fault containment and filesystem/syscall reduction, not a privilege boundary.

`evidence_list`, `audit_append`, and `audit_verify` remain in the trusted
process because they operate on case metadata rather than evidence parsers.

## Request envelope

```json
{
  "abi_major": 1,
  "abi_minor": 0,
  "request_id": "uuid",
  "operation": "case.create",
  "body": {}
}
```

## Response envelope

```json
{
  "abi_major": 1,
  "abi_minor": 0,
  "request_id": "uuid",
  "ok": true,
  "body": {},
  "error": null
}
```

## Error object

```json
{
  "code": "image.segment_missing",
  "operation": "image.verify",
  "summary": "The E01 segment set is incomplete.",
  "retryable": false,
  "partial": true,
  "context": {
    "expected_segment": 4,
    "artifact_id": "uuid"
  }
}
```

Codes use lower-case dot-separated namespaces. User-facing summaries must not
include raw evidence content.

## Hash record

```json
{
  "algorithm": "sha256",
  "scope": "output_logical",
  "digest_hex": "lowercase hex",
  "byte_count": 1073741824,
  "stored_in_image": false,
  "zero_fill_policy": "none",
  "calculated_at_utc": "2026-10-30T12:34:56.123456789Z"
}
```

Allowed scopes:

- `source_read`
- `output_logical`
- `stored_format`
- `extracted_file`

Allowed comparison outcomes:

- `match`
- `mismatch`
- `not_available`
- `error`

## Evidence timestamp

```json
{
  "kind": "modified",
  "raw_value": null,
  "utc_seconds": null,
  "nanos": null,
  "source_semantics": "local_wall_clock_unknown_zone",
  "timezone_assumption": {
    "zone": "Asia/Jakarta",
    "source": "examiner"
  },
  "validity": "requires_assumption",
  "precision": "2s"
}
```

The schema does not use a single unqualified datetime string for evidence
timestamps.

## Image metadata

```json
{
  "image_id": "uuid",
  "format": "ewf1_e01",
  "logical_length": 0,
  "sector_size": 512,
  "segment_count": 1,
  "parser_name": "libewf",
  "parser_version": "pinned value",
  "validation_status": "valid",
  "stored_hashes": [],
  "warnings": []
}
```

## Job snapshot

```json
{
  "job_id": "uuid",
  "kind": "acquisition",
  "state": "running",
  "phase": "hashing_output",
  "bytes_total": 0,
  "bytes_completed": 0,
  "elapsed_seconds": 0,
  "throughput_bytes_per_second": null,
  "warning_count": 0,
  "partial": false,
  "cancellable": true
}
```

Job states:

- `pending`
- `running`
- `succeeded`
- `partial`
- `failed`
- `cancelled`

## Job submission

ABI minor version 1 adds the operations listed below. The C ABI parser first
decodes the complete request without running it. The same parser is used by the
side-effect-free stable property test and fuzz target.

| Operation | Purpose |
|---|---|
| `image_identify` | Identify RAW, EWF1, or a supported image and return metadata. |
| `partition_analysis` | Parse MBR or GPT through TSK. |
| `filesystem_open` | Open a filesystem at a byte offset and return volume metadata. |
| `filesystem_list` | List a directory inode. |
| `source_snapshot` | Capture image-source identity and read-only status. |
| `verification` | Hash an existing image and compare requested values. |
| `acquisition` | Acquire a source to RAW or EWF1 E01. |
| `evidence_list` | List evidence records for a case. |
| `audit_append` | Append an explicit examination event. |
| `audit_verify` | Verify the audit sequence and hash chain. |
| `hex_preview` | Return a bounded file window. |
| `text_preview` | Return a bounded decoded text window. |
| `file_signature` | Identify a file signature. |
| `extract_file` | Export one regular file. |
| `extract_directory` | Export a directory and write extraction reports. |

Image and source operations:

```json
{"operation": "image_identify", "path": "/evidence/disk.E01"}
```

```json
{
  "operation": "filesystem_open",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576
}
```

```json
{"operation": "source_snapshot", "path": "/evidence/disk.E01"}
```

```json
{
  "operation": "evidence_list",
  "db_path": "/cases/2026-001/case.db",
  "case_id": "00000000-0000-0000-0000-000000000001"
}
```

```json
{"operation": "partition_analysis", "path": "/evidence/disk.E01"}
```

```json
{
  "operation": "filesystem_list",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576,
  "inode": 2
}
```

```json
{
  "operation": "verification",
  "path": "/evidence/disk.E01",
  "algorithms": ["sha256", "sha1"],
  "expected": {"sha256": "lowercase hex"},
  "reports_dir": "/cases/2026-001/reports",
  "report_base_name": "verification-01"
}
```

```json
{
  "operation": "acquisition",
  "source_path": "/dev/disk/by-id/example",
  "destination_dir": "/cases/2026-001/acquisition",
  "base_name": "evidence-01",
  "format": "raw",
  "options": {
    "block_size": 1048576,
    "maximum_segment_size": 0,
    "compression": "none",
    "verify_after": true
  },
  "reports_dir": "/cases/2026-001/reports",
  "report_base_name": "acquisition-01"
}
```

When report fields are present, the job returns JSON, CSV, and HTML paths.
Report filenames are derived from `report_base_name`; the value may not contain
path components.

Audit operations:

```json
{
  "operation": "audit_append",
  "audit_path": "/cases/2026-001/audit.jsonl",
  "case_id": "00000000-0000-0000-0000-000000000001",
  "actor": "examiner",
  "action": "evidence.opened",
  "subject_id": "00000000-0000-0000-0000-000000000002",
  "details": {"source": "disk.E01"}
}
```

```json
{
  "operation": "audit_verify",
  "audit_path": "/cases/2026-001/audit.jsonl"
}
```

`audit_verify` returns `valid`, `record_count`, `head_hash`, and `reason`.
`audit_append` runs under the exclusive audit writer lock. Case creation and
evidence insertion accept an optional `audit_path`; when it is omitted the log
is `audit.jsonl` beside the case database, so a missing field never skips the
audit. The log is opened before the SQLite write so an unavailable log prevents
the database commit.

Both operations accept an optional `db_path` (and `case_id` for `audit_verify`)
to bind the request to a case. When supplied, a log that does not belong to that
case is rejected with `audit.case` instead of being appended to or attested.

The audit event is inserted in the same transaction as the case or evidence row
and then replayed into the JSONL chain. If the append cannot happen the state is
committed, the event stays queued, and the call reports
`audit.append_after_commit`; the next audited operation replays it. There is no
atomic transaction across the two files, but no committed row is left
unrecoverable.

Verification reports `verification.not_performed` unless every requested
algorithm was compared against an expected digest and matched. A missing
expected digest is a failure, not a silent success.

A successful submission returns a `job_id`. The caller polls
`ofi_job_snapshot` and may call `ofi_job_cancel` while the job is non-terminal.
Cancellation terminates the worker process group; a terminal state remains
immutable.

Preview and extraction operations:

```json
{
  "operation": "hex_preview",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576,
  "metadata_address": 42,
  "file_offset": 0,
  "length": 4096
}
```

```json
{
  "operation": "text_preview",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576,
  "metadata_address": 42,
  "file_offset": 0,
  "max_bytes": 4096
}
```

```json
{
  "operation": "file_signature",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576,
  "metadata_address": 42
}
```

```json
{
  "operation": "extract_file",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576,
  "parent_inode": 2,
  "metadata_address": 42,
  "destination_root": "/cases/2026-001/exports"
}
```

Preview length is capped at 64 KiB. Extraction requires the parent inode so the
evidence name is resolved from a directory entry, not invented from metadata.

Directory extraction additionally accepts a reports directory and base name:

```json
{
  "operation": "extract_directory",
  "path": "/evidence/disk.E01",
  "offset_bytes": 1048576,
  "inode": 2,
  "directory_name": "files",
  "destination_root": "/cases/2026-001/exports",
  "reports_dir": "/cases/2026-001/reports",
  "base_name": "directory-export"
}
```

The job returns the extraction manifest and all three report paths. A manifest
with any failed entry is reported as a partial job, never as success.

## Viewer page

```json
{
  "source_length": 1073741824,
  "offset": 0,
  "requested_length": 4096,
  "returned_length": 4096,
  "truncated": false,
  "encoding": null,
  "signature": "png",
  "renderer": "image",
  "isolated": true,
  "image_pixels": 64000
}
```

Hex and text requests are limited to 64 KiB. An image render plan is rejected
when the detected dimensions exceed 40 million pixels or the source signature
does not match the supplied plan. PDF rendering is limited to 64 MiB until an
isolated renderer proves stricter streaming behavior.

## Extraction manifest

```json
{
  "schema_version": 1,
  "generated_at": "2026-10-30T12:34:56Z",
  "output_root": "/examiner/exports",
  "directory": "files",
  "status": "complete",
  "records": [
    {
      "original_name": "report.txt",
      "relative_path": "files/report.txt",
      "exported_size": 1234,
      "source_sha256": "lowercase hex",
      "output_sha256": "lowercase hex",
      "extracted_at": "2026-10-30T12:34:57Z"
    }
  ],
  "failures": []
}
```

`partial` is mandatory when any requested entry fails. Every successful record
requires matching source and reopened-output SHA-256 values.

## Worker frame

```text
4-byte big-endian JSON length
JSON control message
optional binary payload
```

The JSON message declares the payload length and purpose. The service rejects:

- Messages over the configured control limit.
- Payloads over the configured binary-page limit.
- Payload lengths that differ from the frame.
- Unknown frame types.
- Unexpected sequence numbers.

## Audit record

The authoritative JSONL record contains:

```json
{
  "schema_major": 1,
  "schema_minor": 0,
  "sequence": 1,
  "recorded_at_utc": "2026-10-30T12:34:56.123456789Z",
  "case_id": "uuid",
  "actor": "examiner",
  "action": "evidence.added",
  "subject_id": "uuid",
  "details": {},
  "previous_hash": "lowercase hex or empty",
  "record_hash": "lowercase hex"
}
```

`record_hash` is computed over a canonical encoding with `record_hash` omitted.
Canonical rules must be documented in code and fixed by test vectors.

## Compatibility policy

- Adding a field is backward-compatible for readers that tolerate unknown
  optional fields.
- Removing a field, changing a meaning, changing an enum, or changing ownership
  requires a major version.
- Database migrations are forward-only.
- Report schemas are versioned separately from ABI schemas.

