use super::{TransportConnection, TransportConnector, TransportListener};
use crate::crypto::CryptoProvider;
use anyhow::Result;
use async_trait::async_trait;
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// TCP + TLS 1.3 transport implementation.
pub struct TcpTransport {
    crypto: Arc<CryptoProvider>,
}

impl TcpTransport {
    pub fn new(crypto: Arc<CryptoProvider>) -> Self {
        Self { crypto }
    }
}

#[async_trait]
impl TransportConnector for TcpTransport {
    async fn connect(&self, addr: SocketAddr) -> Result<Box<dyn TransportConnection>> {
        let tcp = TcpStream::connect(addr).await?;
        let config = self.crypto.client_config().await?;
        let connector = TlsConnector::from(config);
        let name = ServerName::try_from(addr.ip().to_string())?;
        let tls = connector.connect(name, tcp).await?;
        Ok(Box::new(TcpConnection {
            inner: Arc::new(Mutex::new(tokio_rustls::TlsStream::Client(tls))),
        }))
    }
}

#[async_trait]
impl TransportListener for TcpTransport {
    async fn bind(_addr: SocketAddr) -> Result<Self>
    where
        Self: Sized,
    {
        let crypto = CryptoProvider::generate()?;
        Ok(Self {
            crypto: Arc::new(crypto),
        })
    }

    async fn accept(&mut self) -> Result<Box<dyn TransportConnection>> {
        let (tcp, _) = TcpListener::bind("0.0.0.0:0").await?.accept().await?;
        let config = self.crypto.server_config().await?;
        let acceptor = TlsAcceptor::from(config);
        let tls = acceptor.accept(tcp).await?;
        Ok(Box::new(TcpConnection {
            inner: Arc::new(Mutex::new(tokio_rustls::TlsStream::Server(tls))),
        }))
    }
}

pub struct TcpConnection {
    inner: Arc<Mutex<tokio_rustls::TlsStream<TcpStream>>>,
}

#[async_trait]
impl TransportConnection for TcpConnection {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        use tokio::io::AsyncReadExt;
        let mut inner = self.inner.lock().await;
        Ok(inner.read(buf).await?)
    }

    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut inner = self.inner.lock().await;
        Ok(inner.write_all(buf).await?)
    }

    async fn close(&mut self) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        let mut inner = self.inner.lock().await;
        Ok(inner.shutdown().await?)
    }
}
