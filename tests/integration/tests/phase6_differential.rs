use std::io::Write;
use std::process::{Command, Stdio};

use filesystem::{FileSystem, FileSystemKind};
use hashing::{HashAlgorithm, StreamingHasher};
use image_reader::open_image;
use partition_analyzer::{PartitionScheme, analyze_partitions};
use tempfile::tempdir;

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_with_input(command: &mut Command, input: &str) {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    hasher.update(bytes);
    hasher.finish()[&HashAlgorithm::Sha256].to_hex()
}

#[test]
fn mbr_and_gpt_match_independent_partition_tools() {
    let directory = tempdir().unwrap();
    let mbr = directory.path().join("mbr.raw");
    std::fs::File::create(&mbr)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    run_with_input(
        Command::new("sfdisk").arg(&mbr),
        "label: dos\nstart=2048, size=131072, type=83\nstart=133120, size=65536, type=07\n",
    );
    let analysis = analyze_partitions(&mbr).unwrap();
    assert_eq!(analysis.scheme, PartitionScheme::Mbr);
    let sfdisk = Command::new("sfdisk").arg("-d").arg(&mbr).output().unwrap();
    assert!(sfdisk.status.success());
    let sfdisk_text = String::from_utf8_lossy(&sfdisk.stdout);
    let normalized = sfdisk_text.replace(['=', ',', ':'], " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    for partition in analysis
        .partitions
        .iter()
        .filter(|partition| partition.allocated)
    {
        let start = partition.start_sector.to_string();
        let size = partition.sector_count.to_string();
        assert!(
            tokens
                .windows(2)
                .any(|pair| pair[0] == "start" && pair[1] == start),
            "missing MBR start {} in {sfdisk_text}",
            partition.start_sector
        );
        assert!(
            tokens
                .windows(2)
                .any(|pair| pair[0] == "size" && pair[1] == size),
            "missing MBR size {} in {sfdisk_text}",
            partition.sector_count
        );
    }

    let gpt = directory.path().join("gpt.raw");
    std::fs::File::create(&gpt)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    run(Command::new("sgdisk").arg("-o").arg(&gpt));
    run(Command::new("sgdisk")
        .args(["-n", "1:2048:+32M", "-t", "1:8300", "-c", "1:LINUX"])
        .arg(&gpt));
    let analysis = analyze_partitions(&gpt).unwrap();
    assert_eq!(analysis.scheme, PartitionScheme::Gpt);
    let sgdisk = Command::new("sgdisk")
        .arg("-i")
        .arg("1")
        .arg(&gpt)
        .output()
        .unwrap();
    assert!(sgdisk.status.success());
    let sgdisk_text = String::from_utf8_lossy(&sgdisk.stdout);
    assert!(sgdisk_text.contains("First sector: 2048"));
    assert!(sgdisk_text.contains("Partition size: 65536 sectors"));
    assert!(sgdisk_text.contains("Partition name: 'LINUX'"));
    assert!(
        analysis
            .partitions
            .iter()
            .any(|partition| partition.start_sector == 2048 && partition.sector_count == 65536)
    );
}

#[test]
fn ext4_listing_and_content_match_debugfs() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("ext4.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    run(Command::new("mke2fs")
        .args(["-q", "-F", "-t", "ext4", "-L", "DIFF"])
        .arg(&image));
    let source = directory.path().join("hello.txt");
    std::fs::write(&source, b"differential-content").unwrap();
    run(Command::new("debugfs")
        .args(["-w", "-R"])
        .arg(format!("write {} /hello.txt", source.display()))
        .arg(&image));
    let filesystem = FileSystem::open(&image, 0).unwrap();
    assert_eq!(filesystem.kind(), FileSystemKind::Ext4);
    let entry = filesystem
        .list_directory(filesystem.root_inode())
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "hello.txt")
        .unwrap();
    let content = filesystem.read_file(entry.metadata_address, 4096).unwrap();
    let debugfs = Command::new("debugfs")
        .args(["-R", "cat /hello.txt"])
        .arg(&image)
        .output()
        .unwrap();
    assert!(debugfs.status.success());
    assert_eq!(content, debugfs.stdout);
    assert_eq!(sha256(&content), sha256(b"differential-content"));
}

#[test]
fn raw_and_e01_logical_streams_match_independent_tools() {
    let directory = tempdir().unwrap();
    let raw = directory.path().join("source.raw");
    let bytes = (0_u8..=255).cycle().take(256 * 1024).collect::<Vec<_>>();
    std::fs::write(&raw, &bytes).unwrap();
    let target = directory.path().join("image");
    run(Command::new("ewfacquire").args([
        "-u",
        "-q",
        "-f",
        "encase6",
        "-c",
        "none",
        "-t",
        target.to_str().unwrap(),
        raw.to_str().unwrap(),
    ]));
    let e01 = directory.path().join("image.E01");
    let reader = open_image(&e01).unwrap();
    assert_eq!(reader.logical_length(), bytes.len() as u64);
    let mut logical = vec![0_u8; bytes.len()];
    let range = forensic_core::ByteRange::new(0, bytes.len() as u64, bytes.len() as u64).unwrap();
    reader.read_exact_at(range, &mut logical).unwrap();
    assert_eq!(sha256(&logical), sha256(&bytes));

    let export_dir = directory.path().join("export");
    std::fs::create_dir(&export_dir).unwrap();
    let export = Command::new("ewfexport")
        .current_dir(&export_dir)
        .args(["-u", "-q", "-t", "exported", e01.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let exported = std::fs::read(export_dir.join("exported.raw")).unwrap();
    assert_eq!(sha256(&exported), sha256(&bytes));
}

#[test]
fn acquisition_hash_matches_sha256sum() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.raw");
    let bytes = vec![0x5a_u8; 1024 * 1024];
    std::fs::write(&source, &bytes).unwrap();
    let output = Command::new("sha256sum").arg(&source).output().unwrap();
    assert!(output.status.success());
    let expected = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();

    assert_eq!(sha256(&bytes), expected);
}
