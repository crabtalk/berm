//! What a program said it would reach, against what this service answers to.

use crate::Service;
use berm_api::{Deps, Manifest};

impl Service {
    /// The declared dependencies nothing here answers to: programs not
    /// deployed, and syscalls nothing registers. What a served syscall
    /// reaches through it is that syscall's to grant.
    ///
    /// Reported, never refused. Refusing would make deploy order significant,
    /// and `restore` walks a directory in whatever order the filesystem gives
    /// it — two programs naming each other could then never come up at all.
    pub(crate) fn unresolved(&self, manifest: &Manifest) -> Deps {
        let deps = &manifest.deps;
        Deps {
            programs: deps
                .programs
                .iter()
                .filter(|program| self.get(program).is_none())
                .cloned()
                .collect(),
            syscalls: deps
                .syscalls
                .iter()
                .filter(|(name, _)| !self.berm.serves(name))
                .map(|(name, reached)| (name.clone(), reached.clone()))
                .collect(),
        }
    }
}
