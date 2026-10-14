//! Running the SDK's async calls from `crab`'s synchronous commands.

use std::{future::Future, sync::LazyLock};
use tokio::runtime::{Builder, Runtime};

/// One runtime for the process. `crab` stays synchronous because `berm-oci`
/// and `berm-index` use blocking `reqwest`, which panics inside a runtime.
static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime builds")
});

/// Run `future` to completion.
pub fn wait<F: Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}
