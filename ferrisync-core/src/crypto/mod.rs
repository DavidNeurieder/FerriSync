use anyhow::Result;
use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Derive a stable device UUID from a certificate's DER bytes by taking the
/// first 16 bytes of its BLAKE3 hash.
///
/// This is the authoritative identity binding: the device id is a pure
/// function of the public certificate, so a peer cannot claim a different
/// device id than its certificate actually represents.
pub fn cert_to_device_id(cert_der: &[u8]) -> String {
    let hash = blake3::hash(cert_der);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

/// Provides TLS certificate generation and key storage for a device.
#[derive(Debug)]
pub struct CryptoProvider {
    state: Arc<Mutex<CryptoState>>,
}

#[derive(Debug)]
struct CryptoState {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
    cert_fingerprint: Vec<u8>,
}

impl CryptoProvider {
    /// Load an existing certificate and key from raw bytes.
    pub fn load(cert_der: Vec<u8>, key_der: Vec<u8>, fingerprint: Vec<u8>) -> Result<Self> {
        Ok(Self {
            state: Arc::new(Mutex::new(CryptoState {
                cert: CertificateDer::from(cert_der),
                key: PrivateKeyDer::try_from(key_der)
                    .map_err(|e| anyhow::anyhow!("failed to load private key: {e}"))?,
                cert_fingerprint: fingerprint,
            })),
        })
    }

    /// Generate a new self-signed certificate and key pair.
    /// In production, these are persisted and loaded from storage.
    pub fn generate() -> Result<Self> {
        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;

        let params = CertificateParams::new(vec!["FerriSync".to_string()])?;

        let cert = params.self_signed(&key_pair)?;
        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();

        let fp = blake3::hash(&cert_der);
        let fingerprint = fp.as_bytes().to_vec();

        Ok(Self {
            state: Arc::new(Mutex::new(CryptoState {
                cert: CertificateDer::from(cert_der),
                key: PrivateKeyDer::try_from(key_der)
                    .map_err(|e| anyhow::anyhow!("failed to create private key der: {e}"))?,
                cert_fingerprint: fingerprint,
            })),
        })
    }

    pub async fn certificate(&self) -> CertificateDer<'static> {
        self.state.lock().await.cert.clone()
    }

    pub async fn private_key(&self) -> PrivateKeyDer<'static> {
        self.state.lock().await.key.clone_key()
    }

    pub async fn fingerprint(&self) -> Vec<u8> {
        self.state.lock().await.cert_fingerprint.clone()
    }

    pub async fn server_config(&self) -> Result<Arc<rustls::ServerConfig>> {
        let cert = self.certificate().await;
        let key = self.private_key().await;

        let config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(Arc::new(AcceptAnyClientCert))
            .with_single_cert(vec![cert], key)?;

        Ok(Arc::new(config))
    }

    /// Client config that trusts our own cert (for TOFU) and presents
    /// our own certificate for server-side TOFU verification.
    pub async fn client_config(&self) -> Result<Arc<rustls::ClientConfig>> {
        let cert = self.certificate().await;
        let key = self.private_key().await;
        let resolver = Arc::new(SimpleCertResolver { cert, key });
        let config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAllVerifier))
            .with_client_cert_resolver(resolver);

        Ok(Arc::new(config))
    }

    /// Create a client config that trusts specific certificates.
    pub async fn client_config_with_roots(
        certs: Vec<CertificateDer<'static>>,
    ) -> Result<Arc<rustls::ClientConfig>> {
        let mut roots = rustls::RootCertStore::empty();
        for cert in certs {
            roots.add(cert)?;
        }

        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        Ok(Arc::new(config))
    }
}

/// Accepts all server certificates (used for TOFU).
/// Actual verification happens at the protocol layer via fingerprint exchange.
#[derive(Debug)]
struct AcceptAllVerifier;

impl rustls::client::danger::ServerCertVerifier for AcceptAllVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Server-side verifier that accepts any client certificate.
/// Identity is verified at the application layer via TOFU fingerprint check
/// after the TLS handshake completes.
#[derive(Debug)]
struct AcceptAnyClientCert;

impl rustls::server::danger::ClientCertVerifier for AcceptAnyClientCert {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Simple client cert resolver that always presents the same cert/key pair.
#[derive(Debug)]
struct SimpleCertResolver {
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

impl rustls::client::ResolvesClientCert for SimpleCertResolver {
    fn resolve(
        &self,
        _signable_certs: &[&[u8]],
        _schemes: &[rustls::SignatureScheme],
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let cert = rustls::sign::CertifiedKey::new(
            vec![self.cert.clone()],
            rustls::crypto::ring::sign::any_supported_type(&self.key).ok()?,
        );
        Some(Arc::new(cert))
    }

    fn has_certs(&self) -> bool {
        true
    }
}
