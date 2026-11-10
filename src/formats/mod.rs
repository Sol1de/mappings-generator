//! Input readers and output writers (spec/formats.md). Readers take text, writers take
//! `io::Write`; no other I/O.

pub mod json;
pub mod mc_mappings;
pub mod proguard;
pub mod srg;
pub mod tiny;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub format: &'static str,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} line {}: {}", self.format, self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn err<T>(
    format: &'static str,
    line: usize,
    message: impl Into<String>,
) -> Result<T, ParseError> {
    Err(ParseError {
        format,
        line,
        message: message.into(),
    })
}
