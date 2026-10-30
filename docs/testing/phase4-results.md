# Phase 4 Verification Results

Date: 2026-10-30

Branch: local `main`

## Implemented scope

- Bounded hexadecimal pages with offset, hexadecimal, and ASCII rows.
- Bounded text preview with ASCII, UTF-8, UTF-16LE, and UTF-16BE detection.
- Binary-content rejection instead of lossy text rendering.
- File-signature identification for PNG, JPEG, GIF, PDF, ZIP, ELF, PE, and EWF1.
- Image preflight for PNG, GIF, and JPEG dimensions before a renderer is called.
- PDF preview size preflight.
- Preview validation that rejects a supplied signature that does not match the
  source.
- Streaming evidence-file reads that do not load the complete file.
- Capability-based extraction with `openat2`, `RESOLVE_BENEATH`,
  `RESOLVE_NO_SYMLINKS`, and `RESOLVE_NO_MAGICLINKS`.
- Owner-only extraction roots and regular files with mode `0600`.
- Traversal rejection, control-character sanitization, Unicode-name handling,
  symlink rejection, conflict handling, and bounded file/total extraction.
- Source and exported-file SHA-256 comparison before an extraction is reported
  successful.
- Extraction metadata plus JSON, CSV, and escaped HTML reports.

## Verification evidence

The complete workspace was formatted, linted, compiled, and executed:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-run --message-format=json
```

Every emitted test executable was run with `--test-threads=1`.

Result: 36 test executables, 101 tests passed, 0 failed.

## Security coverage

| Control | Executable evidence |
|---|---|
| Path traversal | `.`, `..`, and absolute path names are rejected before writing |
| Destination symlink | An existing symlink is never followed or overwritten |
| Capability root replacement | Writes remain under the opened root after its path is replaced |
| Evidence symlink | Filesystem symlink objects are rejected instead of recreated |
| Conflict safety | The default fails on conflict; rename mode selects a new exclusive name |
| Resource limits | Per-file and cumulative byte limits leave no failed output file |
| Name handling | Unicode is preserved; control, percent, and backslash bytes are escaped |
| Hash integrity | A successful record requires matching source and reopened-output SHA-256 |
| Report injection | HTML is escaped and CSV formula prefixes are neutralized |
| Image bomb preflight | Oversized dimensions and truncated headers are rejected |
| Malformed source | Invalid byte counts and signature mismatches fail closed |

## Integration coverage

The Phase 4 integration test embeds text, PNG, PDF, and binary files in an ext4
image. It opens the filesystem through TSK, previews and identifies the files,
exports the directory through the capability root, writes the report set, checks
the exported bytes and hashes, and confirms the source image hash is unchanged.

## Limitations

- Image and PDF rendering is not implemented in this phase. The core produces a
  validated render plan; the sandboxed Qt renderer remains Phase 5 work.
- The host lacks the Qt PDF module. PDF support remains optional and unverified
  until a sandboxed renderer is available.
- Decoders must enforce the same pixel budget as `plan_preview`. The preflight
  does not replace decoder resource controls.
- Extraction uses Linux `openat2`, so the extraction implementation requires
  Linux 5.6 or newer.
- Evidence names whose sanitized form exceeds 255 bytes are rejected. A
  collision-resistant long-name mapping remains future work.
- Deleted regular files can be requested explicitly, but this is an export of
  parser-exposed data, not deleted-content carving or recovery.
- Reports support JSON, CSV, and HTML. PDF report generation remains deferred.
- Extraction records are not yet committed to the append-only case audit log.
- Filesystem directory enumeration still materializes a directory before
  extraction. Bounded directory paging remains required for very large trees.
- The capability-root replacement test is deterministic. High-frequency
  symlink-race stress and fuzzing remain Phase 6 work.
