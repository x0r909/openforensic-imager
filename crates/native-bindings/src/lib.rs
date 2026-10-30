use std::any::Any;
use std::collections::BTreeMap;
use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;
use std::str::FromStr;

use acquisition::{
    AcquisitionOptions, AcquisitionRequest, AcquisitionStatus, OutputFormat,
    acquire_path_with_cancel, verify_image,
};
use evidence_manager::{AddEvidence, CaseStore, CreateCase, SourceSnapshot};
use extraction::{ExtractionOptions, Extractor};
use filesystem::{EvidenceFile, FileSystem};
use forensic_core::{CaseId, EvidenceId, JobId};
use hashing::HashAlgorithm;
use job_manager::{JobError, JobManager, JobOutcome};
use partition_analyzer::analyze_partitions;
use reporting::write_extraction_report_set;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use viewer::{ByteSource, SourceError, hex_page, identify_signature, preview_text};

const ABI_MAJOR: u32 = 1;
const ABI_MINOR: u32 = 1;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

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
}

#[derive(Deserialize)]
struct EvidenceAddRequest {
    db_path: String,
    id: EvidenceId,
    case_id: CaseId,
    description: String,
    source_path: String,
    source: SourceSnapshot,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum JobRequest {
    PartitionAnalysis {
        path: String,
    },
    FilesystemList {
        path: String,
        offset_bytes: u64,
        inode: u64,
    },
    Verification {
        path: String,
        algorithms: Vec<HashAlgorithm>,
        expected: BTreeMap<HashAlgorithm, String>,
    },
    Acquisition {
        source_path: String,
        destination_dir: String,
        base_name: String,
        format: OutputFormat,
        options: AcquisitionOptions,
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

fn handle_job_submit(manager: &JobManager, request: &[u8]) -> Result<Vec<u8>, FfiError> {
    let request: JobRequest = parse_request(request)?;
    let job_id = match request {
        JobRequest::PartitionAnalysis { path } => manager.submit("partition_analysis", move |_| {
            let analysis = analyze_partitions(Path::new(&path))
                .map_err(|error| JobError::failed("partition.failed", error.to_string()))?;
            job_success(analysis)
        }),
        JobRequest::FilesystemList {
            path,
            offset_bytes,
            inode,
        } => manager.submit("filesystem_list", move |_| {
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
        }),
        JobRequest::Verification {
            path,
            algorithms,
            expected,
        } => manager.submit("verification", move |_| {
            let report = verify_image(Path::new(&path), &algorithms, &expected)
                .map_err(|error| JobError::failed("verification.failed", error.to_string()))?;
            job_success(report)
        }),
        JobRequest::Acquisition {
            source_path,
            destination_dir,
            base_name,
            format,
            options,
        } => manager.submit("acquisition", move |context| {
            let result = acquire_path_with_cancel(
                AcquisitionRequest {
                    source_path: source_path.into(),
                    destination_dir: destination_dir.into(),
                    base_name,
                    format,
                    options,
                },
                &context.cancellation_token(),
            )
            .map_err(|error| JobError::failed("acquisition.failed", error.to_string()))?;
            match result.status {
                AcquisitionStatus::Succeeded => job_success(json!({
                    "status": result.status,
                    "manifest": result.manifest,
                    "manifest_path": result.manifest_path,
                })),
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
                AcquisitionStatus::Cancelled => Err(JobError::cancelled("acquisition cancelled")),
            }
        }),
        JobRequest::HexPreview {
            path,
            offset_bytes,
            metadata_address,
            file_offset,
            length,
        } => manager.submit("hex_preview", move |_| {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let file = filesystem
                .open_file(metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let source = EvidenceByteSource { file };
            let page = hex_page(&source, file_offset, length)
                .map_err(|error| JobError::failed("preview.hex", error.to_string()))?;
            job_success(page)
        }),
        JobRequest::TextPreview {
            path,
            offset_bytes,
            metadata_address,
            file_offset,
            max_bytes,
        } => manager.submit("text_preview", move |_| {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let file = filesystem
                .open_file(metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let source = EvidenceByteSource { file };
            let preview = preview_text(&source, file_offset, max_bytes)
                .map_err(|error| JobError::failed("preview.text", error.to_string()))?;
            job_success(preview)
        }),
        JobRequest::FileSignature {
            path,
            offset_bytes,
            metadata_address,
        } => manager.submit("file_signature", move |_| {
            let filesystem = FileSystem::open(Path::new(&path), offset_bytes)
                .map_err(|error| JobError::failed("filesystem.open", error.to_string()))?;
            let file = filesystem
                .open_file(metadata_address)
                .map_err(|error| JobError::failed("filesystem.file", error.to_string()))?;
            let source = EvidenceByteSource { file };
            let signature = identify_signature(&source)
                .map_err(|error| JobError::failed("preview.signature", error.to_string()))?;
            job_success(signature)
        }),
        JobRequest::ExtractFile {
            path,
            offset_bytes,
            parent_inode,
            metadata_address,
            destination_root,
        } => manager.submit("extract_file", move |_| {
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
        }),
        JobRequest::ExtractDirectory {
            path,
            offset_bytes,
            inode,
            directory_name,
            destination_root,
            reports_dir,
            base_name,
        } => manager.submit("extract_directory", move |_| {
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
            let result = job_value(serde_json::json!({
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
        }),
    }
    .map_err(job_submit_error)?;
    success_bytes(json!({"job_id": job_id}))
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
    let store =
        CaseStore::open(Path::new(&request.db_path)).map_err(|error| operation_error(&error))?;
    let record = store
        .create_case(CreateCase {
            id: request.id,
            title: request.title,
            examiner: request.examiner,
        })
        .map_err(|error| operation_error(&error))?;
    success_bytes(record)
}

fn handle_evidence_add(request: &[u8]) -> Result<Vec<u8>, FfiError> {
    let request: EvidenceAddRequest = parse_request(request)?;
    let store =
        CaseStore::open(Path::new(&request.db_path)).map_err(|error| operation_error(&error))?;
    let record = store
        .add_evidence(AddEvidence {
            id: request.id,
            case_id: request.case_id,
            description: request.description,
            source_path: request.source_path,
            source: request.source,
        })
        .map_err(|error| operation_error(&error))?;
    success_bytes(record)
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
