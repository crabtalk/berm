//! `berm rm` — take a harness out of service.

use crate::Client;
use anyhow::Result;

pub fn run(client: &Client, name: &str) -> Result<()> {
    client.undeploy(name)?;
    println!("removed {name}");
    Ok(())
}
