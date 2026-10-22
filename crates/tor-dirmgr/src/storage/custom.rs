//! Custom storage backend for directory cache using [`KeyValueStore`].
//!
//! The [`BoxedDirStore`] wrapper implements the full [`Store`] trait while
//! delegating to a [`KeyValueStore`], handling serialization internally.

use crate::docmeta::{AuthCertMeta, ConsensusMeta};
#[cfg(feature = "bridge-client")]
use crate::storage::CachedBridgeDescriptor;
use crate::storage::{ExpirationConfig, InputString, Store};
use crate::{Error, Result};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tor_netdoc::doc::authcert::AuthCertKeyIds;
use tor_netdoc::doc::microdesc::MdDigest;
use tor_netdoc::doc::netstatus::{ConsensusFlavor, Lifetime, ProtoStatuses};
use tor_persist::KeyValueStore;
use std::time::SystemTime;
/// Convert a `time::Duration` to `std::time::Duration`, clamping negatives to zero.
fn time_duration_to_std(d: time::Duration) -> std::time::Duration {
    d.try_into().unwrap_or(std::time::Duration::ZERO)
}

#[cfg(feature = "routerdesc")]
use tor_netdoc::doc::routerdesc::RdDigest;

#[cfg(feature = "bridge-client")]
use tor_guardmgr::bridge::BridgeConfig;

// ============================================================================
// JSON-serializable types for storage
// ============================================================================

/// JSON-serializable consensus metadata and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredConsensus {
    /// Valid-after time (seconds since UNIX epoch)
    valid_after_secs: u64,
    /// Fresh-until time (seconds since UNIX epoch)
    fresh_until_secs: u64,
    /// Valid-until time (seconds since UNIX epoch)
    valid_until_secs: u64,
    /// SHA3-256 of the signed portion (hex)
    sha3_of_signed_hex: String,
    /// SHA3-256 of the whole document (hex)
    sha3_of_whole_hex: String,
    /// Whether this consensus is pending (not yet usable)
    pending: bool,
    /// The consensus document text
    content: String,
}

impl StoredConsensus {
    /// Create a `StoredConsensus` from metadata and document content.
    fn from_meta_and_content(meta: &ConsensusMeta, pending: bool, content: &str) -> Self {
        let lifetime = meta.lifetime();
        Self {
            valid_after_secs: system_time_to_secs(lifetime.valid_after()),
            fresh_until_secs: system_time_to_secs(lifetime.fresh_until()),
            valid_until_secs: system_time_to_secs(lifetime.valid_until()),
            sha3_of_signed_hex: hex::encode(meta.sha3_256_of_signed()),
            sha3_of_whole_hex: hex::encode(meta.sha3_256_of_whole()),
            pending,
            content: content.to_string(),
        }
    }

    /// Convert to a `ConsensusMeta`.
    fn to_meta(&self) -> Result<ConsensusMeta> {
        let lifetime = Lifetime::new(
            secs_to_system_time(self.valid_after_secs),
            secs_to_system_time(self.fresh_until_secs),
            secs_to_system_time(self.valid_until_secs),
        )
        .map_err(|_| Error::CacheCorruption("invalid consensus lifetime"))?;

        let sha3_of_signed = hex_to_32_bytes(&self.sha3_of_signed_hex)?;
        let sha3_of_whole = hex_to_32_bytes(&self.sha3_of_whole_hex)?;

        Ok(ConsensusMeta::new(lifetime, sha3_of_signed, sha3_of_whole))
    }
}

/// JSON-serializable authority certificate metadata and content.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAuthcert {
    /// Identity key fingerprint (hex)
    id_fingerprint_hex: String,
    /// Signing key fingerprint (hex)
    sk_fingerprint_hex: String,
    /// Publication time (seconds since UNIX epoch)
    published_secs: u64,
    /// Expiration time (seconds since UNIX epoch)
    expires_secs: u64,
    /// The certificate text
    content: String,
}

/// JSON-serializable microdescriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredMicrodesc {
    /// The microdescriptor text
    content: String,
    /// Last-listed time (seconds since UNIX epoch)
    listed_at_secs: u64,
}

/// JSON-serializable router descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "routerdesc")]
struct StoredRouterdesc {
    /// The router descriptor text
    content: String,
    /// Publication time (seconds since UNIX epoch)
    published_secs: u64,
}

/// JSON-serializable bridge descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg(feature = "bridge-client")]
struct StoredBridgedesc {
    /// When we fetched this (seconds since UNIX epoch)
    fetched_secs: u64,
    /// The document text
    document: String,
    /// Expiration time (seconds since UNIX epoch)
    until_secs: u64,
}

/// JSON-serializable protocol recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredProtocols {
    /// Valid-after time (seconds since UNIX epoch)
    valid_after_secs: u64,
    /// Serialized protocol statuses
    protocols_json: String,
}

// ============================================================================
// Helper functions
// ============================================================================

/// Convert a `SystemTime` to seconds since UNIX epoch.
fn system_time_to_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Convert seconds since UNIX epoch to a `SystemTime`.
fn secs_to_system_time(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
}

/// Decode a hex string into a 32-byte array.
fn hex_to_32_bytes(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str).map_err(|_| Error::CacheCorruption("invalid hex in cache"))?;
    if bytes.len() != 32 {
        return Err(Error::CacheCorruption("wrong digest length in cache"));
    }
    let mut arr = [0_u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Build a storage key for a consensus document.
fn consensus_key(flavor: ConsensusFlavor, sha3_of_whole: &[u8; 32]) -> String {
    format!("dir:consensus:{}:{}", flavor_to_str(flavor), hex::encode(sha3_of_whole))
}

/// Build a storage key for an authority certificate.
fn authcert_key(ids: &AuthCertKeyIds) -> String {
    format!(
        "dir:authcert:{}:{}",
        hex::encode(ids.id_fingerprint.as_bytes()),
        hex::encode(ids.sk_fingerprint.as_bytes())
    )
}

/// Build a storage key for a microdescriptor.
fn microdesc_key(digest: &MdDigest) -> String {
    format!("dir:microdesc:{}", hex::encode(digest))
}

#[cfg(feature = "routerdesc")]
/// Build a storage key for a router descriptor.
fn routerdesc_key(digest: &RdDigest) -> String {
    format!("dir:routerdesc:{}", hex::encode(digest))
}

#[cfg(feature = "bridge-client")]
/// Build a storage key for a bridge descriptor.
fn bridge_key(bridge: &BridgeConfig) -> String {
    // Use a hash of the bridge config string as the key
    use digest::Digest;
    let hash = tor_llcrypto::d::Sha256::digest(bridge.to_string().as_bytes());
    format!("dir:bridge:{}", hex::encode(&hash[..16]))
}

/// Convert a `ConsensusFlavor` to its string representation.
fn flavor_to_str(flavor: ConsensusFlavor) -> &'static str {
    match flavor {
        ConsensusFlavor::Microdesc => "microdesc",
        ConsensusFlavor::Plain => "plain",
    }
}

// ============================================================================
// BoxedDirStore - wrapper implementing Store for any KeyValueStore
// ============================================================================

/// A wrapper that implements [`Store`] for any [`KeyValueStore`].
///
/// This allows custom storage implementations to be used anywhere a `Store`
/// is expected. JSON serialization/deserialization is handled automatically.
///
/// Directory keys already include the `"dir:"` prefix, so no additional
/// prefix is added.
#[derive(Clone)]
pub struct BoxedDirStore {
    /// The underlying key-value store.
    inner: Arc<dyn KeyValueStore>,
}

impl BoxedDirStore {
    /// Create a new `BoxedDirStore` from a shared [`KeyValueStore`].
    pub fn new(storage: Arc<dyn KeyValueStore>) -> Self {
        Self { inner: storage }
    }

    /// Load and deserialize a JSON value from the underlying store.
    fn load_json<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<T>> {
        match self.inner.get(key).map_err(|e| {
            tracing::warn!("custom dir store load error: {}", e);
            Error::CacheCorruption("custom storage read failed")
        })? {
            Some(json) => {
                let value: T = serde_json::from_str(&json)
                    .map_err(|_| Error::CacheCorruption("invalid JSON in cache"))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Serialize and store a value as JSON in the underlying store.
    fn store_json<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        if !self.inner.can_store().unwrap_or(false) {
            return Err(Error::CacheCorruption("store is read-only"));
        }
        let json = serde_json::to_string(value)
            .map_err(|_| Error::CacheCorruption("failed to serialize"))?;
        self.inner.set(key, &json).map_err(|e| {
            tracing::warn!("custom dir store write error: {}", e);
            Error::CacheCorruption("custom storage write failed")
        })
    }

    /// Delete a key from the underlying store.
    fn delete_key(&self, key: &str) -> Result<()> {
        if !self.inner.can_store().unwrap_or(false) {
            return Err(Error::CacheCorruption("store is read-only"));
        }
        self.inner.delete(key).map_err(|e| {
            tracing::warn!("custom dir store delete error: {}", e);
            Error::CacheCorruption("custom storage delete failed")
        })
    }

    /// List all keys with the given prefix.
    fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        self.inner.keys(prefix).map_err(|e| {
            tracing::warn!("custom dir store keys error: {}", e);
            Error::CacheCorruption("custom storage keys failed")
        })
    }
}

impl Store for BoxedDirStore {
    fn is_readonly(&self) -> bool {
        !self.inner.can_store().unwrap_or(false)
    }

    fn upgrade_to_readwrite(&mut self) -> Result<bool> {
        self.inner.try_lock().map_err(|e| {
            tracing::warn!("custom dir store lock error: {}", e);
            Error::CacheCorruption("custom storage lock failed")
        }).map(|_newly| {
            // try_lock returns true if newly acquired, false if already held.
            // Either way, we now have write access.
            true
        })
    }

    fn expire_all(&mut self, expiration: &ExpirationConfig) -> Result<()> {
        use web_time_compat::SystemTimeExt as _;
        let now = SystemTime::get();

        // Expire consensuses
        for key in self.list_keys("dir:consensus:")? {
            if let Some(stored) = self.load_json::<StoredConsensus>(&key)? {
                let valid_until = secs_to_system_time(stored.valid_until_secs);
                let expiry = valid_until + time_duration_to_std(expiration.consensuses);
                if now >= expiry {
                    self.delete_key(&key)?;
                }
            }
        }

        // Expire authcerts
        for key in self.list_keys("dir:authcert:")? {
            if let Some(stored) = self.load_json::<StoredAuthcert>(&key)? {
                let expires = secs_to_system_time(stored.expires_secs);
                let expiry = expires + time_duration_to_std(expiration.authcerts);
                if now >= expiry {
                    self.delete_key(&key)?;
                }
            }
        }

        // Expire microdescs
        for key in self.list_keys("dir:microdesc:")? {
            if let Some(stored) = self.load_json::<StoredMicrodesc>(&key)? {
                let listed = secs_to_system_time(stored.listed_at_secs);
                let expiry = listed + time_duration_to_std(expiration.microdescs);
                if now >= expiry {
                    self.delete_key(&key)?;
                }
            }
        }

        // Expire router descriptors
        #[cfg(feature = "routerdesc")]
        for key in self.list_keys("dir:routerdesc:")? {
            if let Some(stored) = self.load_json::<StoredRouterdesc>(&key)? {
                let published = secs_to_system_time(stored.published_secs);
                let expiry = published + time_duration_to_std(expiration.router_descs);
                if now >= expiry {
                    self.delete_key(&key)?;
                }
            }
        }

        // Expire bridge descriptors
        #[cfg(feature = "bridge-client")]
        for key in self.list_keys("dir:bridge:")? {
            if let Some(stored) = self.load_json::<StoredBridgedesc>(&key)? {
                let until = secs_to_system_time(stored.until_secs);
                if now >= until {
                    self.delete_key(&key)?;
                }
            }
        }

        Ok(())
    }

    fn latest_consensus(
        &self,
        flavor: ConsensusFlavor,
        pending: Option<bool>,
    ) -> Result<Option<InputString>> {
        let prefix = format!("dir:consensus:{}:", flavor_to_str(flavor));

        let mut latest: Option<StoredConsensus> = None;
        for key in self.list_keys(&prefix)? {
            if let Some(stored) = self.load_json::<StoredConsensus>(&key)? {
                // Filter by pending status if specified
                if let Some(want_pending) = pending {
                    if stored.pending != want_pending {
                        continue;
                    }
                }
                // Keep the latest by valid_after time
                match &latest {
                    None => latest = Some(stored),
                    Some(prev) if stored.valid_after_secs > prev.valid_after_secs => {
                        latest = Some(stored);
                    }
                    _ => {}
                }
            }
        }

        Ok(latest.map(|s| InputString::from(s.content)))
    }

    fn latest_consensus_meta(&self, flavor: ConsensusFlavor) -> Result<Option<ConsensusMeta>> {
        let prefix = format!("dir:consensus:{}:", flavor_to_str(flavor));

        let mut latest: Option<StoredConsensus> = None;
        for key in self.list_keys(&prefix)? {
            if let Some(stored) = self.load_json::<StoredConsensus>(&key)? {
                // Only non-pending consensuses
                if stored.pending {
                    continue;
                }
                match &latest {
                    None => latest = Some(stored),
                    Some(prev) if stored.valid_after_secs > prev.valid_after_secs => {
                        latest = Some(stored);
                    }
                    _ => {}
                }
            }
        }

        match latest {
            Some(stored) => Ok(Some(stored.to_meta()?)),
            None => Ok(None),
        }
    }

    #[cfg(test)]
    fn consensus_by_meta(&self, cmeta: &ConsensusMeta) -> Result<InputString> {
        match self.consensus_by_sha3_digest_of_signed_part(cmeta.sha3_256_of_signed())? {
            Some((text, _)) => Ok(text),
            None => Err(Error::CacheCorruption("couldn't find consensus")),
        }
    }

    fn consensus_by_sha3_digest_of_signed_part(
        &self,
        d: &[u8; 32],
    ) -> Result<Option<(InputString, ConsensusMeta)>> {
        let target_hex = hex::encode(d);

        for key in self.list_keys("dir:consensus:")? {
            if let Some(stored) = self.load_json::<StoredConsensus>(&key)? {
                if stored.sha3_of_signed_hex == target_hex {
                    let meta = stored.to_meta()?;
                    return Ok(Some((InputString::from(stored.content), meta)));
                }
            }
        }

        Ok(None)
    }

    fn store_consensus(
        &mut self,
        cmeta: &ConsensusMeta,
        flavor: ConsensusFlavor,
        pending: bool,
        contents: &str,
    ) -> Result<()> {
        let key = consensus_key(flavor, cmeta.sha3_256_of_whole());
        let stored = StoredConsensus::from_meta_and_content(cmeta, pending, contents);
        self.store_json(&key, &stored)
    }

    fn mark_consensus_usable(&mut self, cmeta: &ConsensusMeta) -> Result<()> {
        // Find the consensus with matching sha3_of_whole
        let target_hex = hex::encode(cmeta.sha3_256_of_whole());
        for key in self.list_keys("dir:consensus:")? {
            if let Some(mut stored) = self.load_json::<StoredConsensus>(&key)? {
                if stored.sha3_of_whole_hex == target_hex {
                    stored.pending = false;
                    return self.store_json(&key, &stored);
                }
            }
        }

        Ok(())
    }

    fn delete_consensus(&mut self, cmeta: &ConsensusMeta) -> Result<()> {
        let target_hex = hex::encode(cmeta.sha3_256_of_whole());

        for key in self.list_keys("dir:consensus:")? {
            if key.ends_with(&target_hex) {
                self.delete_key(&key)?;
            }
        }

        Ok(())
    }

    fn authcerts(&self, certs: &[AuthCertKeyIds]) -> Result<HashMap<AuthCertKeyIds, String>> {
        let mut result = HashMap::new();
        for ids in certs {
            let key = authcert_key(ids);
            if let Some(stored) = self.load_json::<StoredAuthcert>(&key)? {
                result.insert(*ids, stored.content);
            }
        }
        Ok(result)
    }

    fn store_authcerts(&mut self, certs: &[(AuthCertMeta, &str)]) -> Result<()> {
        for (meta, content) in certs {
            let key = authcert_key(meta.key_ids());
            let stored = StoredAuthcert {
                id_fingerprint_hex: hex::encode(meta.key_ids().id_fingerprint.as_bytes()),
                sk_fingerprint_hex: hex::encode(meta.key_ids().sk_fingerprint.as_bytes()),
                published_secs: system_time_to_secs(meta.published()),
                expires_secs: system_time_to_secs(meta.expires()),
                content: (*content).to_string(),
            };
            self.store_json(&key, &stored)?;
        }
        Ok(())
    }

    fn microdescs(&self, digests: &[MdDigest]) -> Result<HashMap<MdDigest, String>> {
        let mut result = HashMap::new();
        for digest in digests {
            let key = microdesc_key(digest);
            if let Some(stored) = self.load_json::<StoredMicrodesc>(&key)? {
                result.insert(*digest, stored.content);
            }
        }
        Ok(result)
    }

    fn store_microdescs(&mut self, digests: &[(&str, &MdDigest)], when: SystemTime) -> Result<()> {
        for (content, digest) in digests {
            let key = microdesc_key(digest);
            let stored = StoredMicrodesc {
                content: (*content).to_string(),
                listed_at_secs: system_time_to_secs(when),
            };
            self.store_json(&key, &stored)?;
        }
        Ok(())
    }

    fn update_microdescs_listed(&mut self, digests: &[MdDigest], when: SystemTime) -> Result<()> {
        let when_secs = system_time_to_secs(when);
        for digest in digests {
            let key = microdesc_key(digest);
            if let Some(mut stored) = self.load_json::<StoredMicrodesc>(&key)? {
                if stored.listed_at_secs < when_secs {
                    stored.listed_at_secs = when_secs;
                    self.store_json(&key, &stored)?;
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "routerdesc")]
    fn routerdescs(&self, digests: &[RdDigest]) -> Result<HashMap<RdDigest, String>> {
        let mut result = HashMap::new();
        for digest in digests {
            let key = routerdesc_key(digest);
            if let Some(stored) = self.load_json::<StoredRouterdesc>(&key)? {
                result.insert(*digest, stored.content);
            }
        }
        Ok(result)
    }

    #[cfg(feature = "routerdesc")]
    fn store_routerdescs(&mut self, digests: &[(&str, SystemTime, &RdDigest)]) -> Result<()> {
        for (content, when, digest) in digests {
            let key = routerdesc_key(digest);
            let stored = StoredRouterdesc {
                content: (*content).to_string(),
                published_secs: system_time_to_secs(*when),
            };
            self.store_json(&key, &stored)?;
        }
        Ok(())
    }

    #[cfg(feature = "bridge-client")]
    fn lookup_bridgedesc(&self, bridge: &BridgeConfig) -> Result<Option<CachedBridgeDescriptor>> {
        let key = bridge_key(bridge);
        if let Some(stored) = self.load_json::<StoredBridgedesc>(&key)? {
            Ok(Some(CachedBridgeDescriptor {
                fetched: secs_to_system_time(stored.fetched_secs),
                document: stored.document,
            }))
        } else {
            Ok(None)
        }
    }

    #[cfg(feature = "bridge-client")]
    fn store_bridgedesc(
        &mut self,
        bridge: &BridgeConfig,
        entry: CachedBridgeDescriptor,
        until: SystemTime,
    ) -> Result<()> {
        let key = bridge_key(bridge);
        let stored = StoredBridgedesc {
            fetched_secs: system_time_to_secs(entry.fetched),
            document: entry.document,
            until_secs: system_time_to_secs(until),
        };
        self.store_json(&key, &stored)
    }

    #[cfg(feature = "bridge-client")]
    fn delete_bridgedesc(&mut self, bridge: &BridgeConfig) -> Result<()> {
        let key = bridge_key(bridge);
        self.delete_key(&key)
    }

    fn update_protocol_recommendations(
        &mut self,
        valid_after: SystemTime,
        protocols: &ProtoStatuses,
    ) -> Result<()> {
        let key = "dir:protocols";
        let valid_after_secs = system_time_to_secs(valid_after);

        // Only update if this is newer than what we have
        if let Some(existing) = self.load_json::<StoredProtocols>(key)? {
            if existing.valid_after_secs >= valid_after_secs {
                return Ok(());
            }
        }

        let protocols_json = serde_json::to_string(protocols)
            .map_err(|_| Error::CacheCorruption("failed to serialize protocols"))?;

        let stored = StoredProtocols {
            valid_after_secs,
            protocols_json,
        };
        self.store_json(key, &stored)
    }

    fn cached_protocol_recommendations(&self) -> Result<Option<(SystemTime, ProtoStatuses)>> {
        let key = "dir:protocols";
        if let Some(stored) = self.load_json::<StoredProtocols>(key)? {
            let valid_after = secs_to_system_time(stored.valid_after_secs);
            let protocols: ProtoStatuses = serde_json::from_str(&stored.protocols_json)
                .map_err(|_| Error::CacheCorruption("invalid protocol JSON in cache"))?;
            Ok(Some((valid_after, protocols)))
        } else {
            Ok(None)
        }
    }
}
