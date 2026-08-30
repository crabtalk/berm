//! Run a program's tools natively, without a toolchain or a daemon.
//!
//! A program is an image that a host compiles and enters, which is a slow way
//! to find out that a handler mishandles an empty string. Off a guest target
//! the exports are ordinary functions and the buffers are ordinary memory, so a
//! test can call one the way the host would — the same argument transfer, the
//! same buffer limits, the same failure channel — and check what came back.
//!
//! ```ignore
//! #[test]
//! fn echo_wraps_the_payload() {
//!     let out = berm_lang::test::call(berm_tool_echo, b"hi").unwrap();
//!     assert_eq!(out, br#"{"echo":hi}"#);
//! }
//! ```
#![cfg(not(any(target_arch = "wasm32", target_arch = "riscv64")))]

use crate::{CallError, abi, sys};
use std::{
    string::String,
    sync::{Mutex, PoisonError},
    vec::Vec,
};

/// A program writes into buffers the macro puts in `.bss`, which on the guest
/// is safe because one invocation runs at a time. A test binary runs its tests
/// in parallel, so they take turns here or they write over each other.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Invoke a tool export with `args`, as the host does.
///
/// `Err` is what the program reported through its failure channel — the same
/// distinction the host draws between a tool that failed and one that returned
/// the word "error".
pub fn call(entry: extern "C" fn(), args: &[u8]) -> Result<Vec<u8>, String> {
    // A failing assertion is a test doing its job, so a panic while holding
    // this must not poison it into failing every other test.
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);

    sys::with(|host| {
        host.args = args.to_vec();
        host.logged.clear();
        host.done = None;
        host.failure = None;
    });

    entry();

    // A tool that returned without answering wrote nothing, which is an empty
    // result rather than a missing one.
    sys::with(|host| match (host.failure.take(), host.done.take()) {
        (Some(failure), _) => Err(failure),
        (None, Some(result)) => Ok(result),
        (None, None) => Ok(Vec::new()),
    })
}

/// What the program logged during the last [`call`].
pub fn logged() -> Vec<String> {
    sys::with(|host| host.logged.clone())
}

/// Arrange what another program answers, so a tool that calls one can be run
/// without a host running the other side.
///
/// Answers persist until [`forget`], so a test sets them up once and calls as
/// often as it likes.
///
/// ```ignore
/// test::answer("weather", "forecast", Ok(br#"{"c":7}"#));
/// test::answer("gone", "any", Err(CallError::Refused("not deployed".into())));
/// ```
pub fn answer(program: &str, tool: &str, result: Result<&[u8], CallError>) {
    let (outcome, bytes) = match result {
        Ok(bytes) => (0, bytes.to_vec()),
        Err(CallError::Failed(message)) => (abi::ERROR, message.into_bytes()),
        Err(CallError::Refused(message)) => (abi::ERROR | abi::REFUSED, message.into_bytes()),
    };
    sys::with(|host| {
        host.answers.retain(|(a, b, ..)| a != program || b != tool);
        host.answers
            .push((program.into(), tool.into(), outcome, bytes));
    });
}

/// Drop every answer [`answer`] arranged.
pub fn forget() {
    sys::with(|host| host.answers.clear());
}
