#![no_main]

use libfuzzer_sys::fuzz_target;
use native_bindings::validate_job_request;

fuzz_target!(|data: &[u8]| {
    let _ = validate_job_request(data);
});
