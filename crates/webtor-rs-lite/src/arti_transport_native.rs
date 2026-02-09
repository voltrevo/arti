//! Native Arti-compatible transport for Snowflake bridges
//!
//! This module provides integration with arti-client by implementing
//! `ChannelFactory` and `AbstractPtMgr` for Snowflake transports on native (non-WASM).

#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use async_trait::async_trait;
use tor_chanmgr::factory::{AbstractPtError, AbstractPtMgr, BootstrapReporter, ChannelFactory};
use tor_error::{ErrorKind, HasKind, HasRetryTime, RetryTime};
use tor_linkspec::{HasRelayIds, IntoOwnedChanTarget, OwnedChanTarget, OwnedChanTargetBuilder, PtTransportName};
use tor_llcrypto::pk::rsa::RsaIdentity;
use tor_proto::channel::{Channel, ChannelBuilder};
use tor_proto::memquota::ChannelAccount;
use tor_rtcompat::{Runtime, SpawnExt};
use tor_time::SystemTime;
use tracing::{debug, info, warn};

use crate::snowflake_ws_native::{SnowflakeWsConfig, SnowflakeWsStream, SNOWFLAKE_WS_URL, SNOWFLAKE_FINGERPRINT};

/// Snowflake channel factory that builds Tor channels over Snowflake transport (native)
pub struct SnowflakeChannelFactory<R: Runtime> {
    url: String,
    fingerprint: Option<String>,
    runtime: R,
}

impl<R: Runtime> SnowflakeChannelFactory<R> {
    /// Create a new Snowflake channel factory with default PSE bridge
    pub fn new(runtime: R) -> Self {
        Self {
            url: SNOWFLAKE_WS_URL.to_string(),
            fingerprint: Some(SNOWFLAKE_FINGERPRINT.to_string()),
            runtime,
        }
    }

    /// Create with custom URL
    pub fn with_url(runtime: R, url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            fingerprint: None,
            runtime,
        }
    }

    /// Set the fingerprint
    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }

    /// Build a channel using WebSocket Snowflake
    async fn build_channel(
        &self,
        _target: &OwnedChanTarget,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        info!("Building native Snowflake channel via WebSocket: {}", self.url);

        // Configure WebSocket Snowflake
        let mut config = SnowflakeWsConfig::new().with_url(&self.url);
        if let Some(fp) = &self.fingerprint {
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
        let rsa_id = self.fingerprint.as_ref().and_then(|fp| {
            hex::decode(fp)
                .ok()
                .and_then(|bytes| RsaIdentity::from_bytes(&bytes))
        });

        // Get peer certificate from TLS stream
        let peer_cert = stream.peer_certificate().map_err(|e| tor_chanmgr::Error::Io {
            action: "get peer certificate",
            peer: None,
            source: e.into(),
        })?;

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
        let handshake = builder.launch_client(stream, self.runtime.clone(), memquota);

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

        let now_fn = || SystemTime::now();
        let unverified = handshake.connect(now_fn).await.map_err(|e| {
            tor_chanmgr::Error::Proto {
                source: e,
                peer: peer.clone().to_logged(),
                clock_skew: None,
            }
        })?;

        debug!("Handshake connect completed, verifying...");

        // Verify channel and finish handshake
        let verified = unverified
            .verify(&peer, &peer_cert, Some(SystemTime::now()))
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
        if self.fingerprint.is_none() {
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

        // Spawn the channel reactor using SpawnExt trait
        self.runtime.spawn(async move {
            let _ = reactor.run().await;
        }).map_err(|e| tor_chanmgr::Error::Spawn {
            spawning: "channel reactor",
            cause: Arc::new(e),
        })?;

        Ok(chan)
    }
}

#[async_trait]
impl<R: Runtime> ChannelFactory for SnowflakeChannelFactory<R> {
    async fn connect_via_transport(
        &self,
        target: &OwnedChanTarget,
        _reporter: BootstrapReporter,
        memquota: ChannelAccount,
    ) -> tor_chanmgr::Result<Arc<Channel>> {
        self.build_channel(target, memquota).await
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

/// In-process Snowflake pluggable transport manager (native)
///
/// This implements `AbstractPtMgr` to provide Snowflake transport
/// for arti-client without requiring an external PT binary.
pub struct SnowflakePtMgr<R: Runtime> {
    url: String,
    fingerprint: Option<String>,
    runtime: R,
}

impl<R: Runtime> SnowflakePtMgr<R> {
    /// Create a new Snowflake PT manager with default PSE bridge
    pub fn new(runtime: R) -> Self {
        Self {
            url: SNOWFLAKE_WS_URL.to_string(),
            fingerprint: Some(SNOWFLAKE_FINGERPRINT.to_string()),
            runtime,
        }
    }

    /// Create with custom WebSocket URL
    pub fn with_url(runtime: R, url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            fingerprint: None,
            runtime,
        }
    }

    /// Set the fingerprint
    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }
}

#[async_trait]
impl<R: Runtime> AbstractPtMgr for SnowflakePtMgr<R> {
    async fn factory_for_transport(
        &self,
        transport: &PtTransportName,
    ) -> std::result::Result<Option<Arc<dyn ChannelFactory + Send + Sync>>, Arc<dyn AbstractPtError>>
    {
        let transport_name = transport.to_string();

        // Support "snowflake" transport name
        if transport_name == "snowflake" {
            info!(
                "Creating native Snowflake channel factory for transport: {}",
                transport_name
            );
            let mut factory = SnowflakeChannelFactory::new(self.runtime.clone());
            factory.url = self.url.clone();
            factory.fingerprint = self.fingerprint.clone();
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
    fn test_pt_mgr_creation() {
        // Just verify the types compile - actual runtime test would need tokio
    }
}