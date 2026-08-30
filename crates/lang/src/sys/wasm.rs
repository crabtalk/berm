//! WebAssembly, where a host call is an import.

// Every syscall arrives through this one import, carrying the number its name
// hashes to — the same number rvtime takes in `a7`.
#[link(wasm_import_module = "berm")]
unsafe extern "C" {
    #[link_name = "syscall"]
    fn syscall(number: u64, ptr: u32, len: u32) -> u64;
}

#[inline]
pub fn call0(number: u64) -> u64 {
    unsafe { syscall(number, 0, 0) }
}

#[inline]
pub fn call2(number: u64, a0: u64, a1: u64) -> u64 {
    unsafe { syscall(number, a0 as u32, a1 as u32) }
}

#[inline]
pub fn abort() -> ! {
    core::arch::wasm32::unreachable()
}
