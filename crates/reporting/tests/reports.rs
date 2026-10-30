use std::collections::BTreeMap;

use acquisition::{
    AcquisitionCompression, AcquisitionOptions, AcquisitionRequest, OutputFormat, acquire_path,
    verify_image,
};
use forensic_fixtures::{RawLayout, generate_raw_fixture};
use hashing::HashAlgorithm;
use reporting::write_report_set;
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
    let verification = verify_image(
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
