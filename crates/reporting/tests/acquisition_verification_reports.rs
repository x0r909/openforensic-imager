use std::collections::BTreeMap;

use acquisition::{
    AcquisitionCompression, AcquisitionOptions, AcquisitionRequest, OutputFormat, acquire_path,
    verify_image_report,
};
use forensic_fixtures::{RawLayout, generate_raw_fixture};
use hashing::HashAlgorithm;
use reporting::{write_acquisition_report_set, write_verification_report_set};
use tempfile::tempdir;

#[test]
fn writes_separate_acquisition_and_verification_reports() {
    let directory = tempdir().unwrap();
    let fixtures = directory.path().join("fixtures");
    let fixture = generate_raw_fixture(
        &fixtures,
        44,
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
        base_name: "phase5".to_owned(),
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

    let acquisition_paths =
        write_acquisition_report_set(&destination, "phase5", &result.manifest).unwrap();
    let verification_paths =
        write_verification_report_set(&destination, "phase5", &verification).unwrap();

    assert!(acquisition_paths.json.ends_with("phase5.acquisition.json"));
    assert!(acquisition_paths.csv.ends_with("phase5.acquisition.csv"));
    assert!(acquisition_paths.html.ends_with("phase5.acquisition.html"));
    assert!(
        verification_paths
            .json
            .ends_with("phase5.verification.json")
    );
    assert!(verification_paths.csv.ends_with("phase5.verification.csv"));
    assert!(
        verification_paths
            .html
            .ends_with("phase5.verification.html")
    );
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&verification_paths.json).unwrap()).unwrap();
    assert_eq!(json["verification"]["outcomes"]["sha256"], "match");
}
