//! Arti-compatible transport for Snowflake bridges
//!
//! This module provides integration with arti-client by implementing
//! `ChannelFactory` and `AbstractPtMgr` for Snowflake transports.
//!
//! # Example
//!
//! ```ignore
//! use webtor_rs::arti_transport::{SnowflakePtMgr, SnowflakeMode};
//!
//! // Create a PT manager for WebSocket Snowflake
//! let pt_mgr = SnowflakePtMgr::new(SnowflakeMode::WebSocket {
//!     url: "wss://snowflake.torproject.net/".to_string(),
//!     fingerprint: None,
//! });
//!
//! // Or for WebRTC via broker
//! let pt_mgr = SnowflakePtMgr::new(SnowflakeMode::WebRtc {
//!     broker_url: "https://snowflake-broker.torproject.net/".to_string(),
//!     fingerprint: None,
//! });
//!
//! // Then set it on the ChanMgr
//! chanmgr.set_pt_mgr(Arc::new(pt_mgr));
//! ```

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use tor_chanmgr::factory::{AbstractPtError, AbstractPtMgr, BootstrapReporter, ChannelFactory};
use tor_error::{ErrorKind, HasKind, HasRetryTime, RetryTime};
use tor_linkspec::{HasRelayIds, IntoOwnedChanTarget, OwnedChanTarget, OwnedChanTargetBuilder, PtTransportName};
use tor_llcrypto::pk::rsa::RsaIdentity;
use tor_proto::channel::Channel;
use tor_proto::memquota::ChannelAccount;
use tor_async_compat::async_trait;
use tracing::{debug, info, warn};

use crate::snowflake::{SnowflakeBridge, SnowflakeConfig};
use crate::snowflake_ws::{SnowflakeWsConfig, SnowflakeWsStream};
use crate::time::system_time_now;
use crate::wasm_runtime::WasmRuntime;

/// Snowflake transport mode
#[derive(Debug, Clone)]
pub enum SnowflakeMode {
    /// WebSocket direct connection to Snowflake bridge
    WebSocket {
        /// WebSocket URL (e.g., "wss://snowflake.torproject.net/")
        url: String,
        /// Optional bridge fingerprint for verification
        fingerprint: Option<String>,
    },
    /// WebRTC connection via broker
    WebRtc {
        /// Broker URL (e.g., "https://snowflake-broker.torproject.net/")
        broker_url: String,
        /// Optional bridge fingerprint for verification
        fingerprint: Option<String>,
    },
}

impl Default for SnowflakeMode {
    fn default() -> Self {
        // Default to WebSocket as it's simpler
        SnowflakeMode::WebSocket {
            url: crate::snowflake_ws::SNOWFLAKE_WS_URL.to_string(),
            fingerprint: Some(crate::snowflake_ws::SNOWFLAKE_FINGERPRINT.to_string()),
        }
    }
}

/// Snowflake channel factory that builds Tor channels over Snowflake transport
pub struct SnowflakeChannelFactory {
    mode: SnowflakeMode,
}

impl SnowflakeChannelFactory {
    /// Create a new Snowflake channel factory
    pub fn new(mode: SnowflakeMode) -> Self {
        Self { mode }
    }

    /// Build a channel using WebSocket Snowflake
    async fn build_ws_channel(
        &self,
        url: &str,
        fingerprint: Option<&str>,
        _target: &OwnedChanTarget,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        info!("Building Snowflake channel via WebSocket: {}", url);

        // Configure WebSocket Snowflake
        let mut config = SnowflakeWsConfig::new().with_url(url);
        if let Some(fp) = fingerprint {
            config = config.with_fingerprint(fp);
        }

        // Connect via WebSocket
        let stream = SnowflakeWsStream::connect(config)
            .await
            .map_err(|e| tor_chanmgr::Error::Io {
                action: "Snowflake WebSocket connect",
                peer: None,
                source: std::io::Error::other(e.to_string()).into(),
            })?;

        // Parse fingerprint to RSA identity if provided
        let rsa_id = fingerprint.and_then(|fp| {
            hex::decode(fp)
                .ok()
                .and_then(|bytes| RsaIdentity::from_bytes(&bytes))
        });

        // Build channel from the stream
        self.create_channel_from_stream(stream, rsa_id, memquota)
            .await
    }

    /// Build a channel using WebRTC Snowflake
    async fn build_webrtc_channel(
        &self,
        broker_url: &str,
        fingerprint: Option<&str>,
        _target: &OwnedChanTarget,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        info!(
            "Building Snowflake channel via WebRTC broker: {}",
            broker_url
        );

        // Configure WebRTC Snowflake
        let mut config = SnowflakeConfig::with_broker(broker_url.to_string());
        if let Some(fp) = fingerprint {
            config = config.with_fingerprint(fp.to_string());
        }

        // Connect via WebRTC
        let bridge = SnowflakeBridge::with_config(config);
        let stream = bridge.connect().await.map_err(|e| tor_chanmgr::Error::Io {
            action: "Snowflake WebRTC connect",
            peer: None,
            source: std::io::Error::other(e.to_string()).into(),
        })?;

        // Parse fingerprint to RSA identity if provided
        let rsa_id = fingerprint.and_then(|fp| {
            hex::decode(fp)
                .ok()
                .and_then(|bytes| RsaIdentity::from_bytes(&bytes))
        });

        // Build channel from the stream
        self.create_channel_from_stream(stream, rsa_id, memquota)
            .await
    }

    /// Create a Tor channel from a connected stream
    ///
    /// This is the core channel building logic, adapted from webtor-rs.
    async fn create_channel_from_stream<S>(
        &self,
        stream: S,
        rsa_id: Option<RsaIdentity>,
        chan_account: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>>
    where
        S: futures::AsyncRead
            + futures::AsyncWrite
            + Send
            + Unpin
            + tor_rtcompat::StreamOps
            + tor_rtcompat::CertifiedConn
            + 'static,
    {
        use tor_proto::channel::ChannelBuilder;

        let runtime = WasmRuntime::default();

        // Extract peer certificate from TLS stream (convert to owned before moving stream)
        let peer_cert = stream.peer_certificate().map_err(|e| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: None,
            source: e.into(),
        })?;

        let peer_cert = peer_cert.map(|c| c.into_owned());

        let peer_cert = peer_cert.ok_or_else(|| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: None,
            source: std::io::Error::new(std::io::ErrorKind::Other, "No peer certificate from TLS")
                .into(),
        })?;

        debug!("Got peer certificate: {} bytes", peer_cert.len());

        // Launch Tor channel handshake
        let builder = ChannelBuilder::new();
        debug!("Launching Tor channel client handshake...");
        let handshake = builder.launch_client(stream, runtime, chan_account);

        debug!("Starting handshake connect...");

        // Build peer target for error reporting and verification
        let mut peer_builder = OwnedChanTargetBuilder::default();
        if let Some(id) = rsa_id {
            peer_builder.rsa_identity(id);
        }

        let peer = peer_builder.build().map_err(|e| {
            tor_chanmgr::Error::Internal(tor_error::internal!(
                "Failed to build peer target: {}",
                e
            ))
        })?;

        let unverified = handshake.connect(system_time_now).await.map_err(|e| {
            tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            }
        })?;

        debug!("Handshake connect completed, verifying...");

        // Verify channel and finish handshake
        let verified = unverified
            .verify(&peer, &peer_cert, Some(system_time_now()))
            .map_err(|e| tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            })?;

        let (chan, reactor) = verified.finish().await.map_err(|e| tor_chanmgr::Error::Proto {
            source: e,
            peer: peer.to_logged(),
            clock_skew: None,
        })?;

        // Log fingerprint if verification was skipped
        if rsa_id.is_none() {
            if let Some(peer_rsa_id) = chan.target().rsa_identity() {
                let fingerprint_hex = hex::encode(peer_rsa_id.as_bytes()).to_uppercase();
                warn!(
                    "Bridge fingerprint verification was skipped. \
                     The bridge's fingerprint is: {}. \
                     For security, consider specifying this fingerprint explicitly.",
                    fingerprint_hex
                );
            }
        }

        // Spawn the channel reactor
        wasm_bindgen_futures::spawn_local(async move {
            let _ = reactor.run().await;
        });

        Ok(chan)
    }
}

#[async_trait]
impl ChannelFactory for SnowflakeChannelFactory {
    async fn connect_via_transport(
        &self,
        target: &OwnedChanTarget,
        _reporter: BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        match &self.mode {
            SnowflakeMode::WebSocket { url, fingerprint } => {
                self.build_ws_channel(url, fingerprint.as_deref(), target, memquota)
                    .await
            }
            SnowflakeMode::WebRtc {
                broker_url,
                fingerprint,
            } => {
                self.build_webrtc_channel(broker_url, fingerprint.as_deref(), target, memquota)
                    .await
            }
        }
    }
}

/// Error type for Snowflake PT manager
#[derive(Debug, Clone)]
pub struct SnowflakePtError {
    message: String,
}

impl std::fmt::Display for SnowflakePtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Snowflake PT error: {}", self.message)
    }
}

impl std::error::Error for SnowflakePtError {}

impl HasKind for SnowflakePtError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::TorAccessFailed
    }
}

impl HasRetryTime for SnowflakePtError {
    fn retry_time(&self) -> RetryTime {
        RetryTime::AfterWaiting
    }
}

impl AbstractPtError for SnowflakePtError {}

/// In-process Snowflake pluggable transport manager
///
/// This implements `AbstractPtMgr` to provide Snowflake transport
/// for arti-client without requiring an external PT binary.
pub struct SnowflakePtMgr {
    mode: SnowflakeMode,
}

impl SnowflakePtMgr {
    /// Create a new Snowflake PT manager
    pub fn new(mode: SnowflakeMode) -> Self {
        Self { mode }
    }

    /// Create with default WebSocket mode
    pub fn websocket_default() -> Self {
        Self::new(SnowflakeMode::default())
    }

    /// Create with custom WebSocket URL
    pub fn websocket(url: impl Into<String>) -> Self {
        Self::new(SnowflakeMode::WebSocket {
            url: url.into(),
            fingerprint: None,
        })
    }

    /// Create with WebRTC via default Tor Project broker
    pub fn webrtc_default() -> Self {
        Self::new(SnowflakeMode::WebRtc {
            broker_url: crate::snowflake_broker::BROKER_URL.to_string(),
            fingerprint: Some(crate::snowflake_broker::DEFAULT_BRIDGE_FINGERPRINT.to_string()),
        })
    }

    /// Create with custom WebRTC broker URL
    pub fn webrtc(broker_url: impl Into<String>) -> Self {
        Self::new(SnowflakeMode::WebRtc {
            broker_url: broker_url.into(),
            fingerprint: None,
        })
    }
}

#[async_trait]
impl AbstractPtMgr for SnowflakePtMgr {
    async fn factory_for_transport(
        &self,
        transport: &PtTransportName,
    ) -> std::result::Result<Option<Arc<dyn ChannelFactory + Send + Sync>>, Arc<dyn AbstractPtError>>
    {
        let transport_name = transport.to_string();

        // Support "snowflake" transport name
        if transport_name == "snowflake" {
            info!(
                "Creating Snowflake channel factory for transport: {}",
                transport_name
            );
            let factory = SnowflakeChannelFactory::new(self.mode.clone());
            Ok(Some(Arc::new(factory)))
        } else {
            // Unknown transport
            debug!("Unknown transport requested: {}", transport_name);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_mode_default() {
        let mode = SnowflakeMode::default();
        match mode {
            SnowflakeMode::WebSocket { url, fingerprint } => {
                assert!(url.contains("snowflake"));
                assert!(fingerprint.is_some());
            }
            _ => panic!("Expected WebSocket mode"),
        }
    }

    #[test]
    fn test_pt_mgr_creation() {
        let _mgr = SnowflakePtMgr::websocket_default();
        let _mgr = SnowflakePtMgr::webrtc_default();
    }
}