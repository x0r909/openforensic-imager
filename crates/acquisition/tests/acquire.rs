use std::collections::BTreeMap;
use std::io::Read;
use std::process::Command;
use std::time::Duration;

use acquisition::{
    AcquisitionCompression, AcquisitionErrorCode, AcquisitionOptions, AcquisitionRequest,
    AcquisitionStatus, CancellationToken, OutputFormat, VerificationOutcome, acquire_from_reader,
    acquire_path, acquire_path_with_cancel, reap_stale_staging, verify_image, verify_image_report,
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
    let mismatch = verify_image(&artifact, &[HashAlgorithm::Sha256], &wrong).unwrap_err();
    assert_eq!(
        mismatch.code(),
        AcquisitionErrorCode::VerificationNotPerformed,
        "a mismatch must not be reported as a completed verification"
    );
    assert_eq!(
        verify_image_report(&artifact, &[HashAlgorithm::Sha256], &wrong).unwrap().outcomes
            [&HashAlgorithm::Sha256],
        VerificationOutcome::Mismatch
    );

    // No expected digest means nothing was compared, which is never success.
    let absent = verify_image(&artifact, &[HashAlgorithm::Sha256], &BTreeMap::new()).unwrap_err();
    assert_eq!(
        absent.code(),
        AcquisitionErrorCode::VerificationNotPerformed,
        "an unperformed comparison must not be reported as a completed verification"
    );
    assert_eq!(
        verify_image_report(&artifact, &[HashAlgorithm::Sha256], &BTreeMap::new())
            .unwrap()
            .outcomes[&HashAlgorithm::Sha256],
        VerificationOutcome::NotAvailable
    );

    // One matching algorithm does not rescue an algorithm that was not compared.
    let partial = BTreeMap::from([(
        HashAlgorithm::Sha256,
        result.manifest.artifacts[0].sha256.clone(),
    )]);
    assert!(
        verify_image(
            &artifact,
            &[HashAlgorithm::Sha256, HashAlgorithm::Sha1],
            &partial
        )
        .is_err(),
        "an uncompared requested algorithm must fail the verification"
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

/// Creates a staging-shaped directory `.<base>.partial-<uuid>` under `parent`
/// with an optional marker file, and ages it so the reaper's mtime check has
/// something to act on.
fn make_staging(
    parent: &std::path::Path,
    base: &str,
    marker: Option<&str>,
    age_seconds: u64,
) -> std::path::PathBuf {
    let staging = parent.join(format!(".{base}.partial-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).unwrap();
    if let Some(contents) = marker {
        std::fs::write(staging.join(".ofi-staging.json"), contents).unwrap();
    }
    let age = std::time::SystemTime::now() - std::time::Duration::from_secs(age_seconds);
    set_mtime(&staging, age);
    staging
}

fn set_mtime(path: &std::path::Path, when: std::time::SystemTime) {
    let file = std::fs::File::open(path).unwrap();
    let times = std::fs::FileTimes::new().set_modified(when);
    file.set_times(times).unwrap();
}

fn marker_json(job_id: &str, pid: u32, started_at_unix_ms: u64) -> String {
    format!(
        "{{\"job_id\":\"{job_id}\",\"pid\":{pid},\"started_at_unix_ms\":{started_at_unix_ms}}}"
    )
}

#[test]
fn live_staging_directory_is_kept() {
    let directory = tempdir().unwrap();
    let staging = make_staging(
        directory.path(),
        "live",
        Some(&marker_json("job-live", std::process::id(), 0)),
        48 * 60 * 60,
    );

    let report =
        reap_stale_staging(directory.path(), Duration::from_secs(24 * 60 * 60), &|_| true).unwrap();

    assert!(report.removed.is_empty(), "{:?}", report.removed);
    assert!(staging.exists(), "a live acquisition's staging was removed");
    assert!(report.kept.contains(&staging));
}

#[test]
fn stale_staging_directory_is_reaped() {
    let directory = tempdir().unwrap();
    // pid 0 is never a live process, so both the liveness check and the age
    // check agree that this directory is orphaned.
    let staging = make_staging(
        directory.path(),
        "stale",
        Some(&marker_json("job-stale", 0, 0)),
        48 * 60 * 60,
    );

    let report =
        reap_stale_staging(directory.path(), Duration::from_secs(24 * 60 * 60), &|_| false).unwrap();

    assert!(!staging.exists(), "a stale staging directory survived");
    assert!(report.removed.contains(&staging));
}

#[test]
fn unmarked_directory_without_the_staging_prefix_is_never_reaped() {
    let directory = tempdir().unwrap();
    let foreign = directory.path().join("examiner-notes");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("case.txt"), b"evidence").unwrap();
    // The prefix alone must not be enough: a directory that is old and matches
    // the staging shape but carries no marker is not ours to delete.
    let lookalike = directory.path().join(".notes.partial-not-a-uuid");
    std::fs::create_dir(&lookalike).unwrap();
    set_mtime(
        &foreign,
        std::time::SystemTime::now() - Duration::from_secs(72 * 60 * 60),
    );
    set_mtime(
        &lookalike,
        std::time::SystemTime::now() - Duration::from_secs(72 * 60 * 60),
    );

    let report =
        reap_stale_staging(directory.path(), Duration::from_secs(24 * 60 * 60), &|_| false).unwrap();

    assert!(report.removed.is_empty(), "{:?}", report.removed);
    assert!(foreign.join("case.txt").exists());
    assert!(lookalike.exists(), "a non-uuid lookalike was removed");
}

#[test]
fn fresh_cancelled_staging_survives_the_reaper() {
    let directory = tempdir().unwrap();
    // ACQ-011: a cancelled acquisition reports its retained staging directory.
    // The reaper must not delete that output while the examiner can still use it.
    let staging = make_staging(
        directory.path(),
        "cancelled",
        Some(&marker_json("job-cancelled", 0, 0)),
        60,
    );

    let report =
        reap_stale_staging(directory.path(), Duration::from_secs(24 * 60 * 60), &|_| false).unwrap();

    assert!(report.removed.is_empty(), "{:?}", report.removed);
    assert!(staging.exists(), "a fresh cancelled staging was reaped");
}

#[test]
fn unreadable_marker_is_reaped_only_once_the_directory_is_old() {
    let directory = tempdir().unwrap();
    let fresh = make_staging(directory.path(), "fresh-garbage", Some("not json"), 60);
    let old = make_staging(directory.path(), "old-garbage", Some("not json"), 48 * 60 * 60);

    let report =
        reap_stale_staging(directory.path(), Duration::from_secs(24 * 60 * 60), &|_| true).unwrap();

    assert!(fresh.exists(), "a fresh unreadable marker was reaped");
    assert!(!old.exists(), "an old unreadable marker survived");
    assert!(report.removed.contains(&old));
}

#[test]
fn acquisition_reaps_a_stale_sibling_and_records_the_count() {
    let root = tempdir().unwrap();
    let destination = root.path().join("out");
    std::fs::create_dir(&destination).unwrap();
    let source = source_fixture(root.path());
    let orphan = make_staging(
        &destination,
        "prior",
        Some(&marker_json("job-prior", 0, 0)),
        48 * 60 * 60,
    );

    let result = acquire_path(AcquisitionRequest {
        source_path: source,
        destination_dir: destination.clone(),
        base_name: "reaped".to_owned(),
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
    assert!(!orphan.exists(), "a stale sibling staging survived acquisition");
    assert_eq!(result.manifest.staging_reaped, Some(1));
    assert!(result.manifest.staging_reap_warning.is_none());
}

#[test]
fn successful_acquisition_leaves_no_staging_directory_behind() {
    let root = tempdir().unwrap();
    let destination = root.path().join("out");
    std::fs::create_dir(&destination).unwrap();
    let source = source_fixture(root.path());

    let result = acquire_path(AcquisitionRequest {
        source_path: source,
        destination_dir: destination.clone(),
        base_name: "clean".to_owned(),
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
    // The marker file makes staging non-empty, so a promotion that removes the
    // directory without removing the marker first leaks the directory silently.
    let leftovers = std::fs::read_dir(&destination)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".partial-"))
        .collect::<Vec<_>>();
    assert!(leftovers.is_empty(), "staging leaked: {leftovers:?}");
}

#[test]
fn cancelled_acquisition_still_reports_its_retained_staging_directory() {
    let root = tempdir().unwrap();
    let destination = root.path().join("out");
    std::fs::create_dir(&destination).unwrap();
    let source = source_fixture(root.path());

    // Cancelled before the first block, the way a SIGKILL arrives.
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
    // ACQ-011: the retained staging directory stays discoverable, and it now
    // carries the marker the reaper needs to find it later.
    let retained = result.manifest_path.parent().unwrap();
    assert!(retained.to_string_lossy().contains(".partial-"));
    assert!(retained.join(".ofi-staging.json").exists());
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
