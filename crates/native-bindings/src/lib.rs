use std::any::Any;
use std::collections::BTreeMap;
use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::ptr;
use std::str::FromStr;

use acquisition::{AcquisitionOptions, OutputFormat};
use audit_log::{AuditEvent, AuditLog};
use evidence_manager::{AddEvidence, CaseStore, CreateCase, EvidenceRecord, SourceSnapshot};
use forensic_core::{CaseId, EvidenceId, JobId};
use hashing::HashAlgorithm;
use job_manager::{JobError, JobManager, JobOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod parser_worker;

pub use parser_worker::run_stdio_worker;

const ABI_MAJOR: u32 = 1;
const ABI_MINOR: u32 = 2;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

/// Default audit log name beside a case database, matching the desktop client.
const CASE_AUDIT_FILE_NAME: &str = "audit.jsonl";

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ofi_status {
    OFI_STATUS_OK = 0,
    OFI_STATUS_INVALID_ARGUMENT = 1,
    OFI_STATUS_INVALID_JSON = 2,
    OFI_STATUS_OPERATION_FAILED = 3,
    OFI_STATUS_PANIC = 4,
}

#[allow(non_camel_case_types)]
pub struct ofi_error {
    code: CString,
    message: CString,
}

#[derive(Deserialize)]
struct CaseCreateRequest {
    db_path: String,
    id: CaseId,
    title: String,
    examiner: String,
    audit_path: Option<String>,
}

#[derive(Deserialize)]
struct EvidenceAddRequest {
    db_path: String,
    id: EvidenceId,
    case_id: CaseId,
    description: String,
    source_path: String,
    source: SourceSnapshot,
    audit_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum JobRequest {
    ImageIdentify {
        path: String,
    },
    PartitionAnalysis {
        path: String,
    },
    FilesystemList {
        path: String,
        offset_bytes: u64,
        inode: u64,
    },
    FilesystemOpen {
        path: String,
        offset_bytes: u64,
    },
    Verification {
        path: String,
        algorithms: Vec<HashAlgorithm>,
        expected: BTreeMap<HashAlgorithm, String>,
        reports_dir: Option<String>,
        report_base_name: Option<String>,
    },
    Acquisition {
        source_path: String,
        destination_dir: String,
        base_name: String,
        format: OutputFormat,
        options: AcquisitionOptions,
        reports_dir: Option<String>,
        report_base_name: Option<String>,
    },
    SourceSnapshot {
        path: String,
    },
    EvidenceList {
        db_path: String,
        case_id: CaseId,
    },
    AuditAppend {
        audit_path: String,
        case_id: CaseId,
        actor: String,
        action: String,
        subject_id: Option<String>,
        details: BTreeMap<String, String>,
        /// Case database the event is about. When present the audit path must
        /// resolve to that case's recorded log.
        db_path: Option<String>,
    },
    AuditVerify {
        audit_path: String,
        /// Case database to bind the verification to. When present, a chain
        /// belonging to a different case is rejected instead of reported valid.
        db_path: Option<String>,
        case_id: Option<CaseId>,
    },
    HexPreview {
        path: String,
        offset_bytes: u64,
        metadata_address: u64,
        file_offset: u64,
        length: usize,
    },
    TextPreview {
        path: String,
        offset_bytes: u64,
        metadata_address: u64,
        file_offset: u64,
        max_bytes: usize,
    },
    FileSignature {
        path: String,
        offset_bytes: u64,
        metadata_address: u64,
    },
    ExtractFile {
        path: String,
        offset_bytes: u64,
        parent_inode: u64,
        metadata_address: u64,
        destination_root: String,
    },
    ExtractDirectory {
        path: String,
        offset_bytes: u64,
        inode: u64,
        directory_name: String,
        destination_root: String,
        reports_dir: String,
        base_name: String,
    },
}

impl JobRequest {
    fn operation_name(&self) -> &'static str {
        match self {
            Self::ImageIdentify { .. } => "image_identify",
            Self::PartitionAnalysis { .. } => "partition_analysis",
            Self::FilesystemList { .. } => "filesystem_list",
            Self::FilesystemOpen { .. } => "filesystem_open",
            Self::Verification { .. } => "verification",
            Self::Acquisition { .. } => "acquisition",
            Self::SourceSnapshot { .. } => "source_snapshot",
            Self::EvidenceList { .. } => "evidence_list",
            Self::AuditAppend { .. } => "audit_append",
            Self::AuditVerify { .. } => "audit_verify",
            Self::HexPreview { .. } => "hex_preview",
            Self::TextPreview { .. } => "text_preview",
            Self::FileSignature { .. } => "file_signature",
            Self::ExtractFile { .. } => "extract_file",
            Self::ExtractDirectory { .. } => "extract_directory",
        }
    }

    fn is_isolated(&self) -> bool {
        matches!(
            self,
            Self::ImageIdentify { .. }
                | Self::PartitionAnalysis { .. }
                | Self::FilesystemList { .. }
                | Self::FilesystemOpen { .. }
                | Self::Verification { .. }
                | Self::Acquisition { .. }
                | Self::SourceSnapshot { .. }
                | Self::HexPreview { .. }
                | Self::TextPreview { .. }
                | Self::FileSignature { .. }
                | Self::ExtractFile { .. }
                | Self::ExtractDirectory { .. }
        )
    }
}

/// Decodes a job request without running its operation.
///
/// This is the side-effect-free entry point used by stable property tests and
/// fuzzing. The C ABI applies the same parser before dispatching a job.
pub fn validate_job_request(request: &[u8]) -> Result<&'static str, String> {
    if request.is_empty() || request.len() > MAX_REQUEST_BYTES {
        return Err("request must be a non-empty bounded byte string".to_owned());
    }
    let request = parse_job_request(request).map_err(|error| error.message)?;
    Ok(request.operation_name())
}

#[allow(non_camel_case_types)]
pub struct ofi_job_manager {
    manager: JobManager,
}

#[derive(Serialize)]
struct SuccessResponse<T> {
    abi_major: u32,
    abi_minor: u32,
    ok: bool,
    body: T,
}

struct FfiError {
    status: ofi_status,
    code: &'static str,
    message: String,
}

#[unsafe(no_mangle)]
pub extern "C" fn ofi_abi_version() -> u32 {
    ABI_MAJOR
}

#[unsafe(no_mangle)]
pub extern "C" fn ofi_abi_version_minor() -> u32 {
    ABI_MINOR
}

#[unsafe(no_mangle)]
/// Creates a background job manager.
///
/// # Safety
///
/// `output` and `error` must point to writable pointer slots. The caller owns a
/// successful manager and must release it with `ofi_job_manager_free`.
pub unsafe extern "C" fn ofi_job_manager_new(
    worker_threads: u32,
    output: *mut *mut ofi_job_manager,
    error: *mut *mut ofi_error,
) -> ofi_status {
    unsafe {
        initialize_outputs(ptr::null_mut(), ptr::null_mut(), error);
        if !output.is_null() {
            *output = ptr::null_mut();
        }
    }
    if output.is_null() {
        unsafe {
            set_error(
                error,
                FfiError {
                    status: ofi_status::OFI_STATUS_INVALID_ARGUMENT,
                    code: "invalid_argument",
                    message: "job manager output must not be null".to_owned(),
                },
            );
        }
        return ofi_status::OFI_STATUS_INVALID_ARGUMENT;
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        JobManager::new(worker_threads as usize)
    }));
    match outcome {
        Ok(Ok(manager)) => {
            unsafe {
                *output = Box::into_raw(Box::new(ofi_job_manager { manager }));
            }
            ofi_status::OFI_STATUS_OK
        }
        Ok(Err(manager_error)) => {
            let code = if worker_threads == 0 {
                "job.invalid_worker_count"
            } else {
                "job.runtime"
            };
            unsafe {
                set_error(
                    error,
                    FfiError {
                        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
                        code,
                        message: manager_error.message().to_owned(),
                    },
                );
            }
            ofi_status::OFI_STATUS_OPERATION_FAILED
        }
        Err(_) => {
            unsafe {
                set_error(
                    error,
                    FfiError {
                        status: ofi_status::OFI_STATUS_PANIC,
                        code: "panic",
                        message: "internal panic was contained at the ABI boundary".to_owned(),
                    },
                );
            }
            ofi_status::OFI_STATUS_PANIC
        }
    }
}

#[unsafe(no_mangle)]
/// Frees a job manager.
///
/// # Safety
///
/// `manager` must be null or a pointer returned by `ofi_job_manager_new` and
/// must not be used after this call.
pub unsafe extern "C" fn ofi_job_manager_free(manager: *mut ofi_job_manager) {
    if !manager.is_null() {
        unsafe {
            drop(Box::from_raw(manager));
        }
    }
}

#[unsafe(no_mangle)]
/// Submits a supported background operation from a UTF-8 JSON request.
///
/// # Safety
///
/// `manager` must be a live manager. Request and output pointer rules match
/// `ofi_case_create`.
pub unsafe extern "C" fn ofi_job_submit(
    manager: *mut ofi_job_manager,
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) -> ofi_status {
    if manager.is_null() {
        unsafe {
            initialize_outputs(response, response_len, error);
            set_error(
                error,
                FfiError {
                    status: ofi_status::OFI_STATUS_INVALID_ARGUMENT,
                    code: "invalid_argument",
                    message: "job manager must not be null".to_owned(),
                },
            );
        }
        return ofi_status::OFI_STATUS_INVALID_ARGUMENT;
    }
    let manager = unsafe { &(*manager).manager };
    unsafe {
        run_adapter(
            request,
            request_len,
            response,
            response_len,
            error,
            |bytes| handle_job_submit(manager, bytes),
        )
    }
}

#[unsafe(no_mangle)]
/// Returns a job snapshot by UUID.
///
/// # Safety
///
/// `manager` must be live and `job_id` must point to a NUL-terminated UTF-8
/// string. Output pointer rules match `ofi_case_create`.
pub unsafe extern "C" fn ofi_job_snapshot(
    manager: *mut ofi_job_manager,
    job_id: *const c_char,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) -> ofi_status {
    unsafe {
        initialize_outputs(response, response_len, error);
    }
    let Some((manager, job_id)) = (unsafe { job_manager_and_id(manager, job_id, error) }) else {
        return ofi_status::OFI_STATUS_INVALID_ARGUMENT;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let snapshot = manager
            .snapshot(&job_id)
            .map_err(job_operation_error)?
            .ok_or_else(|| FfiError {
                status: ofi_status::OFI_STATUS_OPERATION_FAILED,
                code: "job.not_found",
                message: format!("job {job_id} was not found"),
            })?;
        success_bytes(snapshot)
    }));
    unsafe { complete_adapter_outcome(outcome, response, response_len, error) }
}

#[unsafe(no_mangle)]
/// Requests cancellation of a non-terminal job.
///
/// # Safety
///
/// Pointer rules match `ofi_job_snapshot`.
pub unsafe extern "C" fn ofi_job_cancel(
    manager: *mut ofi_job_manager,
    job_id: *const c_char,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) -> ofi_status {
    unsafe {
        initialize_outputs(response, response_len, error);
    }
    let Some((manager, job_id)) = (unsafe { job_manager_and_id(manager, job_id, error) }) else {
        return ofi_status::OFI_STATUS_INVALID_ARGUMENT;
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        success_bytes(json!({"cancelled": manager.cancel(&job_id)}))
    }));
    unsafe { complete_adapter_outcome(outcome, response, response_len, error) }
}

#[unsafe(no_mangle)]
/// Creates a case from a UTF-8 JSON request.
///
/// # Safety
///
/// `request` must point to `request_len` readable bytes. The response and error
/// outputs must point to writable pointer slots. The caller owns any returned
/// response or error and must release it with the matching free function.
pub unsafe extern "C" fn ofi_case_create(
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) -> ofi_status {
    unsafe {
        run_adapter(
            request,
            request_len,
            response,
            response_len,
            error,
            handle_case_create,
        )
    }
}

#[unsafe(no_mangle)]
/// Adds evidence from a UTF-8 JSON request.
///
/// # Safety
///
/// `request` must point to `request_len` readable bytes. The response and error
/// outputs must point to writable pointer slots. The caller owns any returned
/// response or error and must release it with the matching free function.
pub unsafe extern "C" fn ofi_evidence_add(
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) -> ofi_status {
    unsafe {
        run_adapter(
            request,
            request_len,
            response,
            response_len,
            error,
            handle_evidence_add,
        )
    }
}

#[unsafe(no_mangle)]
/// Frees a response buffer returned by this ABI.
///
/// # Safety
///
/// `bytes` must be null or a pointer previously returned by this library, and
/// `len` must be its matching returned length. The pointer must not be used
/// after this call.
pub unsafe extern "C" fn ofi_bytes_free(bytes: *mut u8, len: usize) {
    if bytes.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(bytes, len)));
    }
}

#[unsafe(no_mangle)]
/// Frees an error returned by this ABI.
///
/// # Safety
///
/// `error` must be null or a pointer previously returned by this library and
/// must not be freed again.
pub unsafe extern "C" fn ofi_error_free(error: *mut ofi_error) {
    if !error.is_null() {
        unsafe {
            drop(Box::from_raw(error));
        }
    }
}

#[unsafe(no_mangle)]
/// Returns the stable error code string.
///
/// # Safety
///
/// `error` must be null or remain valid and unfreed for the returned lifetime.
pub unsafe extern "C" fn ofi_error_code(error: *const ofi_error) -> *const c_char {
    if error.is_null() {
        return ptr::null();
    }
    unsafe { (*error).code.as_ptr() }
}

#[unsafe(no_mangle)]
/// Returns the human-readable error message.
///
/// # Safety
///
/// `error` must be null or remain valid and unfreed for the returned lifetime.
pub unsafe extern "C" fn ofi_error_message(error: *const ofi_error) -> *const c_char {
    if error.is_null() {
        return ptr::null();
    }
    unsafe { (*error).message.as_ptr() }
}

unsafe fn run_adapter<F>(
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
    handler: F,
) -> ofi_status
where
    F: FnOnce(&[u8]) -> Result<Vec<u8>, FfiError>,
{
    unsafe {
        initialize_outputs(response, response_len, error);
    }
    if response.is_null() || response_len.is_null() {
        unsafe {
            set_error(
                error,
                FfiError {
                    status: ofi_status::OFI_STATUS_INVALID_ARGUMENT,
                    code: "invalid_argument",
                    message: "response pointers must not be null".to_owned(),
                },
            );
        }
        return ofi_status::OFI_STATUS_INVALID_ARGUMENT;
    }

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let request = unsafe { request_bytes(request, request_len)? };
        handler(&request)
    }));

    unsafe { complete_adapter_outcome(outcome, response, response_len, error) }
}

unsafe fn complete_adapter_outcome(
    outcome: Result<Result<Vec<u8>, FfiError>, Box<dyn Any + Send>>,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) -> ofi_status {
    match outcome {
        Ok(Ok(bytes)) => {
            let boxed = bytes.into_boxed_slice();
            let len = boxed.len();
            let bytes = Box::into_raw(boxed) as *mut u8;
            unsafe {
                *response = bytes;
                *response_len = len;
            }
            ofi_status::OFI_STATUS_OK
        }
        Ok(Err(ffi_error)) => {
            let status = ffi_error.status;
            unsafe {
                set_error(error, ffi_error);
            }
            status
        }
        Err(_) => {
            unsafe {
                set_error(
                    error,
                    FfiError {
                        status: ofi_status::OFI_STATUS_PANIC,
                        code: "panic",
                        message: "internal panic was contained at the ABI boundary".to_owned(),
                    },
                );
            }
            ofi_status::OFI_STATUS_PANIC
        }
    }
}

unsafe fn job_manager_and_id<'a>(
    manager: *mut ofi_job_manager,
    job_id: *const c_char,
    error: *mut *mut ofi_error,
) -> Option<(&'a JobManager, JobId)> {
    if manager.is_null() || job_id.is_null() {
        unsafe {
            set_error(
                error,
                FfiError {
                    status: ofi_status::OFI_STATUS_INVALID_ARGUMENT,
                    code: "invalid_argument",
                    message: "job manager and job id must not be null".to_owned(),
                },
            );
        }
        return None;
    }
    let job_id = unsafe { std::ffi::CStr::from_ptr(job_id) }
        .to_str()
        .ok()
        .and_then(|value| JobId::from_str(value).ok());
    let Some(job_id) = job_id else {
        unsafe {
            set_error(
                error,
                FfiError {
                    status: ofi_status::OFI_STATUS_INVALID_ARGUMENT,
                    code: "invalid_argument",
                    message: "job id must be a UUID".to_owned(),
                },
            );
        }
        return None;
    };
    let manager = unsafe { &(*manager).manager };
    Some((manager, job_id))
}

fn handle_job_submit(manager: &JobManager, request_bytes: &[u8]) -> Result<Vec<u8>, FfiError> {
    let request = parse_job_request(request_bytes)?;
    if request.is_isolated() {
        let operation = request.operation_name();
        let worker_request = request_bytes.to_vec();
        let job_id = manager
            .submit(operation, move |context| {
                parser_worker::run_job(&worker_request, &context)
            })
            .map_err(job_submit_error)?;
        return success_bytes(json!({"job_id": job_id}));
    }
    let job_id = match request {
        JobRequest::EvidenceList { db_path, case_id } => {
            manager.submit("evidence_list", move |_| {
                let store = CaseStore::open(Path::new(&db_path))
                    .map_err(|error| JobError::failed("evidence.store", error.to_string()))?;
                let evidence: Vec<EvidenceRecord> = store
                    .list_evidence(case_id)
                    .map_err(|error| JobError::failed("evidence.list", error.to_string()))?;
                job_success(evidence)
            })
        }
        JobRequest::AuditAppend {
            audit_path,
            case_id,
            actor,
            action,
            subject_id,
            details,
            db_path,
        } => manager.submit("audit_append", move |_| {
            let mut log = AuditLog::open_append(Path::new(&audit_path))
                .map_err(|error| JobError::failed("audit.open", error.to_string()))?;
            // A caller-supplied path may not target another case's history.
            if let Some(db_path) = db_path {
                let store = CaseStore::open(Path::new(&db_path))
                    .map_err(|error| JobError::failed("audit.case", error.to_string()))?;
                require_case_audit_binding(&store, case_id, log.path())
                    .map_err(|error| JobError::failed(error.code, error.message))?;
            }
            let receipt = log
                .append(AuditEvent {
                    case_id,
                    job_id: None,
                    actor,
                    action,
                    subject_id,
                    details,
                })
                .map_err(|error| JobError::failed("audit.append", error.to_string()))?;
            job_success(json!({
                "sequence": receipt.sequence,
                "record_hash": receipt.record_hash.to_hex(),
            }))
        }),
        JobRequest::AuditVerify {
            audit_path,
            db_path,
            case_id,
        } => manager.submit("audit_verify", move |_| {
            // Bind the verification to the case before attesting anything, so a
            // valid chain belonging to a different case is not reported.
            let binding = match (db_path, case_id) {
                (Some(db_path), Some(case_id)) => {
                    let store = CaseStore::open(Path::new(&db_path))
                        .map_err(|error| JobError::failed("audit.case", error.to_string()))?;
                    require_case_audit_binding(&store, case_id, Path::new(&audit_path))
                        .map_err(|error| JobError::failed(error.code, error.message))?;
                    Some((store, case_id))
                }
                _ => None,
            };
            let status = AuditLog::verify_chain(Path::new(&audit_path))
                .map_err(|error| JobError::failed("audit.verify", error.to_string()))?;
            if let Some((store, case_id)) = binding {
                // The chain must still end where this case last recorded. A chain
                // that verifies but sits behind the recorded head has lost or
                // rolled back records, which chain-internal checks cannot see.
                let recorded = store
                    .case_audit_head(case_id)
                    .map_err(|error| JobError::failed("audit.case", error.to_string()))?;
                let head = status.head_hash().map(|hash| hash.to_hex().to_owned());
                if let Some(recorded) = recorded
                    && recorded != head.clone().unwrap_or_default()
                {
                    return Err(JobError::failed(
                        "audit.head_mismatch",
                        format!(
                            "audit chain head {} does not match the head recorded for this case ({recorded})",
                            head.as_deref().unwrap_or("none")
                        ),
                    ));
                }
                store
                    .record_case_audit_head(case_id, head.as_deref())
                    .map_err(|error| JobError::failed("audit.case", error.to_string()))?;
            }
            job_success(json!({
                "valid": status.is_valid(),
                "record_count": status.record_count(),
                "head_hash": status.head_hash().map(|hash| hash.to_hex()),
                "reason": status.reason(),
            }))
        }),
        _ => {
            return Err(FfiError {
                status: ofi_status::OFI_STATUS_OPERATION_FAILED,
                code: "job.isolation_required",
                message: "evidence operation bypassed the parser worker".to_owned(),
            });
        }
    }
    .map_err(job_submit_error)?;
    success_bytes(json!({"job_id": job_id}))
}

fn parse_job_request(request: &[u8]) -> Result<JobRequest, FfiError> {
    serde_json::from_slice(request).map_err(|error| FfiError {
        status: ofi_status::OFI_STATUS_INVALID_JSON,
        code: "invalid_json",
        message: error.to_string(),
    })
}

fn job_success<T: Serialize>(value: T) -> Result<JobOutcome, JobError> {
    Ok(JobOutcome::succeeded(job_value(value)?))
}

fn job_value<T: Serialize>(value: T) -> Result<Value, JobError> {
    serde_json::to_value(value)
        .map_err(|error| JobError::failed("job.serialization", error.to_string()))
}

fn job_submit_error(error: JobError) -> FfiError {
    FfiError {
        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
        code: "job.submit",
        message: error.to_string(),
    }
}

fn job_operation_error(error: JobError) -> FfiError {
    FfiError {
        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
        code: "job.operation_failed",
        message: error.to_string(),
    }
}

unsafe fn request_bytes(request: *const u8, request_len: usize) -> Result<Vec<u8>, FfiError> {
    if request.is_null() || request_len == 0 || request_len > MAX_REQUEST_BYTES {
        return Err(FfiError {
            status: ofi_status::OFI_STATUS_INVALID_ARGUMENT,
            code: "invalid_argument",
            message: "request must be a non-empty bounded byte string".to_owned(),
        });
    }
    Ok(unsafe { std::slice::from_raw_parts(request, request_len) }.to_vec())
}

fn handle_case_create(request: &[u8]) -> Result<Vec<u8>, FfiError> {
    let request: CaseCreateRequest = parse_request(request)?;
    let path = case_audit_path(&request.db_path, request.audit_path.as_deref());
    // Preflight before the commit so an unusable log blocks the write. The log
    // is closed immediately: holding an exclusive lock across the database write
    // (or across a worker fork) would block later audited operations.
    open_case_audit(&path)?;
    let store =
        CaseStore::open(Path::new(&request.db_path)).map_err(|error| operation_error(&error))?;
    let record = store
        .create_case(CreateCase {
            id: request.id,
            title: request.title,
            examiner: request.examiner,
        })
        .map_err(|error| operation_error(&error))?;
    // Replay the queued event before binding: an append failure must surface as
    // audit.append_after_commit with the event retained, not as a bind error.
    let bound = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    replay_outbox(&store, &bound)?;
    store
        .bind_case_audit(record.id, &bound)
        .map_err(|error| operation_error(&error))?;
    success_bytes(record)
}

fn handle_evidence_add(request: &[u8]) -> Result<Vec<u8>, FfiError> {
    let request: EvidenceAddRequest = parse_request(request)?;
    let path = case_audit_path(&request.db_path, request.audit_path.as_deref());
    open_case_audit(&path)?;
    let store =
        CaseStore::open(Path::new(&request.db_path)).map_err(|error| operation_error(&error))?;
    // Evidence may only be added to the case's own log, never to an unrelated one.
    require_case_audit_binding(&store, request.case_id, &path)?;
    let record = store
        .add_evidence(AddEvidence {
            id: request.id,
            case_id: request.case_id,
            description: request.description,
            source_path: request.source_path,
            source: request.source,
        })
        .map_err(|error| operation_error(&error))?;
    // The evidence row and its audit event committed together, so replaying the
    // outbox records this event and any earlier one that could not be appended.
    replay_outbox(&store, &path)?;
    success_bytes(record)
}

/// Resolves the audit log path for a case write.
///
/// An omitted path falls back to `audit.jsonl` beside the case database, which
/// is the same location the desktop uses.
fn case_audit_path(db_path: &str, path: Option<&str>) -> PathBuf {
    let db = Path::new(db_path);
    // Resolve against the database's own directory in absolute form, so a later
    // lookup cannot land in the process working directory.
    let parent = db
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        });
    let parent = std::fs::canonicalize(&parent).unwrap_or(parent);
    let candidate = path.map_or_else(
        || parent.join(CASE_AUDIT_FILE_NAME),
        PathBuf::from,
    );
    if candidate.is_absolute() {
        candidate
    } else {
        parent.join(candidate)
    }
}

/// Proves the audit log can be opened and appended to, then closes it.
///
/// Case and evidence state is never committed without its audit record, so this
/// runs before the database write and fails the operation when the log is
/// unusable. The handle is dropped immediately: the log holds an exclusive
/// lock, and holding it across the database write or a worker fork would block
/// the next audited operation.
fn open_case_audit(path: &Path) -> Result<(), FfiError> {
    AuditLog::open_append(path)
        .map(drop)
        .map_err(|error| FfiError {
            status: ofi_status::OFI_STATUS_OPERATION_FAILED,
            code: "audit.open",
            message: error.to_string(),
        })
}

/// Rejects an audit log that does not belong to the case being written.
///
/// A case binds its log on creation. Once bound, writing evidence through a
/// different log would let one case's events land in another case's history.
fn require_case_audit_binding(
    store: &CaseStore,
    case_id: CaseId,
    audit_path: &Path,
) -> Result<(), FfiError> {
    let bound = store.case_audit_path(case_id).map_err(|error| FfiError {
        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
        code: "audit.case",
        message: error.to_string(),
    })?;
    let Some(bound) = bound else {
        // Unbound (a pre-upgrade case): adopt this log as the case's log, stored
        // absolute so a later relative lookup cannot resolve to a different file.
        let absolute = std::fs::canonicalize(audit_path).unwrap_or_else(|_| audit_path.to_path_buf());
        return store
            .bind_case_audit(case_id, &absolute)
            .map_err(|error| FfiError {
                status: ofi_status::OFI_STATUS_OPERATION_FAILED,
                code: "audit.case",
                message: error.to_string(),
            });
    };
    if !same_path(&bound, audit_path) {
        return Err(FfiError {
            status: ofi_status::OFI_STATUS_OPERATION_FAILED,
            code: "audit.case",
            message: format!(
                "audit log {} does not belong to the case, which uses {}",
                audit_path.display(),
                bound.display()
            ),
        });
    }
    Ok(())
}

/// Compares two audit paths, treating canonical forms as equal.
fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Replays pending outbox events into the case's audit log.
///
/// Every event is marked appended only after the append returned a receipt, so a
/// failure leaves it pending for the next attempt instead of losing it. A
/// committed case or evidence row therefore always has a recoverable route into
/// the authoritative chain.
fn replay_outbox(store: &CaseStore, path: &Path) -> Result<(), FfiError> {
    let Some(case_id) = store.pending_case_id().map_err(|error| operation_error(&error))? else {
        return Ok(());
    };
    let pending = store
        .pending_audit_events(case_id)
        .map_err(|error| operation_error(&error))?;
    if pending.is_empty() {
        return Ok(());
    }
    let mut log = AuditLog::open_append(path).map_err(|error| FfiError {
        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
        code: "audit.append_after_commit",
        message: format!(
            "state was committed but the audit append failed; {} event(s) remain queued: {error}",
            pending.len()
        ),
    })?;
    for (outbox_id, event_json) in pending {
        let event: AuditEvent = serde_json::from_str(&event_json).map_err(|error| FfiError {
            status: ofi_status::OFI_STATUS_OPERATION_FAILED,
            code: "audit.outbox",
            message: format!("queued audit event is unreadable: {error}"),
        })?;
        let receipt = match log.append(event) {
            Ok(receipt) => receipt,
            Err(error) => {
                return Err(FfiError {
                    status: ofi_status::OFI_STATUS_OPERATION_FAILED,
                    code: "audit.append_after_commit",
                    message: format!(
                        "state was committed but the audit append failed; queued events are retained: {error}"
                    ),
                });
            }
        };
        store
            .mark_audit_event_appended(outbox_id, receipt.sequence)
            .map_err(|error| operation_error(&error))?;
    }
    drop(log);
    Ok(())
}

fn parse_request<'a, T: Deserialize<'a>>(request: &'a [u8]) -> Result<T, FfiError> {
    serde_json::from_slice(request).map_err(|error| FfiError {
        status: ofi_status::OFI_STATUS_INVALID_JSON,
        code: "invalid_json",
        message: error.to_string(),
    })
}

fn success_bytes<T: Serialize>(body: T) -> Result<Vec<u8>, FfiError> {
    let response = SuccessResponse {
        abi_major: ABI_MAJOR,
        abi_minor: ABI_MINOR,
        ok: true,
        body,
    };
    serde_json::to_vec(&response).map_err(|error| FfiError {
        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
        code: "serialization",
        message: error.to_string(),
    })
}

fn operation_error(error: &impl std::fmt::Display) -> FfiError {
    FfiError {
        status: ofi_status::OFI_STATUS_OPERATION_FAILED,
        code: "operation_failed",
        message: error.to_string(),
    }
}

unsafe fn initialize_outputs(
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
) {
    unsafe {
        if !response.is_null() {
            *response = ptr::null_mut();
        }
        if !response_len.is_null() {
            *response_len = 0;
        }
        if !error.is_null() {
            *error = ptr::null_mut();
        }
    }
}

unsafe fn set_error(output: *mut *mut ofi_error, error: FfiError) {
    if output.is_null() {
        return;
    }
    let error = ofi_error {
        code: safe_c_string(error.code),
        message: safe_c_string(&error.message),
    };
    unsafe {
        *output = Box::into_raw(Box::new(error));
    }
}

fn safe_c_string(value: &str) -> CString {
    CString::new(value.replace('\0', "\\0")).expect("replacement removes NUL bytes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_is_contained_and_reported() {
        fn panicking(_: &[u8]) -> Result<Vec<u8>, FfiError> {
            std::panic::resume_unwind(Box::new("synthetic panic"))
        }

        let request = b"{}";
        let mut response = ptr::null_mut();
        let mut response_len = 0;
        let mut error = ptr::null_mut();
        let status = unsafe {
            run_adapter(
                request.as_ptr(),
                request.len(),
                &mut response,
                &mut response_len,
                &mut error,
                panicking,
            )
        };

        assert_eq!(status, ofi_status::OFI_STATUS_PANIC);
        assert!(response.is_null());
        assert!(!error.is_null());
        unsafe { ofi_error_free(error) };
    }

    #[test]
    fn error_accessors_are_null_safe() {
        assert!(unsafe { ofi_error_code(ptr::null()) }.is_null());
        assert!(unsafe { ofi_error_message(ptr::null()) }.is_null());
        let text = c"invalid_argument";
        assert_eq!(text.to_str().unwrap(), "invalid_argument");
    }
}
