//! Compiling into an object file, and running one
//!
//! The object is the whole artifact: code, the guest ELF it came from, and the
//! settings it was built under. Loading it is a map and a copy, because the
//! only relocations the compiler emits are calls within `.text` -- nothing
//! reaches outside the file, since host calls and indirect jumps go through
//! pointers in `VmCtx` at run time rather than through the linker.

use crate::{
    Engine, Module,
    artifact::{self, Fingerprint, Meta},
    mapping::{Mapping, Patch},
    module::{self, Code},
};
use anyhow::{Context, Result, bail};
use cranelift::module::default_libcall_names;
use cranelift_object::{
    ObjectBuilder, ObjectModule,
    object::{
        SectionKind,
        write::{Object, StandardSection},
    },
};
use object::{Object as _, ObjectSection, ObjectSymbol, RelocationTarget};
use rv::Program;

impl Module {
    /// Compile `program` into a self-contained object file.
    ///
    /// `elf` is the image `program` was decoded from; it travels inside the
    /// artifact so that loading one needs nothing else.
    pub fn object(
        engine: &Engine,
        program: &Program,
        elf: &[u8],
        memory_size: u64,
        interruptible: bool,
    ) -> Result<Vec<u8>> {
        let builder = ObjectBuilder::new(
            engine.isa().clone(),
            "rvtime".to_string(),
            default_libcall_names(),
        )?;
        let mut backend = ObjectModule::new(builder);

        let ids = module::compile(&mut backend, engine, program, memory_size, interruptible)?;
        let product = backend.finish();

        // Every function landed in one `.text`, so a symbol's address is its
        // offset there.
        let offset = |id| -> u64 {
            let symbol = product.function_symbol(id);
            product.object.symbol(symbol).value
        };
        let mut meta = Meta {
            digest: [0; 32],
            fingerprint: fingerprint(engine, memory_size, interruptible),
            trampoline: offset(ids.trampoline),
            relocations: ids.relocations,
            entries: ids
                .functions
                .iter()
                .map(|(addr, id)| (*addr, offset(*id)))
                .collect(),
        };

        let mut object = product.object;
        let text = object.section_id(StandardSection::Text);
        meta.digest = meta.seal(object.section(text).data(), elf);

        let names = names(&object);
        add(&mut object, names.segment, names.elf, elf);
        add(&mut object, names.segment, names.meta, &meta.encode());

        Ok(object.write()?)
    }

    /// Map a previously compiled artifact and make it runnable.
    ///
    /// `memory_size` and `interruptible` are what the caller is about to run
    /// it under, not what it was built with -- an artifact that disagrees is
    /// refused rather than adapted to.
    pub fn load(
        engine: &Engine,
        artifact: &[u8],
        memory_size: u64,
        interruptible: bool,
    ) -> Result<Module> {
        rv::check_memory_size(memory_size)?;

        let file = object::File::parse(artifact).context("failed to read the artifact")?;
        let names = match file.format() {
            object::BinaryFormat::MachO => artifact::MACHO,
            _ => artifact::ELF,
        };

        let meta = Meta::decode(&section(&file, names.meta)?)?;
        let image = section(&file, names.elf)?;
        let text = file
            .section_by_name(names.text)
            .context("the artifact has no code")?;
        let code = text.data().context("the artifact's code is unreadable")?;

        // Integrity before anything is believed, then whether it belongs here.
        if meta.seal(code, &image) != meta.digest {
            bail!("the artifact is damaged: its contents do not match its digest");
        }
        meta.fingerprint
            .check(&fingerprint(engine, memory_size, interruptible))?;

        let program =
            rv::elf::load(&image).context("failed to load the guest image out of the artifact")?;

        let patches = patches(&file, &text)?;
        if patches.len() != meta.relocations as usize {
            bail!(
                "the artifact declares {} call sites and carries {}; it is damaged",
                meta.relocations,
                patches.len()
            );
        }

        let mut mapping = Mapping::new(code)?;
        mapping.relocate(&patches)?;
        mapping.protect()?;

        // Taken before the mapping is handed over; the base outlives both.
        let base = mapping.base();
        let reach = |offset: u64| -> Result<*const u8> {
            if offset as usize >= code.len() {
                bail!("the artifact points at {offset:#x}, past the end of its code");
            }
            Ok(unsafe { base.add(offset as usize) })
        };

        let mut entries = std::collections::BTreeMap::new();
        for (addr, offset) in &meta.entries {
            entries.insert(*addr, reach(*offset)?);
        }
        let trampoline = reach(meta.trampoline)?;

        Ok(Module::assemble(
            Code::Mapped(mapping),
            program,
            entries,
            trampoline,
            memory_size,
            interruptible,
        ))
    }
}

/// What this engine and configuration would produce, for comparison.
fn fingerprint(engine: &Engine, memory_size: u64, interruptible: bool) -> Fingerprint {
    Fingerprint {
        triple: engine.isa().triple().to_string(),
        flags: engine.isa().flags().to_string(),
        memory_size,
        interruptible,
    }
}

fn names(object: &Object<'_>) -> artifact::Names {
    match object.format() {
        cranelift_object::object::BinaryFormat::MachO => artifact::MACHO,
        _ => artifact::ELF,
    }
}

fn add(object: &mut Object<'_>, segment: &[u8], name: &[u8], data: &[u8]) {
    let id = object.add_section(segment.to_vec(), name.to_vec(), SectionKind::ReadOnlyData);
    object.section_mut(id).set_data(data.to_vec(), 8);
}

fn section<'a>(file: &object::File<'a>, name: &[u8]) -> Result<Vec<u8>> {
    let name = std::str::from_utf8(name)?;
    let section = file
        .section_by_name(name)
        .with_context(|| format!("the artifact has no {name} section"))?;
    Ok(section.data()?.to_vec())
}

/// Turn the object's relocations into call sites to patch.
///
/// Anything other than the one call relocation the compiler emits is refused:
/// an unexpected kind means the code reaches outside the file, which is the
/// assumption this whole design rests on.
fn patches(file: &object::File<'_>, text: &object::Section<'_, '_>) -> Result<Vec<Patch>> {
    let base = text.address();
    let mut patches = Vec::new();
    for (at, reloc) in text.relocations() {
        let RelocationTarget::Symbol(index) = reloc.target() else {
            bail!("the artifact relocates against something that is not a symbol");
        };
        let symbol = file.symbol_by_index(index)?;
        if symbol.section_index() != Some(text.index()) {
            bail!(
                "the artifact calls {} from outside its own code",
                symbol.name().unwrap_or("an unnamed symbol")
            );
        }
        let Some(target) = symbol.address().checked_sub(base) else {
            bail!("the artifact places a callee before the code it belongs to");
        };

        patches.push(Patch {
            at,
            target,
            addend: reloc.addend(),
            implicit: reloc.has_implicit_addend(),
        });
    }
    Ok(patches)
}
