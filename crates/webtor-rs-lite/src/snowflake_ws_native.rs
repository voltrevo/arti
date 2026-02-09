//! Native WebSocket-based Snowflake transport
//!
//! This module provides Snowflake connectivity using WebSocket for native (non-WASM) builds.
//! It uses the same protocol stack as the WASM version but with native TLS via rustls.
//!
//! Protocol stack (bottom to top):
//!   WebSocket (wss://snowflake.torproject.net/)
//!       ↓
//!   Turbo (framing + obfuscation)
//!       ↓
//!   KCP (reliability + ordering)
//!       ↓
//!   SMUX (stream multiplexing)
//!       ↓
//!   TLS (link encryption via rustls)
//!       ↓
//!   Tor protocol

#![cfg(not(target_arch = "wasm32"))]

use crate::error::{Result, TorError};
use crate::kcp_stream::{KcpConfig, KcpStream};
use crate::smux::SmuxStream;
use crate::turbo::TurboStream;
use crate::websocket::WebSocketStream;
use futures::{AsyncRead, AsyncWrite};
use std::borrow::Cow;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tracing::info;

use futures_rustls::rustls::client::danger;
use futures_rustls::rustls::crypto::{
    verify_tls12_signature, verify_tls13_signature, CryptoProvider, WebPkiSupportedAlgorithms,
};
use futures_rustls::rustls::pki_types::{CertificateDer, ServerName};
use futures_rustls::rustls::{self, CertificateError, Error as TLSError};
use futures_rustls::TlsConnector;
use webpki::EndEntityCert;

/// WebSocket Snowflake endpoints
/// Note: The Tor Project bridge (snowflake.torproject.net) rejects non-browser clients
/// Use the PSE bridge instead which accepts native clients
pub const SNOWFLAKE_WS_URL: &str = "wss://snowflake.pse.dev/";
pub const SNOWFLAKE_WS_URL_TOR_PROJECT: &str = "wss://snowflake.torproject.net/";

/// Snowflake bridge fingerprint for PSE bridge
pub const SNOWFLAKE_FINGERPRINT: &str = "664A92FF3EF71E03A2F09B1DAABA2DDF920D5194";

/// WebSocket Snowflake configuration
#[derive(Debug, Clone)]
pub struct SnowflakeWsConfig {
    /// WebSocket URL for Snowflake endpoint
    pub ws_url: String,
    /// Bridge fingerprint
    pub fingerprint: String,
    /// KCP conversation ID (0 for default)
    pub kcp_conv: u32,
    /// SMUX stream ID (default: 3)
    pub smux_stream_id: u32,
}

impl Default for SnowflakeWsConfig {
    fn default() -> Self {
        Self {
            ws_url: SNOWFLAKE_WS_URL.to_string(),
            fingerprint: SNOWFLAKE_FINGERPRINT.to_string(),
            kcp_conv: 0,
            smux_stream_id: 3,
        }
    }
}

impl SnowflakeWsConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.ws_url = url.to_string();
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: &str) -> Self {
        self.fingerprint = fingerprint.to_string();
        self
    }
}

/// Custom certificate verifier that skips PKI validation
/// (Tor validates via CERTS cells in the protocol layer)
#[derive(Clone, Debug)]
struct TorCertVerifier(WebPkiSupportedAlgorithms);

impl danger::ServerCertVerifier for TorCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer,
        _roots: &[CertificateDer],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<danger::ServerCertVerified, TLSError> {
        // Just check that the certificate is well-formed
        // Real authentication happens in the Tor handshake via CERTS cells
        let _cert: EndEntityCert<'_> = end_entity
            .try_into()
            .map_err(|_| TLSError::InvalidCertificate(CertificateError::BadEncoding))?;

        Ok(danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<danger::HandshakeSignatureValid, TLSError> {
        verify_tls12_signature(message, cert, dss, &self.0)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<danger::HandshakeSignatureValid, TLSError> {
        verify_tls13_signature(message, cert, dss, &self.0)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.supported_schemes()
    }

    fn root_hint_subjects(&self) -> Option<&[rustls::DistinguishedName]> {
        None
    }
}

/// Create a TLS connector that skips certificate verification (for Tor)
fn create_tor_tls_connector() -> Result<TlsConnector> {
    // Ensure crypto provider is installed
    if CryptoProvider::get_default().is_none() {
        let _ = CryptoProvider::install_default(
            futures_rustls::rustls::crypto::ring::default_provider(),
        );
    }

    let algorithms = CryptoProvider::get_default()
        .ok_or_else(|| TorError::Internal("No crypto provider installed".to_string()))?
        .signature_verification_algorithms;

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TorCertVerifier(algorithms)))
        .with_no_client_auth();

    Ok(TlsConnector::from(Arc::new(config)))
}

type SnowflakeWsStack = SmuxStream<KcpStream<TurboStream<WebSocketStream>>>;

/// Native WebSocket-based Snowflake stream
pub struct SnowflakeWsStream {
    inner: futures_rustls::client::TlsStream<SnowflakeWsStack>,
}

impl SnowflakeWsStream {
    /// Connect to Snowflake via WebSocket
    pub async fn connect(config: SnowflakeWsConfig) -> Result<Self> {
        info!("Connecting to Snowflake via WebSocket (native)");
        info!("URL: {}", config.ws_url);
        info!("Fingerprint: {}", config.fingerprint);

        // 1. Establish WebSocket connection
        info!("Opening WebSocket connection...");
        let ws = WebSocketStream::connect(&config.ws_url).await?;
        info!("WebSocket connected");

        // 2. Wrap with Turbo framing
        info!("Initializing Turbo layer...");
        let mut turbo = TurboStream::new(ws);
        turbo.initialize().await?;
        info!("Turbo layer initialized");

        // 3. Wrap with KCP for reliability
        info!("Initializing KCP layer...");
        let kcp_config = KcpConfig {
            conv: config.kcp_conv,
            ..Default::default()
        };
        let kcp = KcpStream::new(turbo, kcp_config);
        info!("KCP layer initialized");

        // 4. Wrap with SMUX for multiplexing
        info!("Initializing SMUX layer...");
        let mut smux = SmuxStream::with_stream_id(kcp, config.smux_stream_id);
        smux.initialize().await?;
        info!("SMUX layer initialized");

        // 5. Wrap with TLS (using rustls with custom verifier)
        info!("Establishing TLS...");
        let connector = create_tor_tls_connector()?;
        let server_name: ServerName<'_> = "www.example.com"
            .try_into()
            .map_err(|e| TorError::tls(format!("Invalid server name: {}", e)))?;

        let tls_stream = connector
            .connect(server_name.to_owned(), smux)
            .await
            .map_err(|e| TorError::tls(format!("TLS handshake failed: {}", e)))?;
        info!("TLS layer established");

        info!("Snowflake WS connection established: WebSocket → Turbo → KCP → SMUX → TLS");

        Ok(Self { inner: tls_stream })
    }

    /// Get the peer certificate (DER encoded)
    pub fn peer_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        let (_, session) = self.inner.get_ref();
        Ok(session
            .peer_certificates()
            .and_then(|certs| certs.first().map(|c| Vec::from(c.as_ref()))))
    }

    /// Get our own certificate (DER encoded) - always None for client connections
    pub fn own_certificate(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

impl tor_rtcompat::StreamOps for SnowflakeWsStream {}

impl tor_rtcompat::CertifiedConn for SnowflakeWsStream {
    fn peer_certificate(&self) -> io::Result<Option<Cow<'_, [u8]>>> {
        self.peer_certificate()
            .map(|opt| opt.map(Cow::Owned))
    }

    fn own_certificate(&self) -> io::Result<Option<Cow<'_, [u8]>>> {
        Ok(None)
    }

    fn export_keying_material(
        &self,
        len: usize,
        label: &[u8],
        context: Option<&[u8]>,
    ) -> io::Result<Vec<u8>> {
        let (_, session) = self.inner.get_ref();
        session
            .export_keying_material(Vec::with_capacity(len), label, context)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl AsyncRead for SnowflakeWsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for SnowflakeWsStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

/// Convenience function to create a native WebSocket Snowflake stream
pub async fn create_snowflake_ws_stream() -> Result<SnowflakeWsStream> {
    SnowflakeWsStream::connect(SnowflakeWsConfig::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SnowflakeWsConfig::default();
        assert_eq!(config.ws_url, SNOWFLAKE_WS_URL);
        assert_eq!(config.fingerprint, SNOWFLAKE_FINGERPRINT);
        assert_eq!(config.kcp_conv, 0);
        assert_eq!(config.smux_stream_id, 3);
    }
}