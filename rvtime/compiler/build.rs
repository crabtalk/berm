fn main() {
    println!("cargo:rerun-if-changed=csrc/trap.c");

    // Guest memory and trap recovery are POSIX. Without this the build fails
    // inside the C compiler complaining about `sigaction`, which reads like a
    // broken crate rather than an unsupported platform.
    //
    // Windows would need `VirtualAlloc`/`VirtualProtect` and a vectored
    // exception handler. Recovery could not reuse `longjmp`: cranelift-jit
    // never registers unwind tables with the OS, so there is nothing to
    // describe the JIT frames an unwinder would have to walk.
    let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    if !family.split(',').any(|f| f == "unix") {
        panic!(
            "rvtime supports Linux and macOS. Windows is not supported; see \
             https://crabtalk.github.io/rvtime/limitations.html"
        );
    }

    let mut build = cc::Build::new();
    build
        .file("csrc/trap.c")
        .define("_POSIX_C_SOURCE", "200809L");

    // `SA_ONSTACK` and `_setjmp`/`_longjmp` are XSI extensions, which strict
    // POSIX.1-2008 hides. Each libc has its own switch for exposing them, and
    // without the right one glibc fails to compile the shim outright.
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") | Ok("android") => {
            build.define("_GNU_SOURCE", None);
        }
        Ok("macos") | Ok("ios") => {
            build.define("_DARWIN_C_SOURCE", None);
        }
        _ => {}
    }

    build.compile("rvtime_trap");
}
