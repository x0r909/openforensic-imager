use proptest::prelude::*;
use viewer::{
    ByteSource, MAX_PAGE_SIZE, PreviewContent, PreviewRenderer, SignatureCategory, SignatureKind,
    TextEncoding, hex_page, identify_signature, plan_preview, preview_text, render_plan,
};

#[test]
fn hex_page_renders_offset_hex_and_ascii() {
    let bytes = b"OpenForensic Imager\x00!";

    let page = hex_page(&bytes[..], 0, 32).unwrap();

    assert_eq!(page.offset, 0);
    assert_eq!(page.bytes, bytes);
    assert!(page.truncated);
    let row = &page.rows()[0];
    assert_eq!(row.offset, 0);
    assert_eq!(row.hex, "4f 70 65 6e 46 6f 72 65 6e 73 69 63 20 49 6d 61");
    assert_eq!(row.ascii, "OpenForensic Ima");
}

#[test]
fn hex_page_reads_only_the_requested_window() {
    let bytes: Vec<u8> = (0_u8..=255).collect();

    let page = hex_page(&bytes[..], 16, 16).unwrap();

    assert_eq!(page.offset, 16);
    assert_eq!(page.bytes, (16_u8..32).collect::<Vec<_>>());
    assert_eq!(page.rows().len(), 1);
    assert_eq!(page.rows()[0].offset, 16);
}

#[test]
fn hex_page_rejects_requests_above_the_preview_budget() {
    let error = hex_page(&[0_u8; 8][..], 0, MAX_PAGE_SIZE + 1).unwrap_err();

    assert!(error.message().contains("limit"));
}

#[test]
fn hex_page_marks_a_short_read_at_end_of_source() {
    let page = hex_page(&b"abc"[..], 0, 16).unwrap();

    assert_eq!(page.bytes, b"abc");
    assert!(page.truncated);
    assert_eq!(page.rows()[0].ascii, "abc");
}

#[test]
fn text_preview_decodes_utf8_and_marks_the_read_as_truncated() {
    let source = "forensik 世界".as_bytes();

    let preview = preview_text(source, 0, 12).unwrap();

    assert_eq!(preview.content, PreviewContent::Text);
    assert_eq!(preview.encoding, Some(TextEncoding::Utf8));
    assert_eq!(preview.text, "forensik 世");
    assert!(preview.truncated);
}

#[test]
fn text_preview_decodes_utf16le_with_a_bom() {
    let mut source = vec![0xff, 0xfe];
    for unit in "forensic".encode_utf16() {
        source.extend_from_slice(&unit.to_le_bytes());
    }

    let preview = preview_text(&source[..], 0, 1024).unwrap();

    assert_eq!(preview.content, PreviewContent::Text);
    assert_eq!(preview.encoding, Some(TextEncoding::Utf16Le));
    assert_eq!(preview.text, "forensic");
}

#[test]
fn text_preview_rejects_binary_content() {
    let preview = preview_text(&[0x00, 0x01, 0x02, 0xff][..], 0, 1024).unwrap();

    assert_eq!(preview.content, PreviewContent::Binary);
    assert_eq!(preview.encoding, None);
    assert!(preview.text.is_empty());
}

#[test]
fn signature_identification_covers_common_evidence_types() {
    let cases: &[(&[u8], SignatureKind, SignatureCategory)] = &[
        (
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
            SignatureKind::Png,
            SignatureCategory::Image,
        ),
        (
            b"\xff\xd8\xff\xe0\0\x10JFIF\0",
            SignatureKind::Jpeg,
            SignatureCategory::Image,
        ),
        (
            b"GIF89a\x01\0\x01\0",
            SignatureKind::Gif,
            SignatureCategory::Image,
        ),
        (
            b"%PDF-1.7\n",
            SignatureKind::Pdf,
            SignatureCategory::Document,
        ),
        (
            b"PK\x03\x04\x14\0\0\0",
            SignatureKind::Zip,
            SignatureCategory::Archive,
        ),
        (
            b"\x7fELF\x02\x01\x01\0",
            SignatureKind::Elf,
            SignatureCategory::Executable,
        ),
        (
            b"MZ\x90\0\x03\0\0\0",
            SignatureKind::Pe,
            SignatureCategory::Executable,
        ),
        (
            b"EVF\x09\x0d\x0a\xff\0",
            SignatureKind::Ewf1,
            SignatureCategory::ForensicImage,
        ),
    ];

    for (bytes, kind, category) in cases {
        let signature = identify_signature(&bytes[..]).unwrap();
        assert_eq!(signature.kind, *kind);
        assert_eq!(signature.category, *category);
    }
}

#[test]
fn image_and_pdf_plans_require_isolated_rendering() {
    let png = identify_signature(&b"\x89PNG\r\n\x1a\n"[..]).unwrap();
    let pdf = identify_signature(&b"%PDF-1.7\n"[..]).unwrap();
    let unknown = identify_signature(&b"plain text"[..]).unwrap();

    assert_eq!(render_plan(png).renderer, PreviewRenderer::Image);
    assert!(render_plan(png).isolated);
    assert_eq!(render_plan(pdf).renderer, PreviewRenderer::Pdf);
    assert!(render_plan(pdf).isolated);
    assert_eq!(render_plan(unknown).renderer, PreviewRenderer::Hex);
}

#[test]
fn image_preflight_reads_png_dimensions() {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&100_u32.to_be_bytes());
    png.extend_from_slice(&50_u32.to_be_bytes());
    let signature = identify_signature(&png[..]).unwrap();

    let plan = plan_preview(&png[..], signature).unwrap();

    assert_eq!(plan.renderer, PreviewRenderer::Image);
    assert_eq!(plan.image_pixels, Some(5000));
}

#[test]
fn image_preflight_rejects_a_decompression_bomb_dimension() {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&100_000_u32.to_be_bytes());
    png.extend_from_slice(&100_000_u32.to_be_bytes());
    let signature = identify_signature(&png[..]).unwrap();

    let error = plan_preview(&png[..], signature).unwrap_err();

    assert!(error.message().contains("pixel"));
}

#[test]
fn image_preflight_reads_jpeg_sof_dimensions() {
    let jpeg = [
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x02,
        0x00, 0x03, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
    ];
    let signature = identify_signature(&jpeg[..]).unwrap();

    let plan = plan_preview(&jpeg[..], signature).unwrap();

    assert_eq!(plan.renderer, PreviewRenderer::Image);
    assert_eq!(plan.image_pixels, Some(6));
}

#[test]
fn image_preflight_rejects_a_truncated_header() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0";
    let signature = identify_signature(&png[..]).unwrap();

    let error = plan_preview(&png[..], signature).unwrap_err();

    assert!(error.message().contains("truncated"));
}

struct FailingSource;

impl ByteSource for FailingSource {
    fn len(&self) -> u64 {
        1
    }

    fn read_at(&self, _offset: u64, _buffer: &mut [u8]) -> Result<usize, viewer::SourceError> {
        Err(viewer::SourceError::new("synthetic read failure"))
    }
}

#[test]
fn source_errors_are_returned_without_partial_preview_success() {
    let error = hex_page(&FailingSource, 0, 16).unwrap_err();

    assert!(error.message().contains("synthetic"));
}

struct OverReportingSource;

impl ByteSource for OverReportingSource {
    fn len(&self) -> u64 {
        1
    }

    fn read_at(&self, _offset: u64, buffer: &mut [u8]) -> Result<usize, viewer::SourceError> {
        Ok(buffer.len() + 1)
    }
}

#[test]
fn rejects_sources_that_claim_more_bytes_than_the_buffer_holds() {
    let error = hex_page(&OverReportingSource, 0, 16).unwrap_err();

    assert!(error.message().contains("invalid byte count"));
}

#[test]
fn preview_plan_rejects_a_signature_that_does_not_match_the_source() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    let forged = viewer::FileSignature {
        kind: SignatureKind::Pdf,
        category: SignatureCategory::Document,
    };

    let error = plan_preview(&png[..], forged).unwrap_err();

    assert!(error.message().contains("does not match"));
}

proptest! {
    #[test]
    fn hex_pages_never_exceed_the_request(offset in 0_u64..4096, length in 1_usize..4096) {
        let bytes = vec![0x41_u8; 4096];
        let page = hex_page(&bytes[..], offset, length).unwrap();
        prop_assert!(page.bytes.len() <= length);
        prop_assert_eq!(page.offset, offset);
    }
}
