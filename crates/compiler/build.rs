fn main() {
    println!("cargo:rerun-if-changed=csrc/trap.c");

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
