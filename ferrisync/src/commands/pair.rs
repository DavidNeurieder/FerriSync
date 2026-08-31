use anyhow::Context;
use std::net::SocketAddr;

use crate::app::ApplicationContext;

pub async fn run(ctx: &ApplicationContext, ip: &str, port: u16) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{ip}:{port}")
        .parse()
        .with_context(|| format!("invalid address {ip}:{port}"))?;
    println!("Pairing with {addr}...");
    let peer = ctx
        .pairing
        .pair_with(addr)
        .await
        .context("Pairing failed")?;
    println!("Paired with {} ({})", peer.name, peer.id);
    Ok(())
}
