use serde::{Deserialize, Serialize};

use crate::{CoreError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    offset: u64,
    length: u64,
}

impl ByteRange {
    pub fn new(offset: u64, length: u64, subject_length: u64) -> Result<Self, CoreError> {
        let end = offset.checked_add(length).ok_or_else(|| {
            CoreError::new(
                ErrorCode::RangeOverflow,
                "byte range end exceeds u64",
                false,
            )
            .with_context("offset", offset)
            .with_context("length", length)
        })?;

        if end > subject_length {
            return Err(CoreError::new(
                ErrorCode::RangeOutOfBounds,
                "byte range exceeds the subject length",
                false,
            )
            .with_context("offset", offset)
            .with_context("length", length)
            .with_context("subject_length", subject_length));
        }

        Ok(Self { offset, length })
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn end(&self) -> u64 {
        self.offset + self.length
    }
}
