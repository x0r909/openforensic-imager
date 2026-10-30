use std::process::Command;

use filesystem::{FileSystem, FileSystemKind, TimestampSemantics};
use tempfile::tempdir;

const CONTENT: &[u8] = b"forensic-content";

fn write_source(directory: &std::path::Path) -> std::path::PathBuf {
    let source = directory.join("hello.txt");
    std::fs::write(&source, CONTENT).unwrap();
    source
}

fn create_sized(path: &std::path::Path, size: u64) {
    let file = std::fs::File::create(path).unwrap();
    file.set_len(size).unwrap();
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_debugfs(image: &std::path::Path, command: &str) {
    run(Command::new("debugfs")
        .arg("-w")
        .arg("-R")
        .arg(command)
        .arg(image));
}

fn assert_hello_present(
    kind: FileSystemKind,
    semantics: TimestampSemantics,
    image: &std::path::Path,
) {
    let filesystem = FileSystem::open(image, 0).unwrap();
    assert_eq!(filesystem.kind(), kind);
    let entries = filesystem.list_directory(filesystem.root_inode()).unwrap();
    let hello = entries
        .iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap_or_else(|| panic!("hello.txt missing from {kind:?}: {entries:?}"));
    assert_eq!(hello.size, CONTENT.len() as u64);
    let timestamp = hello.mtime.as_ref().expect("mtime should be present");
    assert_eq!(timestamp.semantics, semantics);
    let data = filesystem.read_file(hello.metadata_address, 1024).unwrap();
    assert_eq!(data, CONTENT);
}

#[test]
fn explores_ext4_directory_metadata_and_content() {
    let directory = tempdir().unwrap();
    let source = write_source(directory.path());
    let image = directory.path().join("ext4.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mke2fs")
        .arg("-q")
        .arg("-F")
        .arg("-t")
        .arg("ext4")
        .arg("-L")
        .arg("OFTEST")
        .arg(&image));
    run_debugfs(&image, &format!("write {} /hello.txt", source.display()));

    assert_hello_present(FileSystemKind::Ext4, TimestampSemantics::Utc, &image);
}

#[test]
fn explores_ntfs_directory_metadata_and_content() {
    let directory = tempdir().unwrap();
    let source = write_source(directory.path());
    let image = directory.path().join("ntfs.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mkntfs")
        .arg("-F")
        .arg("-Q")
        .arg("-L")
        .arg("OFTEST")
        .arg(&image));
    run(Command::new("ntfscp")
        .arg("-f")
        .arg(&image)
        .arg(&source)
        .arg("/hello.txt"));

    assert_hello_present(FileSystemKind::Ntfs, TimestampSemantics::Utc, &image);
}

#[test]
fn explores_fat32_directory_metadata_and_content() {
    let directory = tempdir().unwrap();
    let source = write_source(directory.path());
    let image = directory.path().join("fat32.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mkfs.vfat")
        .arg("-F")
        .arg("32")
        .arg("-n")
        .arg("OFTEST")
        .arg(&image));
    run(Command::new("mcopy")
        .arg("-i")
        .arg(&image)
        .arg(&source)
        .arg("::hello.txt"));

    assert_hello_present(
        FileSystemKind::Fat32,
        TimestampSemantics::LocalWallClock,
        &image,
    );
}

#[test]
fn opens_exfat_and_lists_root_entries() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("exfat.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mkfs.exfat")
        .arg("-L")
        .arg("OFTEST")
        .arg(&image));

    let filesystem = FileSystem::open(&image, 0).unwrap();
    assert_eq!(filesystem.kind(), FileSystemKind::Exfat);
    let entries = filesystem.list_directory(filesystem.root_inode()).unwrap();
    assert!(!entries.is_empty());
}

#[test]
fn deleted_ext4_name_is_marked() {
    let directory = tempdir().unwrap();
    let source = write_source(directory.path());
    let image = directory.path().join("ext4-deleted.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mke2fs")
        .arg("-q")
        .arg("-F")
        .arg("-t")
        .arg("ext4")
        .arg("-L")
        .arg("OFTEST")
        .arg(&image));
    run_debugfs(&image, &format!("write {} /deleted.txt", source.display()));
    run_debugfs(&image, "rm /deleted.txt");

    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entries = filesystem.list_directory(filesystem.root_inode()).unwrap();
    let deleted = entries
        .iter()
        .find(|entry| entry.name == "deleted.txt")
        .expect("deleted name should remain");
    assert!(deleted.deleted);
    assert!(deleted.metadata_address > 0);
}

#[test]
fn missing_image_path_is_an_error() {
    let directory = tempdir().unwrap();
    let error = FileSystem::open(&directory.path().join("missing.raw"), 0).unwrap_err();
    assert!(!error.message().is_empty());
}

#[test]
fn read_limit_is_enforced() {
    let directory = tempdir().unwrap();
    let source = write_source(directory.path());
    let image = directory.path().join("ext4-limit.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mke2fs")
        .arg("-q")
        .arg("-F")
        .arg("-t")
        .arg("ext4")
        .arg(&image));
    run_debugfs(&image, &format!("write {} /hello.txt", source.display()));
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = filesystem
        .list_directory(filesystem.root_inode())
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();

    let error = filesystem.read_file(entry.metadata_address, 4).unwrap_err();

    assert!(error.message().contains("limit"));
}

#[test]
fn reads_an_evidence_file_in_bounded_chunks() {
    let directory = tempdir().unwrap();
    let source = write_source(directory.path());
    let image = directory.path().join("ext4-stream.raw");
    create_sized(&image, 64 * 1024 * 1024);
    run(Command::new("mke2fs")
        .arg("-q")
        .arg("-F")
        .arg("-t")
        .arg("ext4")
        .arg(&image));
    run_debugfs(&image, &format!("write {} /hello.txt", source.display()));
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entry = filesystem
        .list_directory(filesystem.root_inode())
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();

    let file = filesystem.open_file(entry.metadata_address).unwrap();
    let mut first = [0_u8; 4];
    let mut second = [0_u8; 32];
    let first_count = file.read_at(0, &mut first).unwrap();
    let second_count = file.read_at(4, &mut second).unwrap();

    assert_eq!(file.size(), CONTENT.len() as u64);
    assert_eq!(first_count, 4);
    assert_eq!(&first, b"fore");
    assert_eq!(second_count, CONTENT.len() - 4);
    assert_eq!(&second[..second_count], &CONTENT[4..]);
}
