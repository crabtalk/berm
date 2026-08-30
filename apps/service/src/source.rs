//! Starting an invocation from something other than a client.
//!
//! A source names a program, a tool and an argument blob, and berm runs a
//! fresh invocation of it — depth 0, its own deadline, nothing inherited from
//! whatever armed the source. What separates it from a client's call is who
//! holds the result: nobody. The outcome is logged where it happens.

use crate::Service;

impl Service {
    /// Run one tool on behalf of a source, logging whatever comes back.
    ///
    /// Infallible by design. A source outlives the program it points at, so a
    /// target undeployed since is one of the things that happens here.
    pub(crate) async fn dispatch(&self, program: &str, tool: &str, args: Vec<u8>) {
        match self.call(program, tool, args).await {
            Ok(Ok(result)) => {
                tracing::debug!(program, tool, bytes = result.len(), "dispatched")
            }
            Ok(Err(failure)) => tracing::warn!(program, tool, "{failure}"),
            Err(error) => tracing::error!(program, tool, "{error:#}"),
        }
    }
}
