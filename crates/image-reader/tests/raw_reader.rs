use forensic_core::ByteRange;
use image_reader::{ImageErrorCode, ImageFormat, ImageReader, RawReader};
use tempfile::tempdir;

#[test]
fn missing_file_returns_not_found() {
    let directory = tempdir().unwrap();
    let error = RawReader::open_read_only(&directory.path().join("missing.raw")).unwrap_err();

    assert_eq!(error.code(), ImageErrorCode::NotFound);
}

#[test]
fn empty_file_opens_with_zero_length() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("empty.raw");
    std::fs::write(&path, []).unwrap();

    let reader = RawReader::open_read_only(&path).unwrap();

    assert_eq!(reader.logical_length(), 0);
    assert_eq!(reader.sector_size(), 512);
    assert_eq!(reader.metadata().format, ImageFormat::Raw);
}

#[test]
fn full_valid_read_returns_exact_bytes_and_read_only_fingerprint() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("valid.raw");
    std::fs::write(&path, b"forensic").unwrap();

    let reader = RawReader::open_read_only(&path).unwrap();
    let range = ByteRange::new(0, 8, reader.logical_length()).unwrap();
    let mut output = [0_u8; 8];
    reader.read_exact_at(range, &mut output).unwrap();

    assert_eq!(&output, b"forensic");
    assert!(reader.metadata().source.read_only());
    assert_eq!(reader.metadata().logical_length, 8);
}

#[test]
fn range_past_image_is_rejected() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("short.raw");
    std::fs::write(&path, b"data").unwrap();
    let reader = RawReader::open_read_only(&path).unwrap();
    let range = ByteRange::new(4, 2, 6).unwrap();
    let mut output = [0_u8; 2];

    let error = reader.read_exact_at(range, &mut output).unwrap_err();

    assert_eq!(error.code(), ImageErrorCode::OutOfRange);
}

#[test]
fn symbolic_link_source_is_refused() {
    let directory = tempdir().unwrap();
    let target = directory.path().join("target.raw");
    let link = directory.path().join("link.raw");
    std::fs::write(&target, b"data").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = RawReader::open_read_only(&link).unwrap_err();

    assert_eq!(error.code(), ImageErrorCode::SymbolicLink);
}

#[test]
fn file_change_after_open_is_reported() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("changing.raw");
    std::fs::write(&path, b"data").unwrap();
    let reader = RawReader::open_read_only(&path).unwrap();

    let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut file, b"!").unwrap();
    file.sync_all().unwrap();

    let range = ByteRange::new(0, 4, 4).unwrap();
    let mut output = [0_u8; 4];
    let error = reader.read_exact_at(range, &mut output).unwrap_err();

    assert_eq!(error.code(), ImageErrorCode::ConcurrentModification);
}

