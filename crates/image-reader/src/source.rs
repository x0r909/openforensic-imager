use std::fmt;

use forensic_core::ByteRange;

use crate::ReadError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFingerprint {
    device: u64,
    inode: u64,
    length: u64,
    mtime_seconds: i64,
    mtime_nanoseconds: i64,
    ctime_seconds: i64,
    ctime_nanoseconds: i64,
    read_only: bool,
}

impl SourceFingerprint {
    pub(crate) fn new(
        device: u64,
        inode: u64,
        length: u64,
        mtime: (i64, i64),
        ctime: (i64, i64),
        read_only: bool,
    ) -> Self {
        Self {
            device,
            inode,
            length,
            mtime_seconds: mtime.0,
            mtime_nanoseconds: mtime.1,
            ctime_seconds: ctime.0,
            ctime_nanoseconds: ctime.1,
            read_only,
        }
    }

    pub fn device(&self) -> u64 {
        self.device
    }

    pub fn inode(&self) -> u64 {
        self.inode
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn mtime_seconds(&self) -> i64 {
        self.mtime_seconds
    }

    pub fn mtime_nanoseconds(&self) -> i64 {
        self.mtime_nanoseconds
    }

    pub fn ctime_seconds(&self) -> i64 {
        self.ctime_seconds
    }

    pub fn ctime_nanoseconds(&self) -> i64 {
        self.ctime_nanoseconds
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageMetadata {
    pub format: ImageFormat,
    pub logical_length: u64,
    pub sector_size: u32,
    pub source: SourceFingerprint,
}

pub trait ImageReader: Send + Sync {
    fn logical_length(&self) -> u64;
    fn sector_size(&self) -> u32;
    fn read_exact_at(&self, range: ByteRange, out: &mut [u8]) -> Result<(), ReadError>;
    fn metadata(&self) -> ImageMetadata;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageErrorCode {
    NotFound,
    PermissionDenied,
    SymbolicLink,
    NotRegularFile,
    SourceNotReadOnly,
    OutOfRange,
    BufferLengthMismatch,
    ConcurrentModification,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageError {
    code: ImageErrorCode,
    message: String,
}

impl ImageError {
    pub(crate) fn new(code: ImageErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> ImageErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ImageError {}
