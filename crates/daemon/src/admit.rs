use berm_api::host::Caller;
use berm_host::{Admit, Denied};

/// Every call from outside is admitted.
pub struct Open;

impl Admit for Open {
    async fn admit(&self, _: &str, _: &str, _: bool, _: Option<&Caller>) -> Result<(), Denied> {
        Ok(())
    }
}
