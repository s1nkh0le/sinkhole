//! TLS acceptor (server-side) and connector (client-to-upstream) helpers.
//!
//! When `TLS_ENABLED=true` the proxy
//!   1. terminates TLS from clients using the supplied certificate + key,
//!   2. (optionally) re-encrypts to the upstream challenge over TLS.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use tokio_rustls::{TlsAcceptor, TlsConnector};

// ---------------------------------------------------------------------------
// Server side (accept client connections)
// ---------------------------------------------------------------------------

/// Build a `TlsAcceptor` from PEM-encoded certificate chain and private key
/// files on disk.
pub fn build_acceptor(cert_path: &str, key_path: &str) -> Result<TlsAcceptor> {
    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;

    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| anyhow!("TLS server config error: {e}"))?;

    Ok(TlsAcceptor::from(Arc::new(cfg)))
}

// ---------------------------------------------------------------------------
// Client side (connect to upstream)
// ---------------------------------------------------------------------------

/// Build a `TlsConnector` for upstream connections.
///
/// * `skip_verify = true` → accept any upstream certificate (typical for A/D
///   CTF where challenges use self-signed certs).
/// * `skip_verify = false` → standard webpki verification.
pub fn build_connector(skip_verify: bool) -> Result<TlsConnector> {
    let cfg = if skip_verify {
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth()
    } else {
        // Empty root store: only succeeds against publicly-trusted CAs
        // if you bundle webpki-roots; here we keep it minimal.
        let roots = rustls::RootCertStore::empty();
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };

    Ok(TlsConnector::from(Arc::new(cfg)))
}

/// Derive a [`ServerName`] from the upstream host string.
pub fn server_name(host: &str) -> Result<ServerName<'static>> {
    ServerName::try_from(host.to_owned()).map_err(|_| anyhow!("invalid TLS server name: {host}"))
}

// ---------------------------------------------------------------------------
// File helpers
// ---------------------------------------------------------------------------

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let file = std::fs::File::open(path)
        .map_err(|e| anyhow!("cannot open TLS cert file '{}': {}", path, e))?;
    let mut reader = std::io::BufReader::new(file);

    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("failed to parse certs from '{}': {}", path, e))?;

    if certs.is_empty() {
        return Err(anyhow!("no certificates found in '{}'", path));
    }
    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let file = std::fs::File::open(path)
        .map_err(|e| anyhow!("cannot open TLS key file '{}': {}", path, e))?;
    let mut reader = std::io::BufReader::new(file);

    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| anyhow!("failed to parse private key from '{}': {}", path, e))?
        .ok_or_else(|| anyhow!("no private key found in '{}'", path))
}

// ---------------------------------------------------------------------------
// Upstream cert-skip verifier (CTF helper)
// ---------------------------------------------------------------------------

/// A certificate verifier that accepts **any** server certificate.
/// Only used when `TLS_SKIP_VERIFY=true` for upstream CTF challenge
/// connections that typically serve self-signed certs.
#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        use rustls::SignatureScheme::*;
        vec![
            RSA_PKCS1_SHA256,
            RSA_PKCS1_SHA384,
            RSA_PKCS1_SHA512,
            ECDSA_NISTP256_SHA256,
            ECDSA_NISTP384_SHA384,
            ECDSA_NISTP521_SHA512,
            ED25519,
            ED448,
            RSA_PSS_SHA256,
            RSA_PSS_SHA384,
            RSA_PSS_SHA512,
        ]
    }
}
