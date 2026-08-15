fn main() {
    println!("cargo:rerun-if-changed=csrc/trap.c");

    cc::Build::new()
        .file("csrc/trap.c")
        .define("_POSIX_C_SOURCE", "200809L")
        .compile("rvtime_trap");
}
