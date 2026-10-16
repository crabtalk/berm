use crate::{Alone, Fetch, Grants, Stored};
use berm_host::{Env, System};

/// One machine, running every program itself.
pub struct Local;

impl Env for Local {
    type Fetch = Fetch;
    type Images = Stored;
    type Peers = Alone;
    type Secrets = Grants;
    type Observe = ();
    type Clock = System;
}
