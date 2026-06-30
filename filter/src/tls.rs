//! TLS termination + re-encryption support (rustls).
//!
//! A port configured with TLS is treated as a TLS service end-to-end: the filter
//! terminates the client handshake with the service's real cert/key
//! ([`build_acceptor`]), runs the decrypted bytes through the normal
//! parse/evaluate/log path, and then opens a **new** TLS connection to the still-
//! TLS upstream ([`build_connector`]). Plaintext exists only inside the filter,
//! exactly where the engine and recorder need it; to a client the connection is
//! indistinguishable from talking to the service directly.
//!
//! There is intentionally no TLS-in/plaintext-out mode: a plaintext service is
//! proxied plaintext→plaintext (no TLS at all), a TLS service is proxied
//! TLS→TLS. The presence of cert/key for a port selects the latter.

use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    CryptoProvider, verify_tls12_signature, verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    ClientConfig, DigitallySignedStruct, Error as TlsError, ServerConfig, SignatureScheme,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Install the process-wide rustls crypto provider exactly once.
///
/// `ServerConfig::builder()` reads the default provider; with the `ring`
/// feature enabled this installs it on first use.
fn ensure_crypto_provider() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

/// Build a [`TlsAcceptor`] from a PEM cert chain and private key file.
pub fn build_acceptor(cert_path: &str, key_path: &str) -> Result<TlsAcceptor> {
    ensure_crypto_provider();

    let certs = load_certs(cert_path)
        .with_context(|| format!("loading TLS certificate from '{cert_path}'"))?;
    let key =
        load_key(key_path).with_context(|| format!("loading TLS private key from '{key_path}'"))?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("TLS certificate and key are not a valid pair")?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Build a [`TlsConnector`] for the filter→upstream (re-encrypt) leg.
///
/// Certificate verification is intentionally **disabled**: the filter→upstream
/// hop is inside the operator's trust boundary (it dials a local/LAN service
/// whose real key it already holds), and that service's certificate is typically
/// self-signed or issued for a public hostname the filter is not connecting by.
/// The handshake signatures are still checked for well-formedness via the crypto
/// provider; only peer identity / chain validation is skipped. A stricter,
/// pluggable policy (cert pinning, full CA verification) can be added later.
pub fn build_connector() -> Result<TlsConnector> {
    ensure_crypto_provider();

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerification::new()))
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

/// A `ServerCertVerifier` that accepts any peer certificate (no identity / chain
/// validation) while still verifying handshake signatures are well-formed. See
/// [`build_connector`] for the rationale.
#[derive(Debug)]
struct NoCertVerification {
    provider: Arc<CryptoProvider>,
}

impl NoCertVerification {
    fn new() -> Self {
        Self {
            provider: Arc::new(tokio_rustls::rustls::crypto::ring::default_provider()),
        }
    }
}

impl ServerCertVerifier for NoCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path)?);
    let certs = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err(anyhow!("no certificates found in PEM file"));
    }
    Ok(certs)
}

fn load_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(File::open(path)?);
    rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| anyhow!("no private key found in PEM file"))
}
