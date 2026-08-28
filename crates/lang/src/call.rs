//! Calling a tool on another harness.
//!
//! The target is named at the call, not at build time, so one image works
//! against whatever it was deployed beside — the same way a container reaches
//! another by name rather than by having been linked to it. What is deployed is
//! reachable; a name that is not answers [`CallError::Refused`].
//!
//! Whether a host serves this at all is its own decision. berm itself runs one
//! harness and registers nothing for it, so under a plain [`berm::Berm`] the
//! call traps as an unknown host call.
#![cfg(feature = "alloc")]

use crate::abi::{self, host, wire};
use alloc::string::{String, ToString};

pub use host::CallError;

/// Run `tool` on the harness deployed as `harness`, with `args` as its argument
/// blob — the same JSON a model would have sent it.
///
/// ```ignore
/// let result = berm_lang::call("weather", "forecast", r#"{"city":"Oslo"}"#)?;
/// ```
///
/// `Err` says which kind of failure it was. [`CallError::Refused`] means
/// nothing ran — no such harness, or the chain is already as deep as the host
/// allows — and [`CallError::Failed`] means the tool ran and reported failure.
pub fn call(harness: &str, tool: &str, args: &str) -> Result<String, CallError> {
    let request = wire::request(&[harness.as_bytes(), tool.as_bytes(), args.as_bytes()]);
    let result = host::call(abi::HOST_CALL, &request)?;
    Ok(String::from_utf8_lossy(&result).into_owned())
}

/// Have `tool` on `harness` run in `delay` milliseconds, and return now.
///
/// ```ignore
/// berm_lang::after(300, "weather", "poll", "{}")?;
/// ```
///
/// One wake per harness: arming again drops whatever this one had pending, so
/// a harness holds exactly one and cannot fan out. A harness wanting several
/// keeps them in its own keys and arms for the earliest.
///
/// The invocation it starts carries nothing from this one — guest memory does
/// not cross — so what the woken tool needs goes in [`crate::set`], and how
/// late it actually ran is [`crate::now`] against what it expected. Nothing
/// holds its result: a failure there is logged by the host and reaches no one,
/// which is why the harness and tool are checked here instead.
pub fn after(delay: u64, harness: &str, tool: &str, args: &str) -> Result<(), CallError> {
    let request = wire::request(&[
        delay.to_string().as_bytes(),
        harness.as_bytes(),
        tool.as_bytes(),
        args.as_bytes(),
    ]);
    host::call(abi::HOST_CALL_AFTER, &request)?;
    Ok(())
}
