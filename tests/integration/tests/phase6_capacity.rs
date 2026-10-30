use std::process::Command;

use forensic_core::ByteRange;
use image_reader::{ImageReader, RawReader};
use tempfile::tempdir;

const IMAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const PAGE_BYTES: usize = 64 * 1024;
const READ_COUNT: u64 = 1024;

#[test]
fn sparse_multi_gigabyte_image_reads_with_bounded_memory() {
    if Command::new("/usr/bin/time")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("/usr/bin/time is unavailable; bounded-memory capacity check skipped");
        return;
    }
    let directory = tempdir().unwrap();
    let image = directory.path().join("sparse.raw");
    let file = std::fs::File::create(&image).unwrap();
    file.set_len(IMAGE_BYTES).unwrap();

    let output = Command::new("/usr/bin/time")
        .args(["-v", "--"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "capacity_sparse_read_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OFI_CAPACITY_CHILD", "1")
        .env("OFI_CAPACITY_IMAGE", &image)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let maximum_rss_kib = stderr
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Maximum resident set size (kbytes):")
                .and_then(|value| value.trim().parse::<u64>().ok())
        })
        .expect("missing maximum resident set size in time output");
    assert!(
        maximum_rss_kib < 128 * 1024,
        "peak RSS was {maximum_rss_kib} KiB"
    );
}

#[test]
fn capacity_sparse_read_child() {
    if std::env::var_os("OFI_CAPACITY_CHILD").is_none() {
        return;
    }
    let image = std::path::PathBuf::from(std::env::var_os("OFI_CAPACITY_IMAGE").unwrap());
    let reader = RawReader::open_read_only(&image).unwrap();
    assert_eq!(reader.logical_length(), IMAGE_BYTES);
    let mut page = vec![0_u8; PAGE_BYTES];
    let maximum_offset = IMAGE_BYTES - PAGE_BYTES as u64;
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;

    for _ in 0..READ_COUNT {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let offset = state % maximum_offset;
        let range = ByteRange::new(offset, PAGE_BYTES as u64, IMAGE_BYTES).unwrap();
        reader.read_exact_at(range, &mut page).unwrap();
        assert!(page.iter().all(|byte| *byte == 0));
    }
}
