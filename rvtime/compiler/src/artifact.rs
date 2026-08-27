//! What an object file has to say beyond its code
//!
//! The guest ELF travels inside the artifact rather than beside it, so a
//! `Program` is still built the one way it has always been built and the two
//! cannot describe different guests.

use anyhow::{Result, bail};

/// Recognises the header before anything in it is believed.
const MAGIC: &[u8; 8] = b"RVTIMEAO";

/// Bumped whenever the layout below changes, so an old artifact is refused
/// rather than misread.
const VERSION: u32 = 1;

/// Where the digest sits in an encoded header, being what the digest itself
/// has to exclude.
const DIGEST: usize = MAGIC.len() + size_of::<u32>();

/// Section names, which the two formats spell differently: Mach-O qualifies a
/// section by segment and caps both at sixteen bytes.
pub(crate) struct Names {
    pub segment: &'static [u8],
    pub elf: &'static [u8],
    pub meta: &'static [u8],
    pub text: &'static str,
}

pub(crate) const ELF: Names = Names {
    segment: b"",
    elf: b".rvtime.elf",
    meta: b".rvtime.meta",
    text: ".text",
};

pub(crate) const MACHO: Names = Names {
    segment: b"__RVTIME",
    elf: b"__elf",
    meta: b"__meta",
    text: "__text",
};

/// What the artifact must agree with the engine about before it can run.
///
/// The address mask and the interrupt checks are compiled into the code, so an
/// artifact run against a different address space size would confine guest
/// addresses to a range other than the one actually reserved. Every field here
/// is compared for equality and none has a fallback: the target and its
/// settings are stored verbatim rather than hashed, so a mismatch is a
/// mismatch and there is no collision to reason about.
#[derive(PartialEq, Eq)]
pub(crate) struct Fingerprint {
    pub triple: String,
    pub flags: String,
    pub memory_size: u64,
    pub interruptible: bool,
}

impl Fingerprint {
    /// Refuse an artifact that was not built for how it is about to be run.
    pub fn check(&self, expected: &Fingerprint) -> Result<()> {
        if self.triple != expected.triple {
            bail!(
                "artifact targets {}, and this host is {}",
                self.triple,
                expected.triple
            );
        }
        if self.memory_size != expected.memory_size {
            bail!(
                "artifact was compiled for a {:#x}-byte address space, and this engine reserves \
                 {:#x}; the mask baked into its code would not match the reservation",
                self.memory_size,
                expected.memory_size
            );
        }
        if self.interruptible != expected.interruptible {
            bail!(
                "artifact was compiled interruptible={}, and this engine wants {}",
                self.interruptible,
                expected.interruptible
            );
        }
        if self.flags != expected.flags {
            bail!("artifact was compiled with different target settings");
        }
        Ok(())
    }
}

/// Everything the artifact says about itself.
pub(crate) struct Meta {
    /// Covers the code, the guest image, and every other field here.
    ///
    /// A half-written or truncated artifact is the realistic damage, and its
    /// code still runs -- into whatever the unpatched calls happen to reach.
    /// Nothing else in the file would notice, so this is what makes damage an
    /// error rather than a wrong answer.
    pub digest: [u8; 32],

    /// What this code was built for, and will only run under.
    pub fingerprint: Fingerprint,

    /// Offset of the host→guest trampoline within `.text`.
    pub trampoline: u64,

    /// How many call sites the code was emitted with.
    ///
    /// Truncating the file drops the relocation table long before it damages
    /// anything the parser objects to, and code whose calls were never patched
    /// still runs -- into the wrong functions. So the count is recorded here
    /// and the loader refuses to proceed unless it finds exactly that many.
    pub relocations: u32,

    /// Guest entry address to its offset within `.text`.
    pub entries: Vec<(u64, u64)>,
}

impl Meta {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.digest);
        out.extend_from_slice(&self.fingerprint.memory_size.to_le_bytes());
        out.extend_from_slice(&self.trampoline.to_le_bytes());
        out.extend_from_slice(&self.relocations.to_le_bytes());
        out.push(self.fingerprint.interruptible as u8);

        for text in [&self.fingerprint.triple, &self.fingerprint.flags] {
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
        }

        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for (addr, offset) in &self.entries {
            out.extend_from_slice(&addr.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
        }
        out
    }

    /// Read a header back, treating every field as untrusted.
    ///
    /// An artifact is a file that other things can truncate or overwrite, so
    /// damage has to surface as an error rather than a panic or, worse, a
    /// plausible-looking table.
    pub fn decode(bytes: &[u8]) -> Result<Meta> {
        let mut r = Reader { bytes, at: 0 };

        if r.take(8)? != MAGIC {
            bail!("not an rvtime artifact");
        }
        let version = r.u32()?;
        if version != VERSION {
            bail!("artifact is version {version}, and this build reads version {VERSION}");
        }

        let digest: [u8; 32] = r.take(32)?.try_into()?;
        let memory_size = r.u64()?;
        let trampoline = r.u64()?;
        let relocations = r.u32()?;
        let interruptible = r.take(1)?[0] != 0;
        let triple = r.text()?;
        let flags = r.text()?;

        let count = r.u32()? as usize;
        let mut entries = Vec::new();
        // Grow as entries are read rather than reserving `count` up front: a
        // corrupt length would otherwise ask for an arbitrary allocation
        // before anything had a chance to reject it.
        for _ in 0..count {
            entries.push((r.u64()?, r.u64()?));
        }

        Ok(Meta {
            digest,
            fingerprint: Fingerprint {
                triple,
                flags,
                memory_size,
                interruptible,
            },
            trampoline,
            relocations,
            entries,
        })
    }

    /// What the digest is taken over: the code, the guest image, and this
    /// header with the digest field itself zeroed.
    pub fn seal(&self, code: &[u8], elf: &[u8]) -> [u8; 32] {
        use sha2::Digest;

        let mut header = self.encode();
        header[DIGEST..DIGEST + 32].fill(0);

        let mut hasher = sha2::Sha256::new();
        hasher.update(code);
        hasher.update(elf);
        hasher.update(header);
        hasher.finalize().into()
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8]> {
        let end = self
            .at
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| anyhow::anyhow!("artifact header is truncated"))?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }

    fn text(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        Ok(String::from_utf8(self.take(len)?.to_vec())?)
    }
}
