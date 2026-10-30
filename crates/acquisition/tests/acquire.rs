use std::collections::BTreeMap;
use std::io::Read;
use std::process::Command;

use acquisition::{
    AcquisitionCompression, AcquisitionOptions, AcquisitionRequest, AcquisitionStatus,
    CancellationToken, OutputFormat, VerificationOutcome, acquire_from_reader, acquire_path,
    acquire_path_with_cancel, verify_image,
};
use forensic_fixtures::{RawLayout, generate_raw_fixture};
use hashing::{HashAlgorithm, StreamingHasher};
use image_reader::{ImageError, ImageMetadata, ImageReader, RawReader};
use tempfile::tempdir;

fn sha256_file(path: &std::path::Path) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    let mut buffer = [0_u8; 4096];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    hasher.finish()[&HashAlgorithm::Sha256].to_hex()
}

fn source_fixture(root: &std::path::Path) -> std::path::PathBuf {
    let manifest = generate_raw_fixture(
        root,
        77,
        RawLayout::Dense {
            length: 16384,
            block_size: 512,
        },
    )
    .unwrap();
    root.join(&manifest.segments[0].path)
}

#[test]
fn raw_acquisition_copies_source_and_writes_verification_manifest() {
    let directory = tempdir().unwrap();
    let source = source_fixture(&directory.path().join("fixtures"));
    let source_hash = sha256_file(&source);
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();

    let result = acquire_path(AcquisitionRequest {
        source_path: source.clone(),
        destination_dir: destination.clone(),
        base_name: "case-01".to_owned(),
        format: OutputFormat::Raw,
        options: AcquisitionOptions {
            block_size: 4096,
            maximum_segment_size: 1024 * 1024,
            compression: AcquisitionCompression::None,
            verify_after: true,
        },
    })
    .unwrap();

    assert_eq!(result.status, AcquisitionStatus::Succeeded);
    assert_eq!(source_hash, sha256_file(&source));
    assert_eq!(result.manifest.artifacts.len(), 1);
    let artifact = destination.join(&result.manifest.artifacts[0].path);
    assert_eq!(sha256_file(&artifact), source_hash);

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.manifest_path).unwrap()).unwrap();
    assert_eq!(report["status"], "succeeded");
    assert_eq!(report["hashes"][0]["scope"], "output_logical");
}

#[test]
fn e01_acquisition_is_verified_by_libewf_tooling() {
    let directory = tempdir().unwrap();
    let source = source_fixture(&directory.path().join("fixtures"));
    let source_hash = sha256_file(&source);
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();

    let result = acquire_path(AcquisitionRequest {
        source_path: source.clone(),
        destination_dir: destination.clone(),
        base_name: "case-e01".to_owned(),
        format: OutputFormat::Ewf1E01,
        options: AcquisitionOptions {
            block_size: 4096,
            maximum_segment_size: 1024 * 1024,
            compression: AcquisitionCompression::None,
            verify_after: true,
        },
    })
    .unwrap();

    assert_eq!(result.status, AcquisitionStatus::Succeeded);
    assert_eq!(sha256_file(&source), source_hash);
    let e01 = destination.join(&result.manifest.artifacts[0].path);
    assert_eq!(
        e01.extension()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase(),
        "e01"
    );
    let output = std::process::Command::new("ewfverify")
        .arg(&e01)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let hashes: BTreeMap<_, _> = result
        .manifest
        .hashes
        .iter()
        .map(|hash| (hash.algorithm, hash.digest_hex.as_str()))
        .collect();
    assert!(hashes.contains_key(&HashAlgorithm::Sha256));
    assert_eq!(hashes[&HashAlgorithm::Sha256], source_hash);
}

#[test]
fn cancelled_acquisition_is_not_committed() {
    let directory = tempdir().unwrap();
    let source = source_fixture(&directory.path().join("fixtures"));
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();
    let cancellation = CancellationToken::default();
    cancellation.cancel();

    let result = acquire_path_with_cancel(
        AcquisitionRequest {
            source_path: source,
            destination_dir: destination.clone(),
            base_name: "cancelled".to_owned(),
            format: OutputFormat::Raw,
            options: AcquisitionOptions {
                block_size: 4096,
                maximum_segment_size: 1024 * 1024,
                compression: AcquisitionCompression::None,
                verify_after: true,
            },
        },
        &cancellation,
    )
    .unwrap();

    assert_eq!(result.status, AcquisitionStatus::Cancelled);
    assert!(!destination.join("cancelled.raw").exists());
    assert!(result.manifest_path.starts_with(destination));
}

#[test]
fn read_error_is_zero_filled_and_marked_partial() {
    let directory = tempdir().unwrap();
    let source = source_fixture(&directory.path().join("fixtures"));
    let reader = RawReader::open_read_only(&source).unwrap();
    let faulty = FaultyReader {
        inner: reader,
        fail_offset: 4096,
    };
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();

    let result = acquire_from_reader(
        &faulty,
        AcquisitionRequest {
            source_path: source,
            destination_dir: destination.clone(),
            base_name: "partial".to_owned(),
            format: OutputFormat::Raw,
            options: AcquisitionOptions {
                block_size: 4096,
                maximum_segment_size: 1024 * 1024,
                compression: AcquisitionCompression::None,
                verify_after: true,
            },
        },
        &CancellationToken::default(),
    )
    .unwrap();

    assert_eq!(result.status, AcquisitionStatus::Partial);
    assert_eq!(result.manifest.unreadable_ranges.len(), 1);
    assert_eq!(result.manifest.unreadable_ranges[0].offset, 4096);
    assert!(!destination.join("partial.raw").exists());
}

#[test]
fn verification_distinguishes_match_mismatch_and_missing() {
    let directory = tempdir().unwrap();
    let source = source_fixture(&directory.path().join("fixtures"));
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();
    let result = acquire_path(AcquisitionRequest {
        source_path: source,
        destination_dir: destination.clone(),
        base_name: "verify".to_owned(),
        format: OutputFormat::Raw,
        options: AcquisitionOptions {
            block_size: 4096,
            maximum_segment_size: 1024 * 1024,
            compression: AcquisitionCompression::None,
            verify_after: true,
        },
    })
    .unwrap();
    let artifact = destination.join(&result.manifest.artifacts[0].path);
    let expected = BTreeMap::from([(
        HashAlgorithm::Sha256,
        result.manifest.artifacts[0].sha256.clone(),
    )]);

    let matched = verify_image(&artifact, &[HashAlgorithm::Sha256], &expected).unwrap();
    assert_eq!(
        matched.outcomes[&HashAlgorithm::Sha256],
        VerificationOutcome::Match
    );

    let wrong = BTreeMap::from([(HashAlgorithm::Sha256, "00".repeat(32))]);
    let mismatched = verify_image(&artifact, &[HashAlgorithm::Sha256], &wrong).unwrap();
    assert_eq!(
        mismatched.outcomes[&HashAlgorithm::Sha256],
        VerificationOutcome::Mismatch
    );

    let absent = verify_image(&artifact, &[HashAlgorithm::Sha256], &BTreeMap::new()).unwrap();
    assert_eq!(
        absent.outcomes[&HashAlgorithm::Sha256],
        VerificationOutcome::NotAvailable
    );
}

#[test]
fn acquisition_never_opens_the_source_for_write() {
    if Command::new("strace").arg("-V").output().is_err() {
        eprintln!("strace is unavailable; source-open syscall trace skipped");
        return;
    }
    let directory = tempdir().unwrap();
    let source = source_fixture(&directory.path().join("fixtures"));
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();
    let trace = directory.path().join("source-open.trace");

    let status = Command::new("strace")
        .args(["-f", "-qq", "-e", "trace=open,openat,openat2", "-o"])
        .arg(&trace)
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "source_write_syscall_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("OFI_SOURCE_WRITE_CHILD", "1")
        .env("OFI_SOURCE_PATH", &source)
        .env("OFI_DESTINATION_PATH", &destination)
        .status()
        .unwrap();

    assert!(status.success(), "traced acquisition child failed");
    let trace = std::fs::read_to_string(trace).unwrap();
    let source_path = source.to_string_lossy();
    let source_opens = trace
        .lines()
        .filter(|line| line.contains(source_path.as_ref()))
        .collect::<Vec<_>>();
    assert!(
        !source_opens.is_empty(),
        "the trace did not contain the source open"
    );
    for line in source_opens {
        assert!(line.contains("O_RDONLY"), "{line}");
        assert!(!line.contains("O_WRONLY"), "{line}");
        assert!(!line.contains("O_RDWR"), "{line}");
    }
}

#[test]
fn source_write_syscall_child() {
    if std::env::var_os("OFI_SOURCE_WRITE_CHILD").is_none() {
        return;
    }
    let source = std::path::PathBuf::from(std::env::var_os("OFI_SOURCE_PATH").unwrap());
    let destination = std::path::PathBuf::from(std::env::var_os("OFI_DESTINATION_PATH").unwrap());
    let result = acquire_path(AcquisitionRequest {
        source_path: source,
        destination_dir: destination,
        base_name: "syscall-trace".to_owned(),
        format: OutputFormat::Raw,
        options: AcquisitionOptions {
            block_size: 4096,
            maximum_segment_size: 1024 * 1024,
            compression: AcquisitionCompression::None,
            verify_after: true,
        },
    })
    .unwrap();
    assert_eq!(result.status, AcquisitionStatus::Succeeded);
}

struct FaultyReader {
    inner: RawReader,
    fail_offset: u64,
}

impl ImageReader for FaultyReader {
    fn logical_length(&self) -> u64 {
        self.inner.logical_length()
    }

    fn sector_size(&self) -> u32 {
        self.inner.sector_size()
    }

    fn read_exact_at(
        &self,
        range: forensic_core::ByteRange,
        out: &mut [u8],
    ) -> Result<(), ImageError> {
        if range.offset() == self.fail_offset {
            return Err(ImageError::backend("injected read failure"));
        }
        self.inner.read_exact_at(range, out)
    }

    fn metadata(&self) -> ImageMetadata {
        self.inner.metadata()
    }
}
