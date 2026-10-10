//! What a program said it would reach, against what this service answers to.

use crate::Service;
use berm::Program;
use berm_api::Deps;

impl Service {
    /// The declared dependencies nothing here answers to: programs not
    /// deployed, syscalls nothing registers, secrets `secrets.toml` does not
    /// grant the program, and subscriptions to programs not deployed. What a served syscall reaches through it is that
    /// syscall's to grant; an unreadable `secrets.toml` grants nothing.
    ///
    /// Reported, never refused. Refusing would make deploy order significant,
    /// and `restore` walks a directory in whatever order the filesystem gives
    /// it — two programs naming each other could then never come up at all.
    pub(crate) fn unresolved(&self, program: &Program) -> Deps {
        let deps = &program.manifest().deps;
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
            secrets: {
                let granted = self.secrets.names(&program.name).unwrap_or_default();
                deps.secrets
                    .iter()
                    .filter(|name| !granted.contains(name))
                    .cloned()
                    .collect()
            },
            logs: deps
                .logs
                .iter()
                .filter(|logs| self.get(&logs.program).is_none())
                .cloned()
                .collect(),
        }
    }
}
