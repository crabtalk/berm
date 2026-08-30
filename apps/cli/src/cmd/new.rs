//! `berm new` — scaffold a program.

use anyhow::{Context, Result, bail};
use std::{fs, path::Path};

/// The SDK a scaffold depends on. Taken from this binary so a program is
/// generated against the `berm-lang` that shipped with the CLI that wrote it.
const SDK: &str = env!("CARGO_PKG_VERSION");

/// Each template beside the path it is written to. A crate name is a valid
/// Rust identifier, so the placeholders are too — which is what keeps these
/// files parsing as the languages they are.
const FILES: [(&str, &str); 4] = [
    ("Cargo.toml", include_str!("../../templates/manifest.toml")),
    ("src/lib.rs", include_str!("../../templates/lib.rs")),
    ("src/bin/main.rs", include_str!("../../templates/main.rs")),
    ("tests/tools.rs", include_str!("../../templates/tools.rs")),
];

/// What a program is built for.
///
/// The same source builds for either; nothing an author writes changes.
/// RISC-V is experimental, and needs a linker flag wasm does not, which is the
/// whole of what `cargo_config` carries.
#[derive(Clone, Copy, Default, clap::ValueEnum)]
pub enum Target {
    #[default]
    Wasm,
    Riscv,
}

impl Target {
    fn triple(self) -> &'static str {
        match self {
            Self::Wasm => "wasm32-unknown-unknown",
            Self::Riscv => "riscv64imac-unknown-none-elf",
        }
    }

    /// Where cargo leaves the image, under the target directory.
    fn image(self, name: &str) -> String {
        match self {
            Self::Wasm => format!("{}/release/{name}.wasm", self.triple()),
            Self::Riscv => format!("{}/release/{name}", self.triple()),
        }
    }

    /// What the crate needs in `.cargo/config.toml`, if anything.
    fn cargo_config(self) -> Option<&'static str> {
        match self {
            Self::Wasm => None,
            // Not optional: the relocations are what identify indirect-call
            // targets. Set per target rather than as `build.target`, so
            // `cargo test` still runs natively.
            Self::Riscv => Some(concat!(
                "[target.riscv64imac-unknown-none-elf]\n",
                "rustflags = [\"-Clink-arg=--emit-relocs\"]\n",
            )),
        }
    }
}

pub fn run(name: &str, target: Target) -> Result<()> {
    check(name)?;

    let root = Path::new(name);
    if root.exists() {
        bail!("{name} already exists");
    }

    let krate = name.replace('-', "_");
    for (path, template) in FILES {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let contents = template
            .replace("__NAME__", name)
            .replace("__CRATE__", &krate)
            .replace("__VERSION__", SDK);
        fs::write(&path, contents).with_context(|| format!("cannot write {}", path.display()))?;
    }

    if let Some(config) = target.cargo_config() {
        let path = root.join(".cargo/config.toml");
        fs::create_dir_all(path.parent().expect("a parent"))?;
        fs::write(&path, config).with_context(|| format!("cannot write {}", path.display()))?;
    }

    println!("created {name}/");
    println!();
    println!("  cd {name}");
    println!("  cargo test");
    println!("  cargo build --release --target {}", target.triple());
    println!("  berm deploy {name} target/{}", target.image(name));
    Ok(())
}

/// A program name is a crate name, and cargo's rules are the ones that bite —
/// better here than three lines into a build.
fn check(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("a program needs a name");
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        bail!("{name:?} starts with a digit, which a crate name cannot");
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && *c != '-' && *c != '_')
    {
        bail!("{name:?} contains {bad:?}; a crate name takes letters, digits, `-` and `_`");
    }
    Ok(())
}
