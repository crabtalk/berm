use anyhow::{Result, bail};
use berm_api::host::{Link, Reply, Used};
use berm_host::Peers;

/// No peers: every program runs here, and no route names one.
pub struct Alone;

impl Peers for Alone {
    async fn link(&self, peer: &str, _: &str, _: &str, _: Vec<u8>, _: Link) -> Result<Reply> {
        bail!("no peer {peer:?}")
    }

    async fn ship(&self, peer: &str, _: &[Used]) -> Result<()> {
        bail!("no peer {peer:?}")
    }
}
