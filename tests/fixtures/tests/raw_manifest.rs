use std::path::Path;
use std::process::Command;

use forensic_fixtures::{FixtureManifest, generate_raw_fixture, RawLayout};
use tempfile::tempdir;

fn sha256sum(path: &Path) -> String {
    let output = Command::new("sha256sum").arg(path).output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

#[test]
fn empty_layout_creates_an_empty_segment() {
    let directory = tempdir().unwrap();

    let manifest =
        generate_raw_fixture(directory.path(), 1, RawLayout::Empty).unwrap();

    assert_eq!(manifest.byte_length, 0);
    assert_eq!(manifest.segments.len(), 1);
    assert_eq!(manifest.segments[0].length, 0);
    assert_eq!(
        sha256sum(&directory.path().join(&manifest.segments[0].path)),
        manifest.segments[0].sha256
    );
}

#[test]
fn dense_layout_is_deterministic_and_matches_sha256sum() {
    let first = tempdir().unwrap();
    let second = tempdir().unwrap();
    let layout = RawLayout::Dense {
        length: 4096,
        block_size: 512,
    };

    let first_manifest = generate_raw_fixture(first.path(), 42, layout.clone()).unwrap();
    let second_manifest = generate_raw_fixture(second.path(), 42, layout).unwrap();

    assert_eq!(first_manifest, second_manifest);
    assert_eq!(first_manifest.byte_length, 4096);
    assert_eq!(first_manifest.segments[0].length, 4096);
    assert_eq!(
        sha256sum(&first.path().join(&first_manifest.segments[0].path)),
        first_manifest.segments[0].sha256
    );
}

#[test]
fn sparse_layout_preserves_logical_length_and_hash() {
    let directory = tempdir().unwrap();

    let manifest = generate_raw_fixture(
        directory.path(),
        7,
        RawLayout::Sparse {
            length: 8192,
            block_size: 512,
            data_blocks: vec![0, 3, 15],
        },
    )
    .unwrap();

    assert_eq!(manifest.byte_length, 8192);
    assert_eq!(manifest.segments[0].length, 8192);
    assert_eq!(
        sha256sum(&directory.path().join(&manifest.segments[0].path)),
        manifest.segments[0].sha256
    );
}

#[test]
fn split_layout_hashes_every_segment() {
    let directory = tempdir().unwrap();

    let manifest = generate_raw_fixture(
        directory.path(),
        9,
        RawLayout::Split {
            segment_lengths: vec![100, 200],
            block_size: 64,
        },
    )
    .unwrap();

    assert_eq!(manifest.byte_length, 300);
    assert_eq!(manifest.segments.len(), 2);
    for segment in &manifest.segments {
        assert_eq!(
            sha256sum(&directory.path().join(&segment.path)),
            segment.sha256
        );
    }
}

#[test]
fn dense_layout_matches_committed_manifest_vector() {
    let directory = tempdir().unwrap();
    let manifest = generate_raw_fixture(
        directory.path(),
        42,
        RawLayout::Dense {
            length: 4096,
            block_size: 512,
        },
    )
    .unwrap();

    let expected: FixtureManifest = serde_json::from_str(include_str!(
        "../manifests/phase1-dense-seed42.json"
    ))
    .unwrap();
    assert_eq!(manifest, expected);
}

