use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    RangeOutOfBounds,
    RangeOverflow,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreError {
    code: ErrorCode,
    message: String,
    retryable: bool,
    context: BTreeMap<String, String>,
}

impl CoreError {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            context: BTreeMap::new(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message, true)
    }

    pub fn with_context(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.context.insert(key.into(), value.to_string());
        self
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn context(&self) -> &BTreeMap<String, String> {
        &self.context
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code_name(), self.message)
    }
}

impl std::error::Error for CoreError {}

impl CoreError {
    fn code_name(&self) -> &'static str {
        match self.code {
            ErrorCode::InvalidInput => "invalid_input",
            ErrorCode::RangeOutOfBounds => "range_out_of_bounds",
            ErrorCode::RangeOverflow => "range_overflow",
            ErrorCode::Internal => "internal",
        }
    }
}
