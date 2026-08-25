//! Compiling ahead of time, through the embedder-facing API.

#![cfg(feature = "aot")]

use rvtime::{Config, Engine, Linker, Module, Store};

const BASIC: &[u8] = include_bytes!("../../../fixtures/basic.elf");

/// A directory that cleans up after itself, so a failing test leaves nothing.
struct Dir(std::path::PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("rvtime-aot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("creates");
        Dir(path)
    }

    fn files(&self) -> Vec<std::path::PathBuf> {
        let mut files: Vec<_> = std::fs::read_dir(&self.0)
            .expect("reads")
            .map(|entry| entry.expect("entry").path())
            .collect();
        files.sort();
        files
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn add(engine: &Engine, module: &Module, args: (u64, u64)) -> u64 {
    let mut store = Store::new(engine, ());
    let instance = Linker::new(engine)
        .instantiate(&mut store, module)
        .expect("instantiates");
    instance
        .get_typed_func::<(u64, u64), u64>("op_add")
        .expect("op_add")
        .call(&mut store, args)
        .expect("calls")
}

#[test]
fn runs_without_a_directory() {
    // The object is still what runs; it is just not kept.
    let engine = Engine::default();
    let module = Module::new(&engine, BASIC).expect("compiles");
    assert_eq!(add(&engine, &module, (10, 3)), 13);
}

#[test]
fn keeps_an_artifact_and_reuses_it() {
    let dir = Dir::new("reuse");
    let mut config = Config::new();
    config.aot_dir(&dir.0);
    let engine = Engine::new(&config).expect("builds");

    let first = Module::new(&engine, BASIC).expect("compiles");
    let stored = dir.files();
    assert_eq!(stored.len(), 1, "compiling should leave one artifact");

    // Reading the same guest again must reuse that file rather than write a
    // second one, and must answer identically.
    let second = Module::new(&engine, BASIC).expect("loads");
    assert_eq!(dir.files(), stored);
    assert_eq!(
        add(&engine, &first, (10, 3)),
        add(&engine, &second, (10, 3))
    );
    assert_eq!(add(&engine, &second, (10, 3)), 13);
}

#[test]
fn a_different_address_space_is_a_different_artifact() {
    let dir = Dir::new("config");

    for size in [16u64 << 20, 32 << 20] {
        let mut config = Config::new();
        config.aot_dir(&dir.0).memory_size(size);
        let engine = Engine::new(&config).expect("builds");
        let module = Module::new(&engine, BASIC).expect("compiles");
        assert_eq!(add(&engine, &module, (10, 3)), 13);
    }

    // Two files, not one overwritten twice: the address space is baked into
    // the code, so the artifacts are not interchangeable.
    assert_eq!(dir.files().len(), 2);
}

#[test]
fn a_damaged_artifact_is_recompiled() {
    let dir = Dir::new("damaged");
    let mut config = Config::new();
    config.aot_dir(&dir.0);
    let engine = Engine::new(&config).expect("builds");

    Module::new(&engine, BASIC).expect("compiles");
    let stored = dir.files().pop().expect("one artifact");
    std::fs::write(&stored, b"not an object file at all").expect("damages");

    // A damaged file is a miss, not a failure.
    let module = Module::new(&engine, BASIC).expect("recompiles");
    assert_eq!(add(&engine, &module, (10, 3)), 13);
    assert!(
        std::fs::read(&stored).expect("reads").len() > 64,
        "rewritten"
    );
}
