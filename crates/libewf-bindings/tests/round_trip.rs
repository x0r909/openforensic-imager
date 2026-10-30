use libewf_bindings::{EwfCompression, EwfReader, EwfWriteOptions, EwfWriter, is_ewf};
use tempfile::tempdir;

fn pattern(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| (index.wrapping_mul(31) & 0xff) as u8)
        .collect()
}

fn options() -> EwfWriteOptions {
    EwfWriteOptions {
        bytes_per_sector: 512,
        sectors_per_chunk: 64,
        maximum_segment_size: 1024 * 1024,
        compression: EwfCompression::None,
    }
}

#[test]
fn writes_and_reads_e01_round_trip() {
    let directory = tempdir().unwrap();
    let target = directory.path().join("evidence");
    let source = pattern(8192);

    let mut writer = EwfWriter::create(&target, source.len() as u64, options()).unwrap();
    for chunk in source.chunks(700) {
        writer.write_all(chunk).unwrap();
    }
    let segments = writer.finalize().unwrap();

    assert!(!segments.is_empty());
    assert_eq!(
        segments[0]
            .extension()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase(),
        "e01"
    );
    assert!(is_ewf(&segments[0]).unwrap());

    let reader = EwfReader::open(&segments[0]).unwrap();
    assert_eq!(reader.media_size(), 8192);
    assert_eq!(reader.bytes_per_sector(), 512);

    let mut selected = [0_u8; 1024];
    let count = reader.read_at(2048, &mut selected).unwrap();
    assert_eq!(count, selected.len());
    assert_eq!(&selected, &source[2048..3072]);

    let verification = std::process::Command::new("ewfverify")
        .arg(&segments[0])
        .output()
        .unwrap();
    assert!(
        verification.status.success(),
        "{}",
        String::from_utf8_lossy(&verification.stderr)
    );
}

#[test]
fn corrupted_or_missing_e01_is_rejected() {
    let directory = tempdir().unwrap();
    let missing = directory.path().join("missing.E01");

    let error = EwfReader::open(&missing).unwrap_err();

    assert!(!error.message().is_empty());
}

#[test]
fn segmented_e01_reads_across_segment_boundaries() {
    let directory = tempdir().unwrap();
    let target = directory.path().join("segmented");
    let source = pattern(3 * 1024 * 1024 + 512);

    let mut writer = EwfWriter::create(&target, source.len() as u64, options()).unwrap();
    writer.write_all(&source).unwrap();
    let segments = writer.finalize().unwrap();
    assert!(segments.len() > 1);

    let reader = EwfReader::open(&segments[0]).unwrap();
    assert_eq!(reader.media_size(), source.len() as u64);
    let mut selected = [0_u8; 2048];
    let count = reader.read_at(1024 * 1024 - 512, &mut selected).unwrap();
    assert_eq!(count, selected.len());
    assert_eq!(&selected, &source[1024 * 1024 - 512..1024 * 1024 + 1536]);

    let verification = std::process::Command::new("ewfverify")
        .arg(&segments[0])
        .output()
        .unwrap();
    assert!(verification.status.success());
}

#[test]
fn non_ewf_path_fails_signature_check() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("plain.raw");
    std::fs::write(&path, b"not ewf").unwrap();

    assert!(!is_ewf(&path).unwrap());
}

#[test]
fn reads_stored_hashes_from_ewfacquire_image() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    let target = directory.path().join("acquired");
    std::fs::write(&source, pattern(8192)).unwrap();
    let output = std::process::Command::new("ewfacquire")
        .args([
            "-u",
            "-q",
            "-f",
            "encase6",
            "-d",
            "sha1",
            "-t",
            target.to_str().unwrap(),
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let reader = EwfReader::open(&directory.path().join("acquired.E01")).unwrap();
    assert_eq!(reader.stored_md5().unwrap().len(), 32);
    assert_eq!(reader.stored_sha1().unwrap().len(), 40);
}
