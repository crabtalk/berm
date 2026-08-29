//! What a program said it would reach, against what this service answers to.

use crate::{Service, socket};
use berm_api::Manifest;

impl Service {
    /// The declared dependencies nothing here answers to.
    ///
    /// Reported, never refused. Refusing would make deploy order significant,
    /// and `restore` walks a directory in whatever order the filesystem gives
    /// it — two programs naming each other could then never come up at all.
    pub(crate) fn unresolved(&self, manifest: &Manifest) -> Vec<String> {
        manifest
            .deps
            .iter()
            .filter(|dep| !self.answers_to(dep))
            .cloned()
            .collect()
    }

    /// A scheme means somewhere to dial, which the allowlist decides. Anything
    /// else is a program name, which the deployed set does.
    fn answers_to(&self, dep: &str) -> bool {
        match socket::host(dep) {
            Some(host) => self.policy.allow.iter().any(|allowed| allowed == host),
            None => self.get(dep).is_some(),
        }
    }
}
