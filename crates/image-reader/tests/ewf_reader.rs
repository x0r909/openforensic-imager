use forensic_core::ByteRange;
use image_reader::{EwfImageReader, ImageFormat, ImageReader};
use libewf_bindings::{EwfCompression, EwfWriteOptions, EwfWriter};
use tempfile::tempdir;

fn make_e01(directory: &std::path::Path) -> std::path::PathBuf {
    let source: Vec<u8> = (0..8192_usize)
        .map(|index| (index.wrapping_mul(17) & 0xff) as u8)
        .collect();
    let mut writer = EwfWriter::create(
        &directory.join("evidence"),
        source.len() as u64,
        EwfWriteOptions {
            bytes_per_sector: 512,
            sectors_per_chunk: 64,
            maximum_segment_size: 1024 * 1024,
            compression: EwfCompression::None,
        },
    )
    .unwrap();
    writer.write_all(&source).unwrap();
    writer.finalize().unwrap().remove(0)
}

#[test]
fn ewf_reader_exposes_random_access_and_metadata() {
    let directory = tempdir().unwrap();
    let path = make_e01(directory.path());
    let reader = EwfImageReader::open(&path).unwrap();

    assert_eq!(reader.logical_length(), 8192);
    assert_eq!(reader.sector_size(), 512);
    assert_eq!(reader.metadata().format, ImageFormat::Ewf1E01);
    assert!(reader.metadata().source.read_only());

    let range = ByteRange::new(4096, 512, reader.logical_length()).unwrap();
    let mut buffer = [0_u8; 512];
    reader.read_exact_at(range, &mut buffer).unwrap();
    assert_eq!(buffer[0], ((4096_u64.wrapping_mul(17)) & 0xff) as u8);
    assert_eq!(buffer[511], ((4607_u64.wrapping_mul(17)) & 0xff) as u8);
}

#[test]
fn ewf_reader_rejects_out_of_range_read() {
    let directory = tempdir().unwrap();
    let path = make_e01(directory.path());
    let reader = EwfImageReader::open(&path).unwrap();
    let range = ByteRange::new(8192, 1, 8193).unwrap();
    let mut buffer = [0_u8; 1];

    let error = reader.read_exact_at(range, &mut buffer).unwrap_err();

    assert_eq!(error.code(), image_reader::ImageErrorCode::OutOfRange);
}
