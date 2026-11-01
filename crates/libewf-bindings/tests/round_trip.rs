use libewf_bindings::{
    EwfCompression, EwfReader, EwfWriteOptions, EwfWriter, is_ewf, segment_paths,
};
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

/// The worker grants each EWF segment individually instead of the whole
/// directory, so this must name every sibling the image was split across —
/// and only those.
#[test]
fn segment_paths_lists_only_existing_siblings() {
    let directory = tempdir().unwrap();
    std::fs::write(directory.path().join("image.E01"), b"first").unwrap();
    std::fs::write(directory.path().join("image.E02"), b"second").unwrap();
    // E03 exists under a differently-cased extension and must still be found.
    std::fs::write(directory.path().join("image.e03"), b"third").unwrap();
    // Unrelated neighbours the grant must not cover.
    std::fs::write(directory.path().join("other.E01"), b"other").unwrap();
    std::fs::write(directory.path().join("image.raw"), b"raw").unwrap();
    std::fs::create_dir(directory.path().join("image.E04")).unwrap();

    let segments = segment_paths(&directory.path().join("image.E01"));

    let names: Vec<String> = segments
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["image.E01", "image.E02", "image.e03"]);
}

/// `segment_paths` feeds a Landlock read grant whose entries are canonicalised,
/// so a symlink named like a segment must not put its target in the grant.
#[test]
fn segment_paths_ignores_symlinks_that_look_like_segments() {
    let directory = tempdir().unwrap();
    std::fs::write(directory.path().join("image.E01"), b"first").unwrap();
    let outside = tempdir().unwrap();
    let target = outside.path().join("elsewhere");
    std::fs::write(&target, b"outside").unwrap();
    std::os::unix::fs::symlink(&target, directory.path().join("image.E02")).unwrap();

    let segments = segment_paths(&directory.path().join("image.E01"));

    let names: Vec<String> = segments
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["image.E01"]);
}

#[test]
fn segment_paths_keeps_the_given_segment_when_it_is_the_only_one() {
    let directory = tempdir().unwrap();
    let first = directory.path().join("single.E01");
    std::fs::write(&first, b"only").unwrap();

    assert_eq!(segment_paths(&first), vec![first]);
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
