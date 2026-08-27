//! Guest fault recovery
//!
//! Out-of-bounds guest accesses land on guard pages rather than explicit bounds
//! checks, so recovering from one means catching a signal raised inside
//! JIT-compiled code and unwinding out of it. See `csrc/trap.c`.

use core::ffi::c_void;
use std::{cell::Cell, ptr};

unsafe extern "C" {
    fn rvtime_protect(
        body: extern "C" fn(*mut c_void) -> bool,
        payload: *mut c_void,
        slot: *mut *mut c_void,
    ) -> bool;
    fn rvtime_unwind(landing_pad: *mut c_void) -> !;
    fn rvtime_install(handler: extern "C" fn(i32, *mut libc::siginfo_t, *mut c_void)) -> i32;
}

thread_local! {
    /// The innermost `rvtime_protect` frame, or null outside the guest.
    static LANDING_PAD: Cell<*mut c_void> = const { Cell::new(ptr::null_mut()) };
    /// Where the fault happened, recorded by the handler before unwinding.
    static FAULT: Cell<Option<Fault>> = const { Cell::new(None) };
    /// Signal handlers are per-process, but installation is tracked per thread.
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Base and size of this thread's guest memory, for turning a host fault
    /// address back into a guest one.
    static GUEST_REGION: Cell<(usize, u64)> = const { Cell::new((0, 0)) };
}

/// A guest memory fault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    /// The signal number, `SIGSEGV` or `SIGBUS`.
    pub signal: i32,

    /// The faulting address translated back into the guest address space, or
    /// `None` if it fell outside the reserved window.
    pub guest: Option<u64>,

    /// The raw host address, for diagnostics.
    pub host: usize,
}

/// Record the guest memory region so faults can be reported in guest terms.
///
/// Called when a guest is entered on this thread.
pub fn set_guest_region(base: usize, size: u64) {
    GUEST_REGION.with(|region| region.set((base, size)));
}

/// Run `body` under fault protection, with faults reported against this
/// guest's region.
///
/// The region is restored on the way out, which is what makes a guest entered
/// from inside another guest's host call work: the inner entry would otherwise
/// leave its own region behind, and the outer guest's next fault would be
/// translated against memory it does not own. `rvtime_protect` already saves
/// and restores the landing pad for the same reason.
pub fn protect_guest<F, T>(base: usize, size: u64, body: F) -> Result<T, Fault>
where
    F: FnOnce() -> T,
{
    let saved = GUEST_REGION.with(|region| region.get());
    set_guest_region(base, size);
    let outcome = protect(body);
    GUEST_REGION.with(|region| region.set(saved));
    outcome
}

/// Run `body`, catching any guest memory fault it raises.
pub fn protect<F, T>(body: F) -> Result<T, Fault>
where
    F: FnOnce() -> T,
{
    install();
    FAULT.with(|f| f.set(None));

    // The closure and its result are passed through a single pointer so the C
    // side stays free of Rust generics.
    struct Payload<F, T> {
        body: Option<F>,
        result: Option<T>,
    }

    extern "C" fn run<F, T>(payload: *mut c_void) -> bool
    where
        F: FnOnce() -> T,
    {
        let payload = unsafe { &mut *(payload as *mut Payload<F, T>) };
        let Some(body) = payload.body.take() else {
            return false;
        };
        payload.result = Some(body());
        true
    }

    let mut payload = Payload {
        body: Some(body),
        result: None,
    };

    let completed = LANDING_PAD.with(|slot| unsafe {
        rvtime_protect(run::<F, T>, &raw mut payload as *mut c_void, slot.as_ptr())
    });

    match (completed, payload.result) {
        (true, Some(result)) => Ok(result),
        _ => Err(FAULT.with(|f| f.get()).unwrap_or(Fault {
            signal: libc::SIGSEGV,
            guest: None,
            host: 0,
        })),
    }
}

/// How much stack the handler is given, under a guard page. It records a fault
/// and unwinds and does nothing else, so this is wasmtime's size rather than a
/// measured one.
const ALT_STACK: usize = 64 * 1024;

/// The alternate stack this thread's handler runs on, mapped for as long as the
/// thread lives.
///
/// `SA_ONSTACK` names a stack that has to exist. Without one the handler runs
/// on the stack of the thread that faulted, which for the one fault that most
/// needs catching — running off the end of it — is the stack that just ran out.
struct Alternate {
    base: *mut c_void,
    len: usize,
    previous: libc::stack_t,
}

impl Drop for Alternate {
    fn drop(&mut self) {
        // Put back whatever was there before unmapping ours, so nothing is left
        // pointing at memory this is about to return.
        unsafe {
            libc::sigaltstack(&raw const self.previous, ptr::null_mut());
            libc::munmap(self.base, self.len);
        }
    }
}

thread_local! {
    static ALTERNATE: Cell<Option<Alternate>> = const { Cell::new(None) };
}

/// Give this thread an alternate stack, unless it already has a usable one.
///
/// A runtime that installed its own is left alone: replacing it would strand
/// whatever it meant to catch.
fn alternate() -> Option<Alternate> {
    let mut previous: libc::stack_t = unsafe { std::mem::zeroed() };
    if unsafe { libc::sigaltstack(ptr::null(), &raw mut previous) } != 0 {
        return None;
    }
    if previous.ss_flags & libc::SS_DISABLE == 0 && previous.ss_size >= ALT_STACK {
        return None;
    }

    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
    let len = page + ALT_STACK;
    let base = unsafe {
        libc::mmap(
            ptr::null_mut(),
            len,
            libc::PROT_NONE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    if base == libc::MAP_FAILED {
        return None;
    }

    // The lowest page stays unreadable: a stack grows down, so overrunning this
    // one faults rather than writing into whatever was mapped below it.
    let stack = unsafe { base.add(page) };
    if unsafe { libc::mprotect(stack, ALT_STACK, libc::PROT_READ | libc::PROT_WRITE) } != 0 {
        unsafe { libc::munmap(base, len) };
        return None;
    }

    let ours = libc::stack_t {
        ss_sp: stack,
        ss_size: ALT_STACK,
        ss_flags: 0,
    };
    if unsafe { libc::sigaltstack(&raw const ours, ptr::null_mut()) } != 0 {
        unsafe { libc::munmap(base, len) };
        return None;
    }

    Some(Alternate {
        base,
        len,
        previous,
    })
}

/// Install the signal handlers for this thread, once.
fn install() {
    INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        ALTERNATE.with(|slot| slot.set(alternate()));
        let code = unsafe { rvtime_install(handler) };
        assert_eq!(code, 0, "failed to install guest fault handlers: {code}");
        installed.set(true);
    });
}

/// Record the fault and unwind to the innermost [`protect`].
///
/// If there is no landing pad the fault did not come from guest code, so the
/// handler must not swallow it: restore the default action and let it through.
extern "C" fn handler(signal: i32, info: *mut libc::siginfo_t, _context: *mut c_void) {
    let landing_pad = LANDING_PAD.with(|slot| slot.get());
    if landing_pad.is_null() {
        unsafe {
            libc::signal(signal, libc::SIG_DFL);
        }
        return;
    }

    let host = unsafe { (*info).si_addr() } as usize;
    let (base, size) = GUEST_REGION.with(|region| region.get());
    let guest = host
        .checked_sub(base)
        .map(|offset| offset as u64)
        .filter(|offset| *offset < size);

    FAULT.with(|f| {
        f.set(Some(Fault {
            signal,
            guest,
            host,
        }))
    });

    unsafe { rvtime_unwind(landing_pad) }
}
