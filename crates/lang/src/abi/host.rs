//! How a syscall call works, once, for all of them.
//!
//! A call hands the host a request and gets back a length. The bytes stay
//! host-side until the guest asks for them, which is the same pull the
//! argument blob uses (`args.len` then `args.read`) and is here for the same
//! reason: the host never enters a guest to give it something, and a result
//! whose size is unknown in advance cannot be measured by running the work
//! twice.
//!
//! Failure travels on the same wire. The high bit of the returned length says
//! the staged bytes are a message rather than a result, so an error costs no
//! extra call and cannot be mistaken for an empty success. A second bit says
//! whether anything ran at all.

use crate::{
    abi::{ERROR, REFUSED, read_result},
    sys,
};
use alloc::{string::String, vec, vec::Vec};

/// Why a syscall call did not produce a result.
///
/// The distinction is the host's own, carried through unchanged: reaching a
/// program that is not deployed is not the same event as reaching one that ran
/// and said no, and only the caller knows which of them it can do something
/// about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// Whatever the call reached ran, and reported failure.
    Failed(String),
    /// The host refused the call. Nothing ran.
    Refused(String),
}

impl CallError {
    /// What went wrong, whichever kind it was.
    pub fn message(&self) -> &str {
        match self {
            Self::Failed(message) | Self::Refused(message) => message,
        }
    }

    /// Whether the call was refused before anything ran.
    pub fn refused(&self) -> bool {
        matches!(self, Self::Refused(_))
    }
}

impl core::fmt::Display for CallError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.message())
    }
}

/// Make one syscall call. `Err` carries whatever the host said went wrong.
pub fn call(number: u64, request: &[u8]) -> Result<Vec<u8>, CallError> {
    let staged = sys::call2(number, request.as_ptr() as u64, request.len() as u64);
    let failed = staged & ERROR != 0;
    let refused = staged & REFUSED != 0;

    let mut result = vec![0u8; (staged & !(ERROR | REFUSED)) as usize];
    let full = read_result(&mut result);
    if full != result.len() {
        return Err(CallError::Failed(String::from(
            "host staged a result of a different length",
        )));
    }

    if failed {
        let message = String::from_utf8_lossy(&result).into_owned();
        return Err(match refused {
            true => CallError::Refused(message),
            false => CallError::Failed(message),
        });
    }
    Ok(result)
}
