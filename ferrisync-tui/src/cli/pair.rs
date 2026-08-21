use anyhow::Context;
use ferrisync_core::sync_engine::pairing::PairingManager;
use std::net::SocketAddr;

pub async fn run(ip: String, port: u16, pairing: &PairingManager) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{ip}:{port}")
        .parse()
        .with_context(|| format!("invalid address {ip}:{port}"))?;
    println!("Pairing with {addr}...");
    let peer = pairing.pair_with(addr).await.context("Pairing failed")?;
    println!("Paired with {} ({})", peer.name, peer.id);
    Ok(())
}
