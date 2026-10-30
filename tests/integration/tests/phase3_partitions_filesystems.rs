use std::io::{Seek, SeekFrom, Write};
use std::process::Command;

use filesystem::{FileSystem, FileSystemKind};
use partition_analyzer::{PartitionScheme, analyze_partitions};
use tempfile::tempdir;

#[test]
fn partition_offset_opens_the_embedded_filesystem() {
    let directory = tempdir().unwrap();
    let partition_image = directory.path().join("partition.raw");
    let file = std::fs::File::create(&partition_image).unwrap();
    file.set_len(32 * 1024 * 1024).unwrap();
    let output = Command::new("mke2fs")
        .args(["-q", "-F", "-t", "ext4", "-L", "PARTITION"])
        .arg(&partition_image)
        .output()
        .unwrap();
    assert!(output.status.success());

    let source = directory.path().join("hello.txt");
    std::fs::write(&source, b"inside-partition").unwrap();
    let output = Command::new("debugfs")
        .args(["-w", "-R"])
        .arg(format!("write {} /hello.txt", source.display()))
        .arg(&partition_image)
        .output()
        .unwrap();
    assert!(output.status.success());

    let disk_image = directory.path().join("disk.raw");
    let disk = std::fs::File::create(&disk_image).unwrap();
    disk.set_len(128 * 1024 * 1024).unwrap();
    let mut sfdisk = Command::new("sfdisk")
        .arg(&disk_image)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    sfdisk
        .stdin
        .take()
        .unwrap()
        .write_all(b"label: dos\nstart=2048, size=65536, type=83\n")
        .unwrap();
    assert!(sfdisk.wait().unwrap().success());

    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .open(&disk_image)
        .unwrap();
    destination.seek(SeekFrom::Start(2048 * 512)).unwrap();
    let mut source = std::fs::File::open(&partition_image).unwrap();
    std::io::copy(&mut source, &mut destination).unwrap();
    destination.sync_all().unwrap();

    let partitions = analyze_partitions(&disk_image).unwrap();
    assert_eq!(partitions.scheme, PartitionScheme::Mbr);
    let partition = partitions
        .partitions
        .iter()
        .find(|partition| partition.start_sector == 2048)
        .unwrap();
    let filesystem = FileSystem::open(&disk_image, partition.offset_bytes).unwrap();
    assert_eq!(filesystem.kind(), FileSystemKind::Ext4);
    let entries = filesystem.list_directory(filesystem.root_inode()).unwrap();
    assert!(entries.iter().any(|entry| entry.name == "hello.txt"));
}
