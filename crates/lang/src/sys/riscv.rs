//! RV64, where a host call is an `ecall`.

#[inline]
pub fn call0(number: u64) -> u64 {
    unsafe { guest::call0(number) }
}

#[inline]
pub fn call2(number: u64, a0: u64, a1: u64) -> u64 {
    unsafe { guest::call2(number, a0, a1) }
}

#[inline]
pub fn abort() -> ! {
    guest::abort()
}
