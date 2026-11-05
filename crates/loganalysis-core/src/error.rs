use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    #[error("Invalid regex syntax: {0}")]
    InvalidRegex(String),
    #[error("JSON format error: {0}")]
    JsonError(String),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    #[error("Invalid search pattern/regex: {0}")]
    InvalidRegex(String),
    #[error("Search cancelled")]
    Cancelled,
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AdbError {
    #[error("Invalid device serial '{0}': contains illegal characters or leading dash")]
    InvalidSerial(String),
    #[error("ADB execution failed: {0}")]
    ExecutionFailed(String),
    #[error("ADB command timed out after {0:?}")]
    Timeout(Duration),
    #[error("Failed to capture stdout/stderr of ADB process")]
    IoCaptureFailed,
    #[error("ADB exited with non-zero status code: {0:?}")]
    NonZeroExit(Option<i32>),
}
