use std::process::Command;

use extraction::{ExtractionOptions, Extractor};
use filesystem::{FileEntry, FileSystem};
use hashing::{HashAlgorithm, StreamingHasher};
use reporting::write_extraction_report_set;
use tempfile::tempdir;
use viewer::{
    PreviewContent, PreviewRenderer, SignatureKind, hex_page, identify_signature, plan_preview,
    preview_text,
};

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sha256_file(path: &std::path::Path) -> String {
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    for chunk in std::fs::read(path).unwrap().chunks(64 * 1024) {
        hasher.update(chunk);
    }
    hasher.finish()[&HashAlgorithm::Sha256].to_hex()
}

fn entry<'a>(entries: &'a [FileEntry], name: &str) -> &'a FileEntry {
    entries.iter().find(|entry| entry.name == name).unwrap()
}

#[test]
fn previews_and_extracts_multiple_evidence_file_types() {
    let directory = tempdir().unwrap();
    let image = directory.path().join("evidence.raw");
    std::fs::File::create(&image)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    run(Command::new("mke2fs")
        .args(["-q", "-F", "-t", "ext4", "-L", "PHASE4"])
        .arg(&image));

    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&320_u32.to_be_bytes());
    png.extend_from_slice(&200_u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    let fixtures = [
        ("notes.txt", "Catatan forensik 世界\n".as_bytes()),
        ("picture.png", png.as_slice()),
        ("document.pdf", b"%PDF-1.7\nsynthetic\n%%EOF\n"),
        ("blob.bin", &[0_u8, 1, 2, 0xff][..]),
    ];
    for (index, (name, content)) in fixtures.iter().enumerate() {
        let source = directory.path().join(format!("source-{index}"));
        std::fs::write(&source, content).unwrap();
        run(Command::new("debugfs")
            .args(["-w", "-R"])
            .arg(format!("write {} /{name}", source.display()))
            .arg(&image));
    }

    let source_hash = sha256_file(&image);
    let filesystem = FileSystem::open(&image, 0).unwrap();
    let entries = filesystem.list_directory(filesystem.root_inode()).unwrap();

    let notes = filesystem
        .read_file(entry(&entries, "notes.txt").metadata_address, 4096)
        .unwrap();
    let text = preview_text(&notes, 0, 4096).unwrap();
    assert_eq!(text.content, PreviewContent::Text);
    assert!(text.text.contains("世界"));
    let hex = hex_page(&notes, 0, 32).unwrap();
    assert_eq!(hex.bytes, &notes[..notes.len().min(32)]);

    let png = filesystem
        .read_file(entry(&entries, "picture.png").metadata_address, 4096)
        .unwrap();
    let png_signature = identify_signature(&png).unwrap();
    assert_eq!(png_signature.kind, SignatureKind::Png);
    let png_plan = plan_preview(&png, png_signature).unwrap();
    assert_eq!(png_plan.renderer, PreviewRenderer::Image);
    assert_eq!(png_plan.image_pixels, Some(64_000));

    let pdf = filesystem
        .read_file(entry(&entries, "document.pdf").metadata_address, 4096)
        .unwrap();
    let pdf_signature = identify_signature(&pdf).unwrap();
    assert_eq!(pdf_signature.kind, SignatureKind::Pdf);
    assert_eq!(
        plan_preview(&pdf, pdf_signature).unwrap().renderer,
        PreviewRenderer::Pdf
    );

    let output = directory.path().join("exports");
    let reports = directory.path().join("reports");
    std::fs::create_dir(&reports).unwrap();
    let mut extractor = Extractor::create(&output, ExtractionOptions::default()).unwrap();
    let manifest = extractor
        .extract_directory(&filesystem, filesystem.root_inode(), "files")
        .unwrap();
    assert_eq!(manifest.records.len(), fixtures.len());
    let report_paths = write_extraction_report_set(&reports, "phase4", &manifest).unwrap();

    assert_eq!(
        std::fs::read(output.join("files/notes.txt")).unwrap(),
        "Catatan forensik 世界\n".as_bytes()
    );
    assert_eq!(
        std::fs::read(output.join("files/picture.png")).unwrap(),
        png
    );
    assert!(report_paths.json.exists());
    assert!(
        std::fs::read_to_string(report_paths.html)
            .unwrap()
            .contains("notes.txt")
    );
    assert_eq!(sha256_file(&image), source_hash);
}
