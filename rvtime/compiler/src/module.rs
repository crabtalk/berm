//! Compiling a program into runnable machine code
//!
//! Everything here is backend-agnostic: it drives `cranelift_module::Module`,
//! which is implemented both by the JIT and by the object writer. Where the
//! finished code lands is the only thing the two differ in, so it is the only
//! thing that lives elsewhere -- see [`crate::jit`] and [`crate::aot`].

use crate::Engine;
use anyhow::{Context, Result};
use cranelift::{
    codegen::{
        Context as CodegenContext,
        control::ControlPlane,
        ir::{AbiParam, Function, Signature, UserFuncName},
        isa::{CallConv, TargetIsa},
    },
    module::{FuncId, Linkage, Module as Backend, ModuleReloc},
    prelude::*,
};
use rv::Program;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use translator::{Imports, offsets, params};

/// The name the host→guest trampoline is declared under.
pub(crate) const TRAMPOLINE: &str = "rvtime_enter";

/// A compiled program.
pub struct Module {
    /// Owns the executable pages. `Option` only so [`Drop`] can consume it.
    code: Option<Code>,

    program: Program,

    /// Guest entry address to compiled code.
    entries: BTreeMap<u64, *const u8>,

    /// Indirect-call table, indexed by `(addr - text_base) >> 1`.
    dispatch: Vec<*const u8>,

    /// Host→guest trampoline, `extern "C" fn(*mut VmCtx, *const u8)`.
    trampoline: *const u8,

    /// Size of the address space this code was compiled for.
    ///
    /// The address mask is baked into the generated code, so memory must be
    /// mapped at exactly this size or the confinement would not match the
    /// reservation.
    memory_size: u64,

    /// Whether interrupt checks were compiled in.
    interruptible: bool,
}

/// Whatever owns the pages the entry table points into.
pub(crate) enum Code {
    // Boxed only because it dwarfs the other variant, which would otherwise
    // set the size of every `Module`.
    #[cfg(feature = "jit")]
    Jit(Box<cranelift::jit::JITModule>),

    #[cfg(feature = "aot")]
    Mapped(crate::mapping::Mapping),
}

impl Module {
    /// Assemble a module around code that is already in place.
    ///
    /// Where each function ended up is the one thing the JIT and the object
    /// loader answer differently, so both hand it in already resolved.
    pub(crate) fn assemble(
        code: Code,
        program: Program,
        entries: BTreeMap<u64, *const u8>,
        trampoline: *const u8,
        memory_size: u64,
        interruptible: bool,
    ) -> Module {
        Module {
            code: Some(code),
            dispatch: dispatch_table(&program, &entries),
            program,
            entries,
            trampoline,
            memory_size,
            interruptible,
        }
    }

    /// Whether this code checks for interruption on backward edges.
    pub fn interruptible(&self) -> bool {
        self.interruptible
    }

    /// The address space size this code was compiled for.
    pub fn memory_size(&self) -> u64 {
        self.memory_size
    }

    /// The loaded program.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Compiled code for the function exported as `name`.
    pub fn entry(&self, name: &str) -> Option<*const u8> {
        let addr = self.program.symbols.get(name)?;
        self.entries.get(addr).copied()
    }

    /// Compiled code for the function starting at `addr`.
    pub fn entry_at(&self, addr: u64) -> Option<*const u8> {
        self.entries.get(&addr).copied()
    }

    /// The host→guest trampoline.
    pub fn trampoline(&self) -> *const u8 {
        self.trampoline
    }

    /// The indirect-call dispatch table.
    pub fn dispatch(&self) -> &[*const u8] {
        &self.dispatch
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        match self.code.take() {
            // Safe because every pointer into these pages -- the entry table,
            // the dispatch table, the trampoline -- is owned by this `Module`
            // and becomes unreachable at the same moment.
            #[cfg(feature = "jit")]
            Some(Code::Jit(jit)) => unsafe { (*jit).free_memory() },

            // A mapping unmaps itself.
            #[cfg(feature = "aot")]
            Some(Code::Mapped(mapping)) => drop(mapping),

            None => {}
        }
    }
}

// The pointers refer to code owned by this module.
unsafe impl Send for Module {}

// Nothing is mutated after the module is assembled: the entry table, dispatch
// table and trampoline are read-only, and the backend is only touched again in
// `Drop`, which needs unique ownership anyway.
unsafe impl Sync for Module {}

/// What a compilation produced, so its code can be found afterwards.
pub(crate) struct Compiled {
    /// Guest entry address to declared function.
    pub functions: BTreeMap<u64, FuncId>,

    /// The host→guest trampoline.
    pub trampoline: FuncId,

    /// Call sites the code contains, all of them within its own `.text`.
    #[cfg_attr(not(feature = "aot"), allow(dead_code))]
    pub relocations: u32,
}

/// Translate and define every function in `program`, plus the trampoline.
///
/// Both backends go through here, so the code they produce is identical and
/// there is no second translation path to keep correct.
///
/// `incremental` is the per-function cache, which only the JIT wants: the
/// object backend caches the whole artifact, and entries for the functions
/// inside it would be a second copy of the same code that nothing reads back.
pub(crate) fn compile(
    backend: &mut dyn Backend,
    engine: &Engine,
    program: &Program,
    memory_size: u64,
    interruptible: bool,
    incremental: Option<&Arc<crate::Cache>>,
) -> Result<Compiled> {
    rv::check_memory_size(memory_size)?;

    let signature = translator::signature();
    let functions = declare(backend, program, &signature)?;
    let trampoline = declare_trampoline(backend, engine.isa().as_ref())?;

    let mut relocations = define(
        backend,
        engine,
        program,
        &functions,
        memory_size,
        interruptible,
        incremental,
    )?;
    relocations += define_trampoline(backend, engine, trampoline, &signature, incremental)?;

    Ok(Compiled {
        functions,
        trampoline,
        relocations,
    })
}

/// Declare every guest function so calls between them can be linked.
fn declare(
    backend: &mut dyn Backend,
    program: &Program,
    signature: &Signature,
) -> Result<BTreeMap<u64, FuncId>> {
    let mut ids = BTreeMap::new();
    for (addr, function) in &program.functions {
        // Guest symbol names are not guaranteed unique, so the address makes
        // the linker name unambiguous while staying readable in a dump.
        let name = format!("{}@{addr:x}", function.name);
        let id = backend.declare_function(&name, Linkage::Local, signature)?;
        ids.insert(*addr, id);
    }
    Ok(ids)
}

/// Translate and define every guest function.
fn define(
    backend: &mut dyn Backend,
    engine: &Engine,
    program: &Program,
    ids: &BTreeMap<u64, FuncId>,
    memory_size: u64,
    interruptible: bool,
    incremental: Option<&Arc<crate::Cache>>,
) -> Result<u32> {
    let signature = translator::signature();
    let frontend = engine.isa().frontend_config();
    let host = host_signature(engine.isa().as_ref());
    let mut fctx = FunctionBuilderContext::new();
    let entries: std::collections::BTreeSet<u64> = program.functions.keys().copied().collect();
    let mut relocations = 0;

    for (addr, function) in &program.functions {
        let analysis = translator::analyze(function, &entries, &program.indirect);

        let mut ctx = CodegenContext::new();
        ctx.func = Function::with_name_signature(
            UserFuncName::user(0, ids[addr].as_u32()),
            signature.clone(),
        );

        // Callees must be imported before the builder borrows the function.
        let mut calls = HashMap::new();
        for target in &analysis.calls {
            let id = ids.get(target).with_context(|| {
                format!(
                    "{} calls {target:#x}, which is not a known function entry",
                    function.name
                )
            })?;
            calls.insert(*target, backend.declare_func_in_func(*id, &mut ctx.func));
        }

        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fctx);
        let imports = Imports {
            calls: &calls,
            indirect: builder.import_signature(signature.clone()),
            host: builder.import_signature(host.clone()),
            memory_mask: (memory_size - 1) as i64,
            interruptible,
        };

        translator::translate(function, &analysis, &imports, builder, frontend)
            .with_context(|| format!("failed to translate {}", function.name))?;

        relocations += emit(backend, engine, &mut ctx, ids[addr], incremental)
            .with_context(|| format!("failed to compile {}", function.name))?;
    }

    Ok(relocations)
}

/// Generate code for one function and hand it to the backend, reporting how
/// many call sites it contains.
///
/// With a cache configured this goes through Cranelift's incremental cache,
/// which is keyed on the CLIF contents and the ISA settings — so a function
/// already compiled in a previous run is deserialised rather than generated
/// again. Code generation is ~99% of compile time, so this is where the whole
/// saving is.
fn emit(
    backend: &mut dyn Backend,
    engine: &Engine,
    ctx: &mut CodegenContext,
    id: FuncId,
    incremental: Option<&Arc<crate::Cache>>,
) -> Result<u32> {
    let isa = engine.isa().as_ref();
    let mut control = ControlPlane::default();

    // Take owned copies while the compilation result borrows `ctx`, so the
    // relocations can be resolved against `ctx.func` afterwards.
    let (code, mach_relocs) = {
        let compiled = match incremental {
            // `compile_with_cache` needs a `&mut dyn CacheKvStore`, but the
            // cache is shared behind an `Arc` so several modules compiled from
            // one engine accumulate into it. Its interior state is atomic and
            // the filesystem arbitrates the entries themselves.
            Some(cache) => {
                let mut store = crate::cache::Handle(cache.clone());
                ctx.compile_with_cache(isa, &mut store, &mut control)
                    .map_err(|e| anyhow::anyhow!("{}", e.inner))?
                    .0
            }
            None => ctx
                .compile(isa, &mut control)
                .map_err(|e| anyhow::anyhow!("{}", e.inner))?,
        };
        (
            compiled.code_buffer().to_vec(),
            compiled.buffer.relocs().to_vec(),
        )
    };

    let relocs: Vec<ModuleReloc> = mach_relocs
        .iter()
        .map(|reloc| ModuleReloc::from_mach_reloc(reloc, &ctx.func, id))
        .collect();

    backend.define_function_bytes(id, 1, &code, &relocs)?;
    Ok(relocs.len() as u32)
}

/// The signature of a host call: `extern "C" fn(*mut VmCtx) -> u64`.
///
/// The result is a status, zero meaning the call succeeded.
fn host_signature(isa: &dyn TargetIsa) -> Signature {
    Signature {
        params: vec![AbiParam::new(types::I64)],
        returns: vec![AbiParam::new(types::I64)],
        call_conv: CallConv::triple_default(isa.triple()),
    }
}

/// The signature of the trampoline: `extern "C" fn(*mut VmCtx, *const u8)`.
fn trampoline_signature(isa: &dyn TargetIsa) -> Signature {
    Signature {
        params: vec![AbiParam::new(types::I64); 2],
        returns: vec![],
        call_conv: CallConv::triple_default(isa.triple()),
    }
}

fn declare_trampoline(backend: &mut dyn Backend, isa: &dyn TargetIsa) -> Result<FuncId> {
    let signature = trampoline_signature(isa);
    Ok(backend.declare_function(TRAMPOLINE, Linkage::Export, &signature)?)
}

/// Emit the bridge from the host's C ABI into the guest convention.
///
/// Guest functions take ten arguments and return three in Cranelift's `Fast`
/// convention, which the host cannot call directly. The trampoline reads the
/// live registers out of the VM context, performs the call, and writes the
/// results back, so the host only ever deals with `VmCtx`.
fn define_trampoline(
    backend: &mut dyn Backend,
    engine: &Engine,
    id: FuncId,
    guest: &Signature,
    incremental: Option<&Arc<crate::Cache>>,
) -> Result<u32> {
    let isa = engine.isa().as_ref();
    let mut ctx = CodegenContext::new();
    ctx.func = Function::with_name_signature(
        UserFuncName::user(0, id.as_u32()),
        trampoline_signature(isa),
    );

    let mut fctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fctx);
    let guest_sig = builder.import_signature(guest.clone());

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);

    let args = builder.block_params(entry).to_vec();
    let (vmctx, callee) = (args[0], args[1]);

    let mut call_args = Vec::with_capacity(params::COUNT);
    call_args.push(vmctx);
    call_args.push(builder.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        vmctx,
        offsets::reg(rv::Reg::SP.index()),
    ));
    for arg in 0..params::ARGS {
        call_args.push(builder.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            vmctx,
            offsets::reg(rv::Reg::A0.index() + arg),
        ));
    }

    let call = builder.ins().call_indirect(guest_sig, callee, &call_args);
    let results = builder.inst_results(call).to_vec();

    for (value, reg) in results.iter().zip([rv::Reg::A0, rv::Reg::A1]) {
        builder.ins().store(
            MemFlagsData::trusted(),
            *value,
            vmctx,
            offsets::reg(reg.index()),
        );
    }

    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize(isa.frontend_config());

    emit(backend, engine, &mut ctx, id, incremental)
        .context("failed to compile the guest entry trampoline")
}

/// Build the indirect-call table.
///
/// One slot per two bytes of `.text`, because RISC-V instructions are
/// two-byte aligned. Slots for addresses that are not function entries stay
/// null, and compiled code traps on them -- that null check is what stops a
/// corrupted function pointer from becoming an arbitrary jump.
fn dispatch_table(program: &Program, entries: &BTreeMap<u64, *const u8>) -> Vec<*const u8> {
    let span = program.text.end - program.text.start;
    let mut table = vec![std::ptr::null(); (span / 2) as usize + 1];

    for addr in &program.indirect {
        if let Some(code) = entries.get(addr) {
            let index = ((addr - program.text.start) / 2) as usize;
            table[index] = *code;
        }
    }

    table
}
