use ferrisync_core::sync_engine::pairing::PairingManager;
use std::net::SocketAddr;

pub async fn run(ip: String, port: u16, pairing: &PairingManager) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{ip}:{port}").parse()?;
    println!("Pairing with {addr}...");
    match pairing.pair_with(addr).await {
        Ok(peer) => println!("Paired with {} ({})", peer.name, peer.id),
        Err(e) => {
            eprintln!("Pairing failed: {e}");
            std::process::exit(1);
        }
    }
    Ok(())
}
