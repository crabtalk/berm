//! What a host call does when there is no host.
//!
//! Off RISC-V this crate is an ordinary library, so a program author can build
//! and unit test their handlers natively rather than cross-compiling to see
//! anything at all. The calls a test can reasonably answer — the argument blob,
//! the log, the failure channel — are served from [`crate::test`]'s state.
//! Anything else panics naming the syscall, because a test that reached a
//! real syscall should say so rather than read a plausible zero.

use crate::abi;
use std::{cell::RefCell, string::String, vec::Vec};

std::thread_local! {
    static HOST: RefCell<Host> = const { RefCell::new(Host::new()) };
}

/// The bits of a host a test can stand in for.
pub(crate) struct Host {
    pub args: Vec<u8>,
    pub logged: Vec<String>,
    pub failure: Option<String>,
    /// What the last syscall call answered with. Staged exactly as a
    /// real host stages it, so the pull in [`crate::abi::host::call`] is the
    /// same code under a test as in a guest.
    pub result: Vec<u8>,
    /// What a test has arranged another program to answer: the program, the
    /// tool, the outcome bits, and the bytes. A short list rather than a map
    /// because a program calls a handful of things, not a thousand.
    pub answers: Vec<(String, String, u64, Vec<u8>)>,
}

impl Host {
    const fn new() -> Self {
        Self {
            args: Vec::new(),
            logged: Vec::new(),
            failure: None,
            result: Vec::new(),
            answers: Vec::new(),
        }
    }
}

/// Run `f` against the thread's stand-in host.
pub(crate) fn with<T>(f: impl FnOnce(&mut Host) -> T) -> T {
    HOST.with(|host| f(&mut host.borrow_mut()))
}

#[inline]
pub fn call0(number: u64) -> u64 {
    match number {
        abi::HOST_ARG_LEN => with(|host| host.args.len() as u64),
        _ => no_host(number),
    }
}

#[inline]
pub fn call2(number: u64, a0: u64, a1: u64) -> u64 {
    match number {
        abi::HOST_ARG_READ => with(|host| {
            let taken = host.args.len().min(a1 as usize);
            // Safety: a0/a1 describe a buffer the caller owns, exactly as the
            // real host requires. Off-target that buffer is ordinary memory.
            unsafe { core::ptr::copy_nonoverlapping(host.args.as_ptr(), a0 as *mut u8, taken) };
            host.args.len() as u64
        }),
        abi::HOST_LOG => {
            with(|host| host.logged.push(read(a0, a1)));
            0
        }
        abi::HOST_FAIL => {
            with(|host| host.failure = Some(read(a0, a1)));
            0
        }
        abi::HOST_RESULT_READ => with(|host| {
            let taken = host.result.len().min(a1 as usize);
            // Safety: as HOST_ARG_READ above -- the caller owns the buffer it
            // described, and off-target it is ordinary memory.
            unsafe { core::ptr::copy_nonoverlapping(host.result.as_ptr(), a0 as *mut u8, taken) };
            host.result.len() as u64
        }),
        // Another program, if the test arranged what it answers. The request is
        // decoded here rather than matched whole so a test names the program and
        // the tool, not a byte sequence.
        abi::HOST_CALL => {
            let request = unsafe { core::slice::from_raw_parts(a0 as *const u8, a1 as usize) };
            let Some(fields) = crate::abi::wire::fields(request) else {
                no_host(number)
            };
            let (program, tool) = match fields.as_slice() {
                [program, tool, ..] => (
                    String::from_utf8_lossy(program).into_owned(),
                    String::from_utf8_lossy(tool).into_owned(),
                ),
                _ => no_host(number),
            };
            let staged = with(|host| {
                host.answers
                    .iter()
                    .find(|(a, b, ..)| *a == program && *b == tool)
                    .map(|(.., outcome, bytes)| (*outcome, bytes.clone()))
            });
            match staged {
                Some((outcome, bytes)) => with(|host| {
                    let length = bytes.len() as u64;
                    host.result = bytes;
                    length | outcome
                }),
                None => panic!(
                    "nothing arranged for {program}.{tool}; call berm_lang::test::answer first"
                ),
            }
        }
        _ => no_host(number),
    }
}

/// Read a `(ptr, len)` pair the caller passed. Off-target these are real
/// addresses in this process.
fn read(ptr: u64, len: u64) -> String {
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

#[cold]
fn no_host(number: u64) -> ! {
    panic!("host call {number:#x} has no stand-in; it needs a real host");
}
