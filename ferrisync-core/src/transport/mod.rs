pub mod tcp;

use async_trait::async_trait;
use std::net::SocketAddr;

/// Trait for connecting to a remote peer as a client.
#[async_trait]
pub trait TransportConnector: Send + Sync {
    async fn connect(&self, addr: SocketAddr) -> anyhow::Result<Box<dyn TransportConnection>>;
}

/// Trait for listening and accepting incoming connections as a server.
#[async_trait]
pub trait TransportListener: Send {
    async fn bind(addr: SocketAddr) -> anyhow::Result<Self>
    where
        Self: Sized;
    async fn accept(&mut self) -> anyhow::Result<Box<dyn TransportConnection>>;
}

/// Trait for an established connection to send/receive data.
#[async_trait]
pub trait TransportConnection: Send {
    async fn read(&mut self, buf: &mut [u8]) -> anyhow::Result<usize>;
    async fn write_all(&mut self, buf: &[u8]) -> anyhow::Result<()>;
    async fn close(&mut self) -> anyhow::Result<()>;
}
