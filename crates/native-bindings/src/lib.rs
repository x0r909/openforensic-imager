use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;

use evidence_manager::{AddEvidence, CaseStore, CreateCase, SourceSnapshot};
use forensic_core::{CaseId, EvidenceId};
use serde::{Deserialize, Serialize};

const ABI_MAJOR: u32 = 1;
const ABI_MINOR: u32 = 0;
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

unsafe fn run_adapter(
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
    error: *mut *mut ofi_error,
    handler: fn(&[u8]) -> Result<Vec<u8>, FfiError>,
) -> ofi_status {
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
