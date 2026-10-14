//! What a program said it would reach, against what this service answers to.

use crate::Service;
use berm::Program;
use berm_api::Deps;

impl Service {
    /// The declared dependencies nothing here answers to: programs not
    /// deployed, secrets `secrets.toml` does not grant the program, and
    /// subscriptions to programs not deployed. An unreadable `secrets.toml`
    /// grants nothing.
    pub(crate) fn unresolved(&self, program: &Program) -> Deps {
        let deps = &program.manifest().deps;
        Deps {
            programs: deps
                .programs
                .iter()
                .filter(|program| self.get(program).is_none())
                .cloned()
                .collect(),
            syscalls: Default::default(),
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
