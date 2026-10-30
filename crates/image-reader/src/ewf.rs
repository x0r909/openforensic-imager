use std::path::Path;

use forensic_core::ByteRange;
use libewf_bindings::EwfReader;

use crate::ReadError;
use crate::raw::RawReader;
use crate::source::{ImageError, ImageErrorCode, ImageFormat, ImageMetadata, ImageReader};

pub struct EwfImageReader {
    reader: EwfReader,
    metadata: ImageMetadata,
}

impl EwfImageReader {
    pub fn open(path: &Path) -> Result<Self, ImageError> {
        let source = RawReader::open_read_only(path)?.metadata().source;
        let reader = EwfReader::open(path).map_err(map_backend_error)?;
        let metadata = ImageMetadata {
            format: ImageFormat::Ewf1E01,
            logical_length: reader.media_size(),
            sector_size: reader.bytes_per_sector(),
            source,
        };
        Ok(Self { reader, metadata })
    }

    pub fn stored_md5(&self) -> Option<&str> {
        self.reader.stored_md5()
    }

    pub fn stored_sha1(&self) -> Option<&str> {
        self.reader.stored_sha1()
    }

    pub fn is_corrupted(&self) -> Result<bool, ImageError> {
        self.reader.is_corrupted().map_err(map_backend_error)
    }
}

impl ImageReader for EwfImageReader {
    fn logical_length(&self) -> u64 {
        self.metadata.logical_length
    }

    fn sector_size(&self) -> u32 {
        self.metadata.sector_size
    }

    fn read_exact_at(&self, range: ByteRange, out: &mut [u8]) -> Result<(), ReadError> {
        if range.length() != out.len() as u64 {
            return Err(ImageError::new(
                ImageErrorCode::BufferLengthMismatch,
                "output buffer length differs from range length",
            ));
        }
        if range.end() > self.logical_length() {
            return Err(ImageError::new(
                ImageErrorCode::OutOfRange,
                "requested range exceeds the E01 image length",
            ));
        }

        let mut completed = 0_usize;
        while completed < out.len() {
            let offset = range
                .offset()
                .checked_add(completed as u64)
                .ok_or_else(|| {
                    ImageError::new(ImageErrorCode::OutOfRange, "E01 read offset overflowed")
                })?;
            let count = self
                .reader
                .read_at(offset, &mut out[completed..])
                .map_err(map_backend_error)?;
            if count == 0 {
                return Err(ImageError::new(
                    ImageErrorCode::Backend,
                    "libewf returned an incomplete read",
                ));
            }
            completed += count;
        }
        Ok(())
    }

    fn metadata(&self) -> ImageMetadata {
        ImageMetadata {
            format: self.metadata.format,
            logical_length: self.metadata.logical_length,
            sector_size: self.metadata.sector_size,
            source: self.metadata.source.clone(),
        }
    }
}

fn map_backend_error(error: libewf_bindings::EwfError) -> ImageError {
    ImageError::new(
        ImageErrorCode::Backend,
        format!("{}: {}", error.operation(), error.message()),
    )
}
