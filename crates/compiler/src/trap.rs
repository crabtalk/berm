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

/// Install the signal handlers for this thread, once.
fn install() {
    INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
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
