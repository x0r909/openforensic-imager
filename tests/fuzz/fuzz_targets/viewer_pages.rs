#![no_main]

use libfuzzer_sys::fuzz_target;
use viewer::{MAX_PAGE_SIZE, hex_page, identify_signature, preview_text};

fuzz_target!(|data: &[u8]| {
    let length = (data.len() % MAX_PAGE_SIZE).max(1);
    let offset = data.first().copied().unwrap_or(0) as u64;
    let _ = hex_page(data, offset, length);
    let _ = preview_text(data, 0, length);
    let _ = identify_signature(data);
});
