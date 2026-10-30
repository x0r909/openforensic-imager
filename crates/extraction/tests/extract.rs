use std::os::unix::fs::PermissionsExt;
use std::os::unix::fs::symlink;
use std::process::Command;

use extraction::{
    ConflictPolicy, ExtractionManifest, ExtractionOptions, Extractor, ManifestStatus,
};
use filesystem::{FileEntry, FileSystem};
use hashing::{HashAlgorithm, StreamingHasher};
use tempfile::tempdir;

const CONTENT: &[u8] = b"forensic-content";

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ext4_with_files(files: &[(&str, &[u8])]) -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempdir().unwrap();
    let image = directory.path().join("evidence.raw");
    let file = std::fs::File::create(&image).unwrap();
    file.set_len(64 * 1024 * 1024).unwrap();
    run(Command::new("mke2fs")
        .args(["-q", "-F", "-t", "ext4", "-L", "EXTRACT"])
        .arg(&image));
    for (index, (name, content)) in files.iter().enumerate() {
        let source = directory.path().join(format!("source-{index}"));
        std::fs::write(&source, content).unwrap();
        run(Command::new("debugfs")
            .args(["-w", "-R"])
            .arg(format!("write {} /{name}", source.display()))
            .arg(&image));
    }
    (directory, image)
}

fn root_entries(filesystem: &FileSystem) -> Vec<FileEntry> {
    filesystem.list_directory(filesystem.root_inode()).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    hasher.update(bytes);
    hasher.finish()[&HashAlgorithm::Sha256].to_hex()
}

#[test]
fn extracts_file_with_metadata_hash_and_unchanged_source() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let before = std::fs::read(&image).unwrap();
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();

    let record = extractor.extract_file(&filesystem, &entry).unwrap();

    assert_eq!(std::fs::read(output.join("hello.txt")).unwrap(), CONTENT);
    assert_eq!(record.original_name, "hello.txt");
    assert_eq!(record.relative_path, "hello.txt");
    assert_eq!(record.entry.metadata_address, entry.metadata_address);
    assert_eq!(record.entry.size, CONTENT.len() as u64);
    assert_eq!(record.source_sha256, sha256(CONTENT));
    assert_eq!(record.output_sha256, record.source_sha256);
    assert_eq!(std::fs::read(&image).unwrap(), before);
    assert_eq!(
        std::fs::metadata(output.join("hello.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn refuses_to_overwrite_an_existing_export() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();
    extractor.extract_file(&filesystem, &entry).unwrap();
    std::fs::write(output.join("hello.txt"), b"do-not-change").unwrap();

    let error = extractor.extract_file(&filesystem, &entry).unwrap_err();

    assert!(error.message().contains("exists"));
    assert_eq!(
        std::fs::read(output.join("hello.txt")).unwrap(),
        b"do-not-change"
    );
}

#[test]
fn renames_conflicts_when_explicitly_configured() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let options = ExtractionOptions {
        conflict: ConflictPolicy::Rename,
        ..ExtractionOptions::default()
    };
    let mut extractor = Extractor::create(&output, options).unwrap();
    extractor.extract_file(&filesystem, &entry).unwrap();

    let record = extractor.extract_file(&filesystem, &entry).unwrap();

    assert_eq!(record.relative_path, "hello (1).txt");
    assert_eq!(
        std::fs::read(output.join("hello (1).txt")).unwrap(),
        CONTENT
    );
}

#[test]
fn rejects_traversal_and_absolute_names_before_writing() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let mut entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();

    for name in ["../escape.txt", "/absolute.txt", ".", ".."] {
        entry.name = name.to_owned();
        let error = extractor.extract_file(&filesystem, &entry).unwrap_err();
        assert!(error.message().contains("name"), "{name}: {error}");
    }
    assert!(!fixture.path().join("escape.txt").exists());
    assert!(!output.join("absolute.txt").exists());
}

#[test]
fn does_not_follow_a_destination_symlink() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let outside = fixture.path().join("outside.txt");
    std::fs::write(&outside, b"outside").unwrap();
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();
    symlink(&outside, output.join("hello.txt")).unwrap();

    let error = extractor.extract_file(&filesystem, &entry).unwrap_err();

    assert!(error.message().contains("exists"));
    assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
}

#[test]
fn enforces_the_configured_file_limit_before_creating_output() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let options = ExtractionOptions {
        max_file_bytes: 4,
        ..ExtractionOptions::default()
    };
    let mut extractor = Extractor::create(&output, options).unwrap();

    let error = extractor.extract_file(&filesystem, &entry).unwrap_err();

    assert!(error.message().contains("limit"));
    assert!(!output.join("hello.txt").exists());
}

#[test]
fn exports_unicode_names_and_writes_a_json_manifest() {
    let (fixture, image) = ext4_with_files(&[("halo-世界.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let output = fixture.path().join("output");
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();

    let manifest = extractor
        .extract_directory(&filesystem, filesystem.root_inode(), "files")
        .unwrap();

    assert_eq!(manifest.status, ManifestStatus::Complete);
    assert_eq!(manifest.records.len(), 1);
    assert_eq!(manifest.records[0].relative_path, "files/halo-世界.txt");
    assert!(
        output.join("files/halo-世界.txt").exists(),
        "unicode export missing"
    );
    let json = manifest.to_json_pretty().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["status"], "complete");
    assert_eq!(parsed["records"][0]["relative_path"], "files/halo-世界.txt");
    assert_eq!(parsed["records"][0]["output_sha256"], sha256(CONTENT));
}

#[test]
fn manifest_status_is_partial_when_a_directory_entry_fails() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let output = fixture.path().join("output");
    let options = ExtractionOptions {
        max_file_bytes: 4,
        ..ExtractionOptions::default()
    };
    let mut extractor = Extractor::create(&output, options).unwrap();

    let manifest: ExtractionManifest = extractor
        .extract_directory(&filesystem, filesystem.root_inode(), "files")
        .unwrap();

    assert_eq!(manifest.status, ManifestStatus::Partial);
    assert_eq!(manifest.failures.len(), 1);
    assert!(!output.join("files/hello.txt").exists());
}

#[test]
fn extraction_stays_under_the_opened_root_when_the_path_is_replaced() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let moved = fixture.path().join("moved-output");
    let outside = fixture.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();
    std::fs::rename(&output, &moved).unwrap();
    symlink(&outside, &output).unwrap();

    extractor.extract_file(&filesystem, &entry).unwrap();

    assert_eq!(std::fs::read(moved.join("hello.txt")).unwrap(), CONTENT);
    assert!(!outside.join("hello.txt").exists());
}

#[test]
fn sanitizes_control_separator_like_and_percent_characters() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let mut entry = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    entry.name = "line\nbreak%\\name.txt".to_owned();
    let output = fixture.path().join("output");
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();

    let record = extractor.extract_file(&filesystem, &entry).unwrap();

    assert_eq!(record.original_name, "line\nbreak%\\name.txt");
    assert_eq!(record.relative_path, "line%0Abreak%25%5Cname.txt");
    assert!(output.join("line%0Abreak%25%5Cname.txt").exists());
}

#[test]
fn enforces_the_total_extraction_budget_across_files() {
    let (fixture, image) = ext4_with_files(&[("first.txt", CONTENT), ("second.txt", CONTENT)]);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entries = root_entries(&filesystem);
    let first = entries
        .iter()
        .find(|entry| entry.name == "first.txt")
        .unwrap();
    let second = entries
        .iter()
        .find(|entry| entry.name == "second.txt")
        .unwrap();
    let output = fixture.path().join("output");
    let options = ExtractionOptions {
        max_file_bytes: 32,
        max_total_bytes: 20,
        ..ExtractionOptions::default()
    };
    let mut extractor = Extractor::create(&output, options).unwrap();

    extractor.extract_file(&filesystem, first).unwrap();
    let error = extractor.extract_file(&filesystem, second).unwrap_err();

    assert!(error.message().contains("total"));
    assert!(output.join("first.txt").exists());
    assert!(!output.join("second.txt").exists());
}

#[test]
fn rejects_evidence_symlinks_instead_of_preserving_them() {
    let (fixture, image) = ext4_with_files(&[("hello.txt", CONTENT)]);
    run(Command::new("debugfs")
        .args(["-w", "-R", "symlink /link hello.txt"])
        .arg(&image));
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let link = root_entries(&filesystem)
        .into_iter()
        .find(|entry| entry.name == "link")
        .unwrap();
    assert_eq!(link.metadata_type, 6);
    let output = fixture.path().join("output");
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();

    let error = extractor.extract_file(&filesystem, &link).unwrap_err();

    assert!(error.message().contains("regular files"));
    assert!(!output.join("link").exists());
}
