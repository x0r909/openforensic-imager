use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::Path;

use forensic_core::ByteRange;

use crate::ReadError;
use crate::source::{
    ImageError, ImageErrorCode, ImageFormat, ImageMetadata, ImageReader, SourceFingerprint,
};

const DEFAULT_SECTOR_SIZE: u32 = 512;
const BLKGETSIZE64: libc::c_ulong = 0x8008_1272;

#[derive(Debug)]
pub struct RawReader {
    file: File,
    fingerprint: SourceFingerprint,
}

impl RawReader {
    pub fn open_read_only(path: &Path) -> Result<Self, ImageError> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)
            .map_err(map_open_error)?;

        let file_metadata = file.metadata().map_err(map_io_error)?;
        let file_type = file_metadata.file_type();
        if !file_type.is_file() && !file_type.is_block_device() {
            return Err(ImageError::new(
                ImageErrorCode::NotRegularFile,
                "RAW source must be a regular file or block device",
            ));
        }

        let fingerprint = capture_fingerprint(&file, &file_metadata)?;
        Ok(Self { file, fingerprint })
    }

    fn ensure_unchanged(&self) -> Result<(), ImageError> {
        let current =
            capture_fingerprint(&self.file, &self.file.metadata().map_err(map_io_error)?)?;
        if current != self.fingerprint {
            return Err(ImageError::new(
                ImageErrorCode::ConcurrentModification,
                "source changed after it was opened",
            ));
        }
        Ok(())
    }
}

impl ImageReader for RawReader {
    fn logical_length(&self) -> u64 {
        self.fingerprint.length()
    }

    fn sector_size(&self) -> u32 {
        DEFAULT_SECTOR_SIZE
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
                "requested range exceeds the RAW image length",
            ));
        }

        self.ensure_unchanged()?;
        self.file
            .read_exact_at(out, range.offset())
            .map_err(map_io_error)?;
        self.ensure_unchanged()
    }

    fn metadata(&self) -> ImageMetadata {
        ImageMetadata {
            format: ImageFormat::Raw,
            logical_length: self.logical_length(),
            sector_size: self.sector_size(),
            source: self.fingerprint.clone(),
        }
    }
}

fn capture_fingerprint(
    file: &File,
    metadata: &std::fs::Metadata,
) -> Result<SourceFingerprint, ImageError> {
    // The descriptor remains owned by `file`; fcntl only reads its access mode.
    let flags = unsafe { libc::fcntl(std::os::fd::AsRawFd::as_raw_fd(file), libc::F_GETFL) };
    if flags < 0 {
        return Err(map_io_error(io::Error::last_os_error()));
    }
    let read_only = flags & libc::O_ACCMODE == libc::O_RDONLY;
    if !read_only {
        return Err(ImageError::new(
            ImageErrorCode::SourceNotReadOnly,
            "source descriptor is not read-only",
        ));
    }

    let length = source_length(file, metadata)?;

    Ok(SourceFingerprint::new(
        metadata.dev(),
        metadata.ino(),
        length,
        (metadata.mtime(), metadata.mtime_nsec()),
        (metadata.ctime(), metadata.ctime_nsec()),
        read_only,
    ))
}

fn source_length(file: &File, metadata: &std::fs::Metadata) -> Result<u64, ImageError> {
    if !metadata.file_type().is_block_device() {
        return Ok(metadata.len());
    }

    let mut length = 0_u64;
    // The descriptor remains owned by `file`; the ioctl only reads capacity.
    let result = unsafe {
        libc::ioctl(
            std::os::fd::AsRawFd::as_raw_fd(file),
            BLKGETSIZE64,
            &mut length,
        )
    };
    if result == 0 {
        Ok(length)
    } else {
        Err(map_io_error(io::Error::last_os_error()))
    }
}

fn map_open_error(error: io::Error) -> ImageError {
    let code = match error.kind() {
        io::ErrorKind::NotFound => ImageErrorCode::NotFound,
        io::ErrorKind::PermissionDenied => ImageErrorCode::PermissionDenied,
        _ if error.raw_os_error() == Some(libc::ELOOP) => ImageErrorCode::SymbolicLink,
        _ => ImageErrorCode::Io,
    };
    ImageError::new(code, error.to_string())
}

fn map_io_error(error: io::Error) -> ImageError {
    ImageError::new(ImageErrorCode::Io, error.to_string())
}
