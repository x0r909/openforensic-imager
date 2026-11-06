use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::process::CommandExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use acquisition::{
    AcquisitionErrorCode, AcquisitionRequest, AcquisitionStatus, OutputFormat,
    acquire_path_with_cancel, verify_image,
};
use extraction::{ExtractionOptions, Extractor};
use filesystem::{EvidenceFile, FileSystem};
use forensic_core::{CancellationToken, JobId};
use image_reader::open_image;
use job_manager::{JobContext, JobError, JobOutcome};
use partition_analyzer::analyze_partitions;
use reporting::{
    write_acquisition_report_set, write_extraction_report_set, write_verification_report_set,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use viewer::{ByteSource, SourceError, hex_page, identify_signature, preview_text};
use worker_sandbox::{
    ResourceLimits, SandboxPolicy, apply_landlock, apply_privilege_drop,
    apply_resource_limits, apply_seccomp_restrictions,
};

use super::{JobRequest, job_success, job_value};

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const DEFAULT_TIMEOUT_SECONDS: u64 = 60 * 60;
const MAX_TIMEOUT_SECONDS: u64 = 24 * 60 * 60;

/// The only environment variables an evidence parser may inherit.
///
/// SECURITY_MODEL.md requires no inherited environment beyond a minimal
/// allowlist, so the worker is spawned with an empty environment except for
/// these. None of them carries a credential, token, or key.
const WORKER_ENVIRONMENT_ALLOWLIST: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "TZDIR",
    "LIBEWF_VERBOSE",
];

/// Builds the worker environment from the allowlist, dropping unset entries.
fn worker_environment() -> Vec<(OsString, OsString)> {
    WORKER_ENVIRONMENT_ALLOWLIST
        .iter()
        .filter_map(|name| {
            std::env::var_os(name).map(|value| (OsString::from(*name), value))
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerResponse {
    ok: bool,
    outcome: Option<WorkerOutcome>,
    error: Option<WorkerError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerOutcome {
    partial: bool,
    result: Value,
    message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerError {
    code: String,
    message: String,
    /// Payload that survives a cancellation, such as the retained staging path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
}

struct EvidenceByteSource<'a> {
    file: EvidenceFile<'a>,
}

impl ByteSource for EvidenceByteSource<'_> {
    fn len(&self) -> u64 {
        self.file.size()
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, SourceError> {
        self.file
            .read_at(offset, buffer)
            .map_err(|error| SourceError::new(error.to_string()))
    }
}

pub fn run_stdio_worker() -> Result<(), String> {
    let mut request = Vec::new();
    io::stdin()
        .lock()
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut request)
        .map_err(|error| format!("unable to read worker request: {error}"))?;

    let response = if request.is_empty() || request.len() > MAX_REQUEST_BYTES {
        WorkerResponse::error("worker.request", "worker request is empty or too large")
    } else {
        process_request(&request)
    };
    let bytes = serde_json::to_vec(&response)
        .map_err(|error| format!("unable to serialize worker response: {error}"))?;
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&bytes)
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("unable to write worker response: {error}"))
}

fn process_request(bytes: &[u8]) -> WorkerResponse {
    let mut request: JobRequest = match serde_json::from_slice(bytes) {
        Ok(request) => request,
        Err(error) => return WorkerResponse::error("worker.request", error.to_string()),
    };
    if !request.is_isolated() {
        return WorkerResponse::error(
            "worker.unsupported_operation",
            "operation is not allowed in the evidence worker",
        );
    }
    if let Err(error) = canonicalize_request_paths(&mut request) {
        return WorkerResponse::error(error.code(), error.message().to_owned());
    }
    if let Err(error) = prepare_output_paths(&request) {
        return WorkerResponse::error(error.code(), error.message().to_owned());
    }
    let policy = match sandbox_policy(&request) {
        Ok(policy) => policy,
        Err(error) => return WorkerResponse::error(error.code(), error.message().to_owned()),
    };
    if let Err(error) = apply_resource_limits(&file_size_limits(&request))
        .and_then(|()| apply_landlock(&policy))
        .and_then(|()| apply_privilege_drop(ofi_worker_uid(), ofi_worker_gid()))
        .and_then(|()| apply_seccomp_restrictions())
    {
        return WorkerResponse::error("worker.sandbox", error.message().to_owned());
    }

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        execute_isolated_request(request, &CancellationToken::default())
    }));
    match outcome {
        Ok(Ok(outcome)) => WorkerResponse::outcome(outcome),
        Ok(Err(error)) => WorkerResponse::from_error(error),
        Err(_) => WorkerResponse::error("worker.panic", "parser worker panicked"),
    }
}

/// Sizes the worker's `RLIMIT_FSIZE` from what the job says it will write.
///
/// The limit caps one file, and the files the worker creates are evidence. A
/// raw acquisition writes its source verbatim, so the image is exactly the
/// source length; an EWF acquisition rolls segments at
/// `maximum_segment_size`, and libewf keeps every segment under it. Both are
/// known before parsing starts — the reader that reports them is the same one
/// the acquisition opens — so the budget can be derived here instead of guessed
/// inside `ResourceLimits::parser_default`, which has no request to read.
///
/// It has to be set here rather than raised inside the limit itself: a process
/// may lower its own soft limit freely but may only raise the hard limit back
/// up to what it inherited, and the hard limit is what this sets. The worker
/// serves one request per process, so this process's budget is this job's.
///
/// The derived value is a *floor*, not the whole budget: a job may write more
/// than the evidence it names. The acquisition writes its manifest beside the
/// image, and the manifest is larger than a tiny image — a budget equal to a
/// 512-byte source fails the job on the manifest that describes it. So the
/// budget is the larger of the derived value and the default, and the derived
/// value only ever raises it. Extraction is bounded separately, before it opens
/// a destination: `ExtractionOptions` rejects an entry past `max_file_bytes`
/// rather than writing into the limit.
fn file_size_limits(request: &JobRequest) -> ResourceLimits {
    let mut limits = ResourceLimits::parser_default();
    let declared = match request {
        JobRequest::Acquisition {
            source_path,
            format,
            options,
            ..
        } => match format {
            OutputFormat::Raw => {
                // A source the reader cannot size keeps the default floor: the
                // acquisition reports the failure itself, and a smaller limit
                // cannot turn it into a different one.
                acquisition_source_length(source_path).unwrap_or(limits.file_size_bytes)
            }
            // An EWF acquisition rolls a new segment at this size, so it bounds
            // each file the worker writes — but not the report files beside
            // them, which is why this is a floor for the whole budget too.
            OutputFormat::Ewf1E01 => options.maximum_segment_size,
        },
        JobRequest::ExtractFile { .. } | JobRequest::ExtractDirectory { .. } => {
            ExtractionOptions::default().max_file_bytes
        }
        _ => limits.file_size_bytes,
    };
    // A declared bound of zero — an empty source, a truncated EWF header — is a
    // request the job will reject on its own terms. Handing a zero to
    // `apply_resource_limits` would make it a sandbox failure instead, which
    // names the sandbox for a defect in the request.
    limits.file_size_bytes = declared.max(limits.file_size_bytes).max(1);
    limits
}

/// The logical length the acquisition will read from `source_path`.
///
/// `open_image` picks the reader by signature, exactly as the acquisition does,
/// so an EWF segment set and a raw file both report the length of the evidence
/// rather than of the first segment on disk.
fn acquisition_source_length(source_path: &str) -> Option<u64> {
    let reader = open_image(Path::new(source_path)).ok()?;
    Some(reader.logical_length())
}

fn prepare_output_paths(request: &JobRequest) -> Result<(), JobError> {
    let paths: Vec<&str> = match request {
        JobRequest::Acquisition { reports_dir, .. } => reports_dir.as_deref().into_iter().collect(),
        JobRequest::Verification { reports_dir, .. } => {
            reports_dir.as_deref().into_iter().collect()
        }
        JobRequest::ExtractDirectory { reports_dir, .. } => vec![reports_dir],
        _ => Vec::new(),
    };
    for path in paths {
        prepare_directory(Path::new(path))?;
    }
    Ok(())
}

/// Resolve every evidence path in the request to an absolute, canonical form
/// before sandboxing. The Landlock policy grants access to these exact paths,
/// and `open_image` canonicalizes before touching libewf; if a relative path
/// survived to execution, the canonicalize would run *after* Landlock is
/// applied, where walking the un-granted ancestor directories fails EPERM.
fn canonicalize_request_paths(request: &mut JobRequest) -> Result<(), JobError> {
    // Resolve the evidence path without borrowing `request` mutably, then
    // assign the canonical form back. The first match works on an immutable
    // borrow; the second matches again to find the field to overwrite.
    let canonical = match &*request {
        JobRequest::ImageIdentify { path } => std::fs::canonicalize(path),
        JobRequest::PartitionAnalysis { path } => std::fs::canonicalize(path),
        JobRequest::FilesystemList { path, .. } => std::fs::canonicalize(path),
        JobRequest::FilesystemOpen { path, .. } => std::fs::canonicalize(path),
        JobRequest::Verification { path, .. } => std::fs::canonicalize(path),
        JobRequest::Acquisition { source_path, .. } => std::fs::canonicalize(source_path),
        JobRequest::SourceSnapshot { path } => std::fs::canonicalize(path),
        JobRequest::HexPreview { path, .. } => std::fs::canonicalize(path),
        JobRequest::TextPreview { path, .. } => std::fs::canonicalize(path),
        JobRequest::FileSignature { path, .. } => std::fs::canonicalize(path),
        JobRequest::ExtractFile { path, .. } => std::fs::canonicalize(path),
        JobRequest::ExtractDirectory { path, .. } => std::fs::canonicalize(path),
        _ => return Ok(()),
    };
    let canonical = match canonical {
        Ok(path) => path.to_string_lossy().into_owned(),
        Err(error) => {
            return Err(JobError::failed(
                "worker.sandbox",
                format!("unable to resolve evidence path: {error}"),
            ))
        }
    };
    match request {
        JobRequest::ImageIdentify { path } => *path = canonical,
        JobRequest::PartitionAnalysis { path } => *path = canonical,
        JobRequest::FilesystemList { path, .. } => *path = canonical,
        JobRequest::FilesystemOpen { path, .. } => *path = canonical,
        JobRequest::Verification { path, .. } => *path = canonical,
        JobRequest::Acquisition { source_path, .. } => *source_path = canonical,
        JobRequest::SourceSnapshot { path } => *path = canonical,
        JobRequest::HexPreview { path, .. } => *path = canonical,
        JobRequest::TextPreview { path, .. } => *path = canonical,
        JobRequest::FileSignature { path, .. } => *path = canonical,
        JobRequest::ExtractFile { path, .. } => *path = canonical,
        JobRequest::ExtractDirectory { path, .. } => *path = canonical,
        _ => unreachable!(),
    }
    Ok(())
}

fn prepare_directory(path: &Path) -> Result<(), JobError> {
    if !path.is_dir() {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|error| {
                JobError::failed(
                    "worker.sandbox",
                    format!(
                        "unable to create output directory {}: {error}",
                        path.display()
                    ),
                )
            })?;
    }
    // An existing directory is only usable when this user owns it and no other
    // principal can write to it. Otherwise a lower-trust principal could
    // pre-create the directory, keep control of its entries, and substitute
    // report leaves. Group read/execute is fine; group or other write is not.
    // This mirrors the rule Extractor::create enforces for extraction roots.
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        JobError::failed(
            "worker.sandbox",
            format!("unable to inspect {}: {error}", path.display()),
        )
    })?;
    if !metadata.is_dir() {
        return Err(JobError::failed(
            "worker.sandbox",
            format!("output path {} is not a directory", path.display()),
        ));
    }
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err(JobError::failed(
            "worker.sandbox",
            format!(
                "output directory {} must be owned by the current user and not writable by group or others",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn execute_isolated_request(
    request: JobRequest,
    cancellation: &CancellationToken,
) -> Result<JobOutcome, JobError> {
    match request {
        JobRequest::ImageIdentify { path } => {
            let image = open_image(Path::new(&path))
                .map_err(|error| JobError::failed("image.open", error.to_string()))?;
            job_success(image.metadata())
        }
        JobRequest::PartitionAnalysis { path } => {
            let analysis = analyze_partitions(Path::new(&path))
                .map_err(|error| JobError::failed("partition.failed", error.to_string()))?;
            job_success(analysis)
        }
        JobRequest::FilesystemList {
            path,
            offset_bytes,
            inode,
        } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let entries = filesystem
                .list_directory(inode)
                .map_err(|error| JobError::failed("filesystem.list", error.to_string()))?;
            job_success(json!({
                "kind": filesystem.kind(),
                "block_size": filesystem.block_size(),
                "inode_count": filesystem.inode_count(),
                "entries": entries,
            }))
        }
        JobRequest::FilesystemOpen { path, offset_bytes } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            job_success(json!({
                "kind": filesystem.kind(),
                "block_size": filesystem.block_size(),
                "inode_count": filesystem.inode_count(),
                "root_inode": filesystem.root_inode(),
            }))
        }
        JobRequest::Verification {
            path,
            algorithms,
            expected,
            reports_dir,
            report_base_name,
        } => {
            let report = verify_image(Path::new(&path), &algorithms, &expected).map_err(|error| {
                let code = match error.code() {
                    AcquisitionErrorCode::VerificationNotPerformed => "verification.not_performed",
                    _ => "verification.failed",
                };
                JobError::failed(code, error.to_string())
            })?;
            let report_paths = match reports_dir {
                Some(directory) => Some(
                    write_verification_report_set(
                        Path::new(&directory),
                        report_base_name.as_deref().unwrap_or("verification"),
                        &report,
                    )
                    .map_err(|error| {
                        JobError::failed("verification.report", error.message().to_owned())
                    })?,
                ),
                None => None,
            };
            job_success(json!({
                "report": report,
                "reports": report_paths,
            }))
        }
        JobRequest::Acquisition {
            source_path,
            destination_dir,
            base_name,
            format,
            options,
            reports_dir,
            report_base_name,
        } => {
            let report_name = report_base_name
                .clone()
                .unwrap_or_else(|| base_name.clone());
            let result = acquire_path_with_cancel(
                AcquisitionRequest {
                    source_path: source_path.into(),
                    destination_dir: destination_dir.into(),
                    base_name,
                    format,
                    options,
                },
                cancellation,
            )
            .map_err(|error| JobError::failed("acquisition.failed", error.to_string()))?;
            match result.status {
                AcquisitionStatus::Succeeded => {
                    let report_paths = match reports_dir {
                        Some(directory) => Some(
                            write_acquisition_report_set(
                                Path::new(&directory),
                                &report_name,
                                &result.manifest,
                            )
                            .map_err(|error| {
                                JobError::failed("acquisition.report", error.message().to_owned())
                            })?,
                        ),
                        None => None,
                    };
                    job_success(json!({
                        "status": result.status,
                        "manifest": result.manifest,
                        "manifest_path": result.manifest_path,
                        "reports": report_paths,
                    }))
                }
                AcquisitionStatus::Partial => {
                    let value = job_value(json!({
                        "status": result.status,
                        "manifest": result.manifest,
                        "manifest_path": result.manifest_path,
                    }))?;
                    Ok(JobOutcome::partial(
                        value,
                        "acquisition completed with read errors or unverified output",
                    ))
                }
                AcquisitionStatus::Failed => Err(JobError::failed(
                    "acquisition.failed",
                    "acquisition did not produce a verified image",
                )),
                AcquisitionStatus::Cancelled => {
                    // ACQ-011: retain the partial artifacts and tell the caller
                    // where they are, so a cancelled acquisition is recoverable
                    // rather than silently discarded.
                    let value = job_value(json!({
                        "status": result.status,
                        "manifest": result.manifest,
                        "manifest_path": result.manifest_path,
                        "retained": true,
                    }))?;
                    Err(JobError::cancelled_with_result(
                        value,
                        "acquisition cancelled; partial output retained in staging",
                    ))
                }
            }
        }
        JobRequest::SourceSnapshot { path } => {
            let image = open_image(Path::new(&path))
                .map_err(|error| JobError::failed("source.open", error.to_string()))?;
            let metadata = image.metadata();
            job_success(json!({
                "format": metadata.format,
                "logical_length": metadata.logical_length,
                "sector_size": metadata.sector_size,
                "source": metadata.source,
            }))
        }
        JobRequest::HexPreview {
            path,
            offset_bytes,
            metadata_address,
            file_offset,
            length,
        } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let file = filesystem
                .open_file(metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let source = EvidenceByteSource { file };
            let page = hex_page(&source, file_offset, length)
                .map_err(|error| JobError::failed("preview.hex", error.to_string()))?;
            job_success(page)
        }
        JobRequest::TextPreview {
            path,
            offset_bytes,
            metadata_address,
            file_offset,
            max_bytes,
        } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let file = filesystem
                .open_file(metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let source = EvidenceByteSource { file };
            let preview = preview_text(&source, file_offset, max_bytes)
                .map_err(|error| JobError::failed("preview.text", error.to_string()))?;
            job_success(preview)
        }
        JobRequest::FileSignature {
            path,
            offset_bytes,
            metadata_address,
        } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let file = filesystem
                .open_file(metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let source = EvidenceByteSource { file };
            let signature = identify_signature(&source)
                .map_err(|error| JobError::failed("preview.signature", error.to_string()))?;
            job_success(signature)
        }
        JobRequest::ExtractFile {
            path,
            offset_bytes,
            parent_inode,
            metadata_address,
            destination_root,
        } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let entry = filesystem
                .directory_entry(parent_inode, metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let mut extractor =
                Extractor::create(Path::new(&destination_root), ExtractionOptions::default())
                    .map_err(|error| JobError::failed("extraction.root", error.to_string()))?;
            let record = extractor
                .extract_file(&filesystem, &entry)
                .map_err(|error| JobError::failed("extraction.file", error.to_string()))?;
            job_success(record)
        }
        JobRequest::ExtractDirectory {
            path,
            offset_bytes,
            inode,
            directory_name,
            destination_root,
            reports_dir,
            base_name,
        } => {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let mut extractor =
                Extractor::create(Path::new(&destination_root), ExtractionOptions::default())
                    .map_err(|error| JobError::failed("extraction.root", error.to_string()))?;
            let manifest = extractor
                .extract_directory(&filesystem, inode, &directory_name)
                .map_err(|error| JobError::failed("extraction.directory", error.to_string()))?;
            let report_paths =
                write_extraction_report_set(Path::new(&reports_dir), &base_name, &manifest)
                    .map_err(|error| {
                        JobError::failed("extraction.report", error.message().to_owned())
                    })?;
            let status = manifest.status;
            let result = job_value(json!({
                "manifest": manifest,
                "reports": {
                    "json": report_paths.json,
                    "csv": report_paths.csv,
                    "html": report_paths.html,
                },
            }))?;
            if status == extraction::ManifestStatus::Partial {
                Ok(JobOutcome::partial(
                    result,
                    "directory extraction completed with failed entries",
                ))
            } else {
                Ok(JobOutcome::succeeded(result))
            }
        }
        JobRequest::EvidenceList { .. }
        | JobRequest::AuditAppend { .. }
        | JobRequest::AuditVerify { .. } => Err(JobError::failed(
            "worker.unsupported_operation",
            "trusted operation was sent to the evidence worker",
        )),
    }
}

/// Names the staging directory a cancelled acquisition would have written to.
///
/// The worker is killed before it can report, but `acquire_reader` derives the
/// directory name deterministically from the request and the job id, so the
/// retained partial output is still discoverable by the examiner.
fn retained_staging_hint(request: &[u8], job_id: JobId) -> Option<Value> {
    let parsed: JobRequest = serde_json::from_slice(request).ok()?;
    let JobRequest::Acquisition {
        destination_dir,
        base_name,
        ..
    } = parsed
    else {
        return None;
    };
    let staging = Path::new(&destination_dir)
        .join(format!(".{base_name}.partial-{job_id}"));
    // Only report a directory that actually exists, so a cancel before any
    // write does not point the examiner at nothing.
    let exists = staging.is_dir();
    Some(json!({
        "status": "cancelled",
        "retained": exists,
        "staging_path": staging,
    }))
}

pub(super) fn run_job(request: &[u8], context: &JobContext) -> Result<JobOutcome, JobError> {
    let worker = resolve_worker_path()?;
    let mut command = Command::new(&worker);
    command
        .env_clear()
        .envs(worker_environment())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().map_err(|error| {
        JobError::failed(
            "worker.spawn",
            format!("unable to start {}: {error}", worker.display()),
        )
    })?;

    let mut stdin = child.stdin.take().expect("piped worker stdin");
    // Keep a copy for the cancellation path, which reports retained output
    // after the request bytes have been moved into the writer thread.
    let request_for_hint = request.to_vec();
    let request = request.to_vec();
    let writer = thread::spawn(move || {
        stdin.write_all(&request)?;
        stdin.flush()
    });
    let stdout = child.stdout.take().expect("piped worker stdout");
    let stderr = child.stderr.take().expect("piped worker stderr");
    // Shared so the poll loop can stop a worker that exceeds the response cap
    // without waiting for the pipe to close or for the wall-clock deadline.
    let stdout_overflowed = Arc::new(AtomicBool::new(false));
    let overflow_flag = Arc::clone(&stdout_overflowed);
    let stdout_reader = thread::spawn(move || {
        let reader = read_bounded(stdout, MAX_RESPONSE_BYTES);
        if matches!(&reader, Ok((_, true))) {
            overflow_flag.store(true, Ordering::SeqCst);
        }
        reader
    });
    let stderr_reader = thread::spawn(move || read_bounded(stderr, MAX_STDERR_BYTES));

    let started = Instant::now();
    let timeout = worker_timeout();
    let mut cancelled = false;
    let mut timed_out = false;
    let mut overflowed = false;
    let status = loop {
        if let Some(exit_status) = child.try_wait().map_err(|error| {
            JobError::failed(
                "worker.wait",
                format!("unable to poll parser worker: {error}"),
            )
        })? {
            break exit_status;
        }
        if context.is_cancelled() {
            cancelled = true;
            kill_worker_group(&mut child).map_err(|error| {
                JobError::failed(
                    "worker.cancel",
                    format!("unable to stop parser worker: {error}"),
                )
            })?;
            break child.wait().map_err(|error| {
                JobError::failed(
                    "worker.wait",
                    format!("unable to reap parser worker: {error}"),
                )
            })?;
        }
        if stdout_overflowed.load(Ordering::SeqCst) {
            overflowed = true;
            kill_worker_group(&mut child).map_err(|error| {
                JobError::failed(
                    "worker.response_too_large",
                    format!("unable to stop parser worker: {error}"),
                )
            })?;
            break child.wait().map_err(|error| {
                JobError::failed(
                    "worker.wait",
                    format!("unable to reap parser worker: {error}"),
                )
            })?;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            kill_worker_group(&mut child).map_err(|error| {
                JobError::failed(
                    "worker.timeout",
                    format!("unable to stop timed-out parser worker: {error}"),
                )
            })?;
            break child.wait().map_err(|error| {
                JobError::failed(
                    "worker.wait",
                    format!("unable to reap parser worker: {error}"),
                )
            })?;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let writer_result = writer
        .join()
        .map_err(|_| JobError::failed("worker.ipc", "worker request writer panicked"))?;
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| JobError::failed("worker.ipc", "worker response reader panicked"))?
        .map_err(|error| {
            JobError::failed(
                "worker.ipc",
                format!("unable to read worker response: {error}"),
            )
        })?;
    let (stderr, _) = stderr_reader
        .join()
        .map_err(|_| JobError::failed("worker.ipc", "worker error reader panicked"))?
        .map_err(|error| {
            JobError::failed(
                "worker.ipc",
                format!("unable to read worker diagnostics: {error}"),
            )
        })?;

    if cancelled {
        // The worker was killed, so it could not report what it wrote. The
        // staging directory name is deterministic, so name it anyway: ACQ-011
        // requires a cancelled acquisition to retain and expose partial output.
        return Err(match retained_staging_hint(&request_for_hint, context.id()) {
            Some(result) => JobError::cancelled_with_result(
                result,
                "evidence operation cancelled; partial output retained in staging",
            ),
            None => JobError::cancelled("evidence operation cancelled"),
        });
    }
    if timed_out {
        return Err(JobError::failed(
            "worker.timeout",
            format!(
                "parser worker exceeded the {} second deadline",
                timeout.as_secs()
            ),
        ));
    }
    if overflowed || stdout_truncated {
        return Err(JobError::failed(
            "worker.response_too_large",
            format!(
                "parser worker response exceeded {} bytes",
                MAX_RESPONSE_BYTES
            ),
        ));
    }

    if let Ok(response) = serde_json::from_slice::<WorkerResponse>(&stdout) {
        return response.into_job_result();
    }
    if !status.success() {
        let diagnostics = String::from_utf8_lossy(&stderr);
        let diagnostics = diagnostics.trim();
        let message = if diagnostics.is_empty() {
            format!("parser worker exited with {status}")
        } else {
            format!("parser worker exited with {status}: {diagnostics}")
        };
        return Err(JobError::failed("worker.crashed", message));
    }
    if let Err(error) = writer_result {
        return Err(JobError::failed(
            "worker.ipc",
            format!("unable to send worker request: {error}"),
        ));
    }
    Err(JobError::failed(
        "worker.protocol",
        "parser worker returned an invalid response",
    ))
}

impl WorkerResponse {
    fn outcome(outcome: JobOutcome) -> Self {
        Self {
            ok: true,
            outcome: Some(WorkerOutcome {
                partial: outcome.is_partial(),
                result: outcome.result().clone(),
                message: outcome.message().map(str::to_owned),
            }),
            error: None,
        }
    }

    fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            outcome: None,
            error: Some(WorkerError {
                code: code.into(),
                message: message.into(),
                result: None,
            }),
        }
    }

    fn from_error(error: JobError) -> Self {
        Self {
            ok: false,
            outcome: None,
            error: Some(WorkerError {
                code: error.code().to_owned(),
                message: error.message().to_owned(),
                result: error.result().cloned(),
            }),
        }
    }

    fn into_job_result(self) -> Result<JobOutcome, JobError> {
        if self.ok {
            let outcome = self.outcome.ok_or_else(|| {
                JobError::failed("worker.protocol", "worker success omitted its outcome")
            })?;
            if outcome.partial {
                Ok(JobOutcome::partial(
                    outcome.result,
                    outcome
                        .message
                        .unwrap_or_else(|| "worker reported a partial result".to_owned()),
                ))
            } else {
                Ok(JobOutcome::succeeded(outcome.result))
            }
        } else {
            let error = self.error.ok_or_else(|| {
                JobError::failed("worker.protocol", "worker failure omitted its error")
            })?;
            if error.code == "cancelled" {
                Err(match error.result {
                    Some(result) => {
                        JobError::cancelled_with_result(result, error.message)
                    }
                    None => JobError::cancelled(error.message),
                })
            } else {
                Err(JobError::failed(error.code, error.message))
            }
        }
    }
}

/// Reports the read and write paths a request would be sandboxed with, without
/// running anything.
///
/// The narrowness of the EWF grant is the point of the exercise, so a test has
/// to be able to assert on the policy itself rather than on the job merely
/// succeeding: a grant that silently widened back to the parent directory would
/// pass every behavioural test. Gated like `OFI_PARSER_WORKER`, because handing
/// callers a map of the sandbox is not a capability a release build needs.
#[cfg(debug_assertions)]
pub fn sandbox_paths_for_debug(request: &[u8]) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let request: JobRequest = serde_json::from_slice(request).map_err(|error| error.to_string())?;
    let policy = sandbox_policy(&request).map_err(|error| error.message().to_owned())?;
    Ok((policy.read_paths, policy.write_paths))
}

/// Return the UID of the `ofi-worker` system user.
///
/// This user is created at package install time (DEB postinst) and has no login
/// shell, no home directory, and owns no files. The worker drops to this UID
/// after Landlock grants are established but before any evidence is parsed.
///
/// If the user does not exist, returns UID 65534 (nobody) as a safe fallback.
/// The worker will still run, but without the intended privilege separation.
fn ofi_worker_uid() -> u32 {
    let name = std::ffi::CString::new("ofi-worker").unwrap();
    let pwd = unsafe { libc::getpwnam(name.as_ptr()) };
    if pwd.is_null() {
        eprintln!("warning: ofi-worker user not found, falling back to nobody (65534)");
        65534
    } else {
        unsafe { (*pwd).pw_uid }
    }
}

/// Return the GID of the `ofi-worker` system user's primary group.
///
/// See `ofi_worker_uid` for rationale. Falls back to GID 65534 (nogroup) if the
/// user does not exist.
fn ofi_worker_gid() -> u32 {
    let name = std::ffi::CString::new("ofi-worker").unwrap();
    let pwd = unsafe { libc::getpwnam(name.as_ptr()) };
    if pwd.is_null() {
        eprintln!("warning: ofi-worker user not found, falling back to nogroup (65534)");
        65534
    } else {
        unsafe { (*pwd).pw_gid }
    }
}

fn sandbox_policy(request: &JobRequest) -> Result<SandboxPolicy, JobError> {
    let mut reads = BTreeSet::new();
    let mut writes = BTreeSet::new();
    match request {
        JobRequest::ImageIdentify { path }
        | JobRequest::PartitionAnalysis { path }
        | JobRequest::SourceSnapshot { path } => {
            reads.extend(evidence_read_paths(path)?);
        }
        JobRequest::FilesystemList { path, .. }
        | JobRequest::FilesystemOpen { path, .. }
        | JobRequest::HexPreview { path, .. }
        | JobRequest::TextPreview { path, .. }
        | JobRequest::FileSignature { path, .. }
        | JobRequest::ExtractFile { path, .. }
        | JobRequest::ExtractDirectory { path, .. }
        | JobRequest::Verification { path, .. } => {
            reads.extend(evidence_read_paths(path)?);
        }
        JobRequest::Acquisition {
            source_path,
            destination_dir,
            reports_dir,
            ..
        } => {
            reads.extend(evidence_read_paths(source_path)?);
            writes.insert(existing_path(destination_dir)?);
            if let Some(reports_dir) = reports_dir {
                writes.insert(existing_path(reports_dir)?);
            }
        }
        JobRequest::EvidenceList { .. }
        | JobRequest::AuditAppend { .. }
        | JobRequest::AuditVerify { .. } => {
            return Err(JobError::failed(
                "worker.unsupported_operation",
                "trusted operation cannot build an evidence sandbox",
            ));
        }
    }

    match request {
        JobRequest::Verification {
            reports_dir: Some(directory),
            ..
        } => {
            writes.insert(existing_path(directory)?);
        }
        JobRequest::ExtractFile {
            destination_root, ..
        } => {
            writes.insert(output_root(destination_root)?);
        }
        JobRequest::ExtractDirectory {
            destination_root,
            reports_dir,
            ..
        } => {
            writes.insert(output_root(destination_root)?);
            writes.insert(existing_path(reports_dir)?);
        }
        _ => {}
    }

    for path in ["/usr/share/zoneinfo", "/etc/localtime"] {
        if Path::new(path).exists() {
            reads.insert(std::fs::canonicalize(path).map_err(|error| {
                JobError::failed(
                    "worker.sandbox",
                    format!("unable to resolve {path}: {error}"),
                )
            })?);
        }
    }

    let policy = SandboxPolicy {
        read_paths: reads.into_iter().collect(),
        write_paths: writes.into_iter().collect(),
    };
    policy
        .validate()
        .map_err(|error| JobError::failed("worker.sandbox", error.message().to_owned()))?;
    Ok(policy)
}

/// The paths a reader has to reach to open `path`.
///
/// A non-EWF image is a single file. An EWF image is a `E01`/`E02`/… series
/// that `EwfReader::open` discovers by itself, so each segment is named
/// individually here rather than granting the directory that holds them:
/// Landlock `PATH_BENEATH` accepts a regular file, and a directory grant would
/// also expose every unrelated file beside the evidence to a compromised
/// parser. Each segment is then canonicalised, which is what the LSM compares.
///
/// Whether the path is EWF is decided the same way `open_image` decides it —
/// by file signature, not by extension. The two have to agree: a signature
/// check here and an extension check there would disagree on a letter-suffixed
/// segment such as `EAA`, or on an image whose extension does not match its
/// contents, and the grant would then cover a set of files the parse does not
/// use, or miss files it does.
///
/// A first segment whose siblings cannot be enumerated is a hard error, not a
/// narrowed grant: `segment_paths` returns the set it could see, so an empty
/// listing means the evidence itself is unreadable, and continuing would run
/// the job under a policy that the parse is then certain to fail against.
fn evidence_read_paths(path: &str) -> Result<Vec<PathBuf>, JobError> {
    let path = existing_path(path)?;
    let is_ewf = libewf_bindings::is_ewf(&path).map_err(|error| {
        JobError::failed(
            "worker.sandbox",
            format!("sandbox path is unreadable: {error}"),
        )
    })?;
    if !is_ewf {
        return Ok(vec![path]);
    }
    let segments = libewf_bindings::segment_paths(&path);
    if segments.is_empty() {
        return Err(JobError::failed(
            "worker.sandbox",
            format!("EWF image has no readable segment: {}", path.display()),
        ));
    }
    let mut resolved = Vec::with_capacity(segments.len());
    for segment in segments {
        resolved.push(existing_path(&segment.to_string_lossy())?);
    }
    Ok(resolved)
}

fn existing_path(path: &str) -> Result<PathBuf, JobError> {
    std::fs::canonicalize(path).map_err(|error| {
        JobError::failed(
            "worker.sandbox",
            format!("sandbox path is unavailable: {path}: {error}"),
        )
    })
}

fn output_root(path: &str) -> Result<PathBuf, JobError> {
    let path = Path::new(path);
    if path.exists() {
        return existing_path(&path.to_string_lossy());
    }
    let parent = path.parent().ok_or_else(|| {
        JobError::failed("worker.sandbox", "output directory has no parent directory")
    })?;
    existing_path(&parent.to_string_lossy())
}

fn resolve_worker_path() -> Result<PathBuf, JobError> {
    // The override exists so tests can substitute a stand-in worker. In a release
    // build it would let any process able to set the desktop's environment run
    // arbitrary code with the worker's authority, so it is debug-only.
    if cfg!(debug_assertions)
        && let Some(path) = std::env::var_os("OFI_PARSER_WORKER")
    {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe().map_err(|error| {
        JobError::failed(
            "worker.path",
            format!("unable to locate the current executable: {error}"),
        )
    })?;
    for ancestor in current.ancestors().skip(1).take(3) {
        let candidate = ancestor.join("ofi-parser-worker");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(JobError::failed(
        "worker.path",
        "ofi-parser-worker was not found beside the application",
    ))
}

fn worker_timeout() -> Duration {
    let seconds = std::env::var("OFI_WORKER_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .min(MAX_TIMEOUT_SECONDS);
    Duration::from_secs(seconds)
}

fn kill_worker_group(child: &mut Child) -> io::Result<()> {
    let process_group = -(child.id() as i32);
    let result = unsafe { libc::kill(process_group, libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

fn read_bounded(reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut reader = reader;
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok((output, false));
        }
        // Stop at the first byte past the cap instead of draining to EOF: the
        // caller kills the worker on truncation, so continuing to read would
        // turn the cap into unbounded host I/O and a possible deadline stall.
        if output.len().saturating_add(count) > limit {
            return Ok((output, true));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}
