//! What a tool fails with.

use crate::CallError;
use alloc::string::{String, ToString};

/// A tool's failure. `message` is what its caller reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
}

/// What a tool returns: its reply, or why it failed.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A syscall's failure fails the tool with the host's message unchanged.
impl From<CallError> for Error {
    fn from(error: CallError) -> Self {
        Self {
            message: error.message().into(),
        }
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}
