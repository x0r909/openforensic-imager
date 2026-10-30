use std::ffi::CStr;
use std::ptr;

use native_bindings::{
    ofi_abi_version, ofi_bytes_free, ofi_case_create, ofi_error_code, ofi_error_free,
    ofi_evidence_add, ofi_status, ofi_error,
};
use tempfile::tempdir;

unsafe fn error_code(error: *mut ofi_error) -> String {
    assert!(!error.is_null());
    let code = unsafe { CStr::from_ptr(ofi_error_code(error)) };
    let value = code.to_string_lossy().into_owned();
    unsafe { ofi_error_free(error) };
    value
}

#[test]
fn abi_version_is_one() {
    assert_eq!(ofi_abi_version(), 1);
}

#[test]
fn null_request_is_rejected_without_dereference() {
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            ptr::null(),
            0,
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_ARGUMENT);
    assert!(response.is_null());
    assert_eq!(unsafe { error_code(error) }, "invalid_argument");
}

#[test]
fn invalid_utf8_is_rejected() {
    let bytes = [0xff_u8];
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            bytes.as_ptr(),
            bytes.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_JSON);
    assert!(response.is_null());
    assert_eq!(unsafe { error_code(error) }, "invalid_json");
}

#[test]
fn oversized_request_length_is_rejected_before_slice_creation() {
    let bytes = b"{}";
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            bytes.as_ptr(),
            isize::MAX as usize,
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_INVALID_ARGUMENT);
    assert_eq!(unsafe { error_code(error) }, "invalid_argument");
}

#[test]
fn case_and_evidence_requests_return_owned_json() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("abi.db");
    let case_id = "00000000-0000-0000-0000-000000000001";
    let evidence_id = "00000000-0000-0000-0000-000000000002";
    let request = format!(
        r#"{{"db_path":"{}","id":"{case_id}","title":"ABI Case","examiner":"Examiner"}}"#,
        path.display()
    );
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let mut error = ptr::null_mut();

    let status = unsafe {
        ofi_case_create(
            request.as_ptr(),
            request.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    assert!(error.is_null());
    let response_bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let response_json: serde_json::Value = serde_json::from_slice(response_bytes).unwrap();
    assert_eq!(response_json["body"]["id"], case_id);
    unsafe { ofi_bytes_free(response, response_len) };

    let request = format!(
        r#"{{"db_path":"{}","id":"{evidence_id}","case_id":"{case_id}","description":"disk","source_path":"/tmp/disk.raw","source":{{"device":1,"inode":2,"length":4096,"mtime_nanoseconds":3,"ctime_nanoseconds":4,"read_only":true}}}}"#,
        path.display()
    );
    let mut response = ptr::null_mut();
    let mut response_len = 0;
    let status = unsafe {
        ofi_evidence_add(
            request.as_ptr(),
            request.len(),
            &mut response,
            &mut response_len,
            &mut error,
        )
    };

    assert_eq!(status, ofi_status::OFI_STATUS_OK);
    assert!(error.is_null());
    let response_bytes = unsafe { std::slice::from_raw_parts(response, response_len) };
    let response_json: serde_json::Value = serde_json::from_slice(response_bytes).unwrap();
    assert_eq!(response_json["body"]["id"], evidence_id);
    unsafe { ofi_bytes_free(response, response_len) };
}

