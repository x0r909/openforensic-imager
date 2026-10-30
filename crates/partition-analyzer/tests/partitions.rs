use std::io::Write;
use std::process::{Command, Stdio};

use partition_analyzer::{PartitionScheme, analyze_partitions};
use tempfile::tempdir;

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

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn mbr_partitions_match_sfdisk_layout() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("mbr.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    run_with_input(
        Command::new("sfdisk").arg(&image),
        "label: dos\nstart=2048, size=131072, type=83\nstart=133120, size=65536, type=07\n",
    );

    let analysis = analyze_partitions(&image).unwrap();

    assert_eq!(analysis.scheme, PartitionScheme::Mbr);
    assert_eq!(analysis.block_size, 512);
    let linux = analysis
        .partitions
        .iter()
        .find(|partition| partition.start_sector == 2048)
        .unwrap();
    assert_eq!(linux.sector_count, 131072);
    assert_eq!(linux.offset_bytes, 2048 * 512);
    assert_eq!(linux.length_bytes, 131072 * 512);
    assert!(linux.allocated);
}

#[test]
fn gpt_partitions_match_sgdisk_layout() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("gpt.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    run(Command::new("sgdisk").arg("-o").arg(&image));
    run(Command::new("sgdisk")
        .arg("-n")
        .arg("1:2048:+32M")
        .arg("-t")
        .arg("1:8300")
        .arg("-c")
        .arg("1:LINUX")
        .arg(&image));
    run(Command::new("sgdisk")
        .arg("-n")
        .arg("2:0:+16M")
        .arg("-t")
        .arg("2:0700")
        .arg("-c")
        .arg("2:WINDOWS")
        .arg(&image));

    let analysis = analyze_partitions(&image).unwrap();

    assert_eq!(analysis.scheme, PartitionScheme::Gpt);
    assert!(analysis.partitions.iter().any(|partition| {
        partition.start_sector == 2048
            && partition.sector_count == 65536
            && partition.description.contains("LINUX")
    }));
}

#[test]
fn empty_image_returns_a_typed_error() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("empty.raw");
    std::fs::write(&image, []).unwrap();

    let error = analyze_partitions(&image).unwrap_err();

    assert!(!error.message().is_empty());
}
