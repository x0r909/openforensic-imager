use std::collections::BTreeMap;

use acquisition::{
    AcquisitionCompression, AcquisitionOptions, AcquisitionRequest, OutputFormat, acquire_path,
    verify_image_report,
};
use forensic_fixtures::{RawLayout, generate_raw_fixture};
use hashing::HashAlgorithm;
use reporting::{write_report_set, write_verification_report_set};
use tempfile::tempdir;

#[test]
fn writes_json_csv_and_html_reports_with_safe_escaping() {
    let directory = tempdir().unwrap();
    let fixtures = directory.path().join("fixtures");
    let fixture = generate_raw_fixture(
        &fixtures,
        21,
        RawLayout::Dense {
            length: 4096,
            block_size: 512,
        },
    )
    .unwrap();
    let source = fixtures.join(&fixture.segments[0].path);
    let destination = directory.path().join("evidence");
    std::fs::create_dir(&destination).unwrap();
    let result = acquire_path(AcquisitionRequest {
        source_path: source,
        destination_dir: destination.clone(),
        base_name: "report".to_owned(),
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
    let verification = verify_image_report(
        &artifact,
        &[HashAlgorithm::Sha256],
        &BTreeMap::from([(
            HashAlgorithm::Sha256,
            result.manifest.artifacts[0].sha256.clone(),
        )]),
    )
    .unwrap();
    let mut manifest = result.manifest;
    manifest.source_path = "=cmd|' /C calc'!A0<script>".to_owned();

    let reports = write_report_set(&destination, "case-report", &manifest, &verification).unwrap();

    for path in [&reports.json, &reports.csv, &reports.html] {
        assert!(path.is_file());
        assert!(std::fs::metadata(path).unwrap().len() > 0);
        assert_eq!(path.parent().unwrap(), destination);
    }
    let html = std::fs::read_to_string(&reports.html).unwrap();
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    let csv = std::fs::read_to_string(&reports.csv).unwrap();
    assert!(csv.contains("\"'=cmd|' /C calc'!A0<script>\""));
}

#[test]
fn report_write_refuses_a_planted_symlink_leaf() {
    use std::path::PathBuf;

    let directory = tempdir().unwrap();
    let reports = directory.path().join("reports");
    std::fs::create_dir(&reports).unwrap();
    let outside = directory.path().join("victim.txt");
    std::fs::write(&outside, b"original examiner data").unwrap();

    let verification = acquisition::VerificationReport {
        path: PathBuf::from("/evidence/image.raw"),
        format: image_reader::ImageFormat::Raw,
        logical_length: 4096,
        computed: BTreeMap::new(),
        expected: BTreeMap::new(),
        outcomes: BTreeMap::new(),
    };

    // A lower-trust principal plants the verification JSON leaf pointing outside
    // the reports tree. That leaf is written first, so it is the one to block.
    std::os::unix::fs::symlink(&outside, reports.join("case.verification.json")).unwrap();

    let error = write_verification_report_set(&reports, "case", &verification).unwrap_err();
    assert!(
        !error.to_string().is_empty(),
        "a planted symlink leaf must not be written through"
    );

    // The redirect target must be untouched.
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "original examiner data"
    );
}

#[test]
fn report_write_refuses_a_symlinked_output_directory() {
    use std::path::PathBuf;

    let directory = tempdir().unwrap();
    let real = directory.path().join("real-reports");
    std::fs::create_dir(&real).unwrap();
    let link = directory.path().join("linked-reports");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let verification = acquisition::VerificationReport {
        path: PathBuf::from("/evidence/image.raw"),
        format: image_reader::ImageFormat::Raw,
        logical_length: 4096,
        computed: BTreeMap::new(),
        expected: BTreeMap::new(),
        outcomes: BTreeMap::new(),
    };

    let error = write_verification_report_set(&link, "case", &verification).unwrap_err();
    assert!(
        error.to_string().contains("symbolic link"),
        "unexpected error: {error}"
    );
    // Nothing may be written through the link.
    assert!(!real.join("case.verification.json").exists());
}
