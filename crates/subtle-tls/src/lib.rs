//! SubtleTLS - TLS 1.3 implementation using browser SubtleCrypto API
//!
//! This crate provides TLS encryption for WASM environments where
//! native crypto libraries like `ring` cannot be used. It leverages the
//! browser's SubtleCrypto API for all cryptographic operations.
//!
//! # Features
//! - TLS 1.3 client implementation
//! - ECDHE key exchange with P-256 and X25519
//! - AES-128-GCM, AES-256-GCM, and ChaCha20-Poly1305 encryption
//! - Certificate chain validation
//! - AsyncRead/AsyncWrite interface
//!
//! # Example
//! ```no_run
//! use subtle_tls::{TlsConnector, Result};
//! use futures::io::AsyncWriteExt;
//!
//! async fn example<S>(tcp_stream: S) -> Result<()>
//! where
//!     S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + 'static,
//! {
//!     let connector = TlsConnector::new();
//!     let mut tls_stream = connector.connect(tcp_stream, "example.com").await?;
//!     tls_stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await?;
//!     Ok(())
//! }
//! ```

#[cfg(test)]
pub mod test_util;

pub mod cert;
pub mod crypto;
pub mod error;
pub mod handshake;
pub mod record;
pub mod stream;
pub mod trust_store;

pub use error::{Result, TlsError};
pub use stream::TlsStream;

// Re-export the wrapper for version-aware TLS
// Note: TlsStreamWrapper is defined below after TlsConnector

/// TLS connector for establishing secure connections
pub struct TlsConnector {
    config: TlsConfig,
}

/// TLS version preference
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TlsVersion {
    /// TLS 1.3 only
    #[default]
    Tls13,
}

/// TLS configuration
#[derive(Clone)]
pub struct TlsConfig {
    /// Skip certificate verification (INSECURE - for testing only)
    pub skip_verification: bool,
    /// Application-Layer Protocol Negotiation protocols
    pub alpn_protocols: Vec<String>,
    /// TLS version preference
    pub version: TlsVersion,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            skip_verification: false,
            alpn_protocols: vec!["http/1.1".to_string()],
            version: TlsVersion::default(),
        }
    }
}

impl TlsConnector {
    /// Create a new TLS connector with default configuration
    pub fn new() -> Self {
        Self {
            config: TlsConfig::default(),
        }
    }

    /// Create a TLS connector with custom configuration
    pub fn with_config(config: TlsConfig) -> Self {
        Self { config }
    }

    /// Connect to a server, wrapping the given stream with TLS
    pub async fn connect<S>(&self, stream: S, server_name: &str) -> Result<TlsStream<S>>
    where
        S: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin,
    {
        TlsStream::connect(stream, server_name, self.config.clone()).await
    }
}

impl Default for TlsConnector {
    fn default() -> Self {
        Self::new()
    }
}
