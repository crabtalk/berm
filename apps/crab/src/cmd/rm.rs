//! `crab rm` — take a program out of service.

use crate::utils;
use anyhow::Result;
use crabtalk_sdk::client::Client;

pub fn run(client: &Client, name: &str) -> Result<()> {
    utils::wait(client.undeploy(name))?;
    println!("removed {name}");
    Ok(())
}
