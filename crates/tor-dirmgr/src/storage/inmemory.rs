//! In-memory directory storage.
//!
//! This module provides an in-memory implementation of the [`Store`] trait,
//! suitable for use in environments where SQLite is unavailable (e.g., WASM).

use super::{ExpirationConfig, InputString, Store};
#[cfg(feature = "bridge-client")]
use super::CachedBridgeDescriptor;
use crate::docmeta::{AuthCertMeta, ConsensusMeta};
use crate::{Error, Result};

use tor_netdoc::doc::authcert::AuthCertKeyIds;
use tor_netdoc::doc::microdesc::MdDigest;
use tor_netdoc::doc::netstatus::{ConsensusFlavor, ProtoStatuses};

#[cfg(feature = "routerdesc")]
use tor_netdoc::doc::routerdesc::RdDigest;

#[cfg(feature = "bridge-client")]
use tor_guardmgr::bridge::BridgeConfig;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tor_time::{time_duration_to_std, SystemTime};
use tor_error::internal;
use tracing::warn;

/// Stored consensus with its metadata and content.
#[derive(Clone, Debug)]
struct StoredConsensus {
    /// Metadata for the consensus.
    meta: ConsensusMeta,
    /// Whether this consensus is pending (not yet usable).
    pending: bool,
    /// The consensus document text.
    content: String,
}

/// Internal state for [`InMemoryStore`].
#[derive(Debug, Default)]
struct InMemoryStoreInner {
    /// Stored consensuses, keyed by (flavor, sha3_256_of_whole).
    consensuses: HashMap<(ConsensusFlavor, [u8; 32]), StoredConsensus>,
    /// Authority certificates, keyed by their key IDs.
    authcerts: HashMap<AuthCertKeyIds, (AuthCertMeta, String)>,
    /// Microdescriptors, keyed by digest.
    microdescs: HashMap<MdDigest, (String, SystemTime)>,
    /// Router descriptors, keyed by digest (only with routerdesc feature).
    #[cfg(feature = "routerdesc")]
    routerdescs: HashMap<RdDigest, (String, SystemTime)>,
    /// Bridge descriptors (only with bridge-client feature).
    #[cfg(feature = "bridge-client")]
    bridgedescs: HashMap<String, (CachedBridgeDescriptor, SystemTime)>,
    /// Cached protocol recommendations.
    protocol_recs: Option<(SystemTime, ProtoStatuses)>,
}

/// In-memory directory cache.
///
/// This store keeps all directory data in memory. It does not persist
/// across restarts. This is useful for WASM environments where SQLite
/// is not available.
#[derive(Debug)]
pub(crate) struct InMemoryStore {
    /// The inner state, protected by a RwLock for interior mutability.
    inner: Arc<RwLock<InMemoryStoreInner>>,
    /// Whether this store is read-only.
    readonly: bool,
}

impl InMemoryStore {
    /// Create a new, empty in-memory store.
    pub(crate) fn new(readonly: bool) -> Self {
        InMemoryStore {
            inner: Arc::new(RwLock::new(InMemoryStoreInner::default())),
            readonly,
        }
    }
}

impl Store for InMemoryStore {
    fn is_readonly(&self) -> bool {
        self.readonly
    }

    fn upgrade_to_readwrite(&mut self) -> Result<bool> {
        self.readonly = false;
        Ok(true)
    }

    fn expire_all(&mut self, expiration: &ExpirationConfig) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let now = SystemTime::now();
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        // Expire consensuses based on valid_until + tolerance
        inner.consensuses.retain(|_, stored| {
            let valid_until = stored.meta.lifetime().valid_until();
            let expiry = valid_until + time_duration_to_std(expiration.consensuses);
            now < expiry
        });

        // Expire authcerts based on expires time
        inner.authcerts.retain(|_, (meta, _)| {
            let expiry = meta.expires() + time_duration_to_std(expiration.authcerts);
            now < expiry
        });

        // Expire microdescs based on last-listed time
        inner.microdescs.retain(|_, (_, listed)| {
            let expiry = *listed + time_duration_to_std(expiration.microdescs);
            now < expiry
        });

        // Expire router descriptors based on publication time
        #[cfg(feature = "routerdesc")]
        inner.routerdescs.retain(|_, (_, published)| {
            let expiry = *published + time_duration_to_std(expiration.router_descs);
            now < expiry
        });

        // Expire bridge descriptors based on until time
        #[cfg(feature = "bridge-client")]
        inner.bridgedescs.retain(|_, (_, until)| now < *until);

        Ok(())
    }

    fn latest_consensus(
        &self,
        flavor: ConsensusFlavor,
        pending: Option<bool>,
    ) -> Result<Option<InputString>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        // Find the latest consensus of the given flavor
        let mut latest: Option<&StoredConsensus> = None;
        for ((f, _), stored) in &inner.consensuses {
            if *f != flavor {
                continue;
            }
            if let Some(want_pending) = pending {
                if stored.pending != want_pending {
                    continue;
                }
            }
            match latest {
                None => latest = Some(stored),
                Some(prev) => {
                    if stored.meta.lifetime().valid_after() > prev.meta.lifetime().valid_after() {
                        latest = Some(stored);
                    }
                }
            }
        }

        Ok(latest.map(|s| InputString::from(s.content.clone()).into()))
    }

    fn latest_consensus_meta(&self, flavor: ConsensusFlavor) -> Result<Option<ConsensusMeta>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        // Find the latest non-pending consensus of the given flavor
        let mut latest: Option<&StoredConsensus> = None;
        for ((f, _), stored) in &inner.consensuses {
            if *f != flavor || stored.pending {
                continue;
            }
            match latest {
                None => latest = Some(stored),
                Some(prev) => {
                    if stored.meta.lifetime().valid_after() > prev.meta.lifetime().valid_after() {
                        latest = Some(stored);
                    }
                }
            }
        }

        Ok(latest.map(|s| s.meta.clone()))
    }

    #[cfg(test)]
    fn consensus_by_meta(&self, cmeta: &ConsensusMeta) -> Result<InputString> {
        if let Some((text, _)) =
            self.consensus_by_sha3_digest_of_signed_part(cmeta.sha3_256_of_signed())?
        {
            Ok(text)
        } else {
            Err(Error::CacheCorruption(
                "couldn't find a consensus we thought we had.",
            ))
        }
    }

    fn consensus_by_sha3_digest_of_signed_part(
        &self,
        d: &[u8; 32],
    ) -> Result<Option<(InputString, ConsensusMeta)>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        for (_, stored) in &inner.consensuses {
            if stored.meta.sha3_256_of_signed() == d {
                return Ok(Some((
                    InputString::from(stored.content.clone()).into(),
                    stored.meta.clone(),
                )));
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
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let key = (flavor, *cmeta.sha3_256_of_whole());
        inner.consensuses.insert(
            key,
            StoredConsensus {
                meta: cmeta.clone(),
                pending,
                content: contents.to_string(),
            },
        );

        Ok(())
    }

    fn mark_consensus_usable(&mut self, cmeta: &ConsensusMeta) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        // Find and mark the consensus as non-pending
        for (_, stored) in inner.consensuses.iter_mut() {
            if stored.meta.sha3_256_of_whole() == cmeta.sha3_256_of_whole() {
                stored.pending = false;
                return Ok(());
            }
        }

        Ok(())
    }

    fn delete_consensus(&mut self, cmeta: &ConsensusMeta) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        // Remove by sha3_256_of_whole
        inner.consensuses.retain(|(_, digest), _| {
            digest != cmeta.sha3_256_of_whole()
        });

        Ok(())
    }

    fn authcerts(&self, certs: &[AuthCertKeyIds]) -> Result<HashMap<AuthCertKeyIds, String>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let mut result = HashMap::new();
        for ids in certs {
            if let Some((_, content)) = inner.authcerts.get(ids) {
                result.insert(*ids, content.clone());
            }
        }

        Ok(result)
    }

    fn store_authcerts(&mut self, certs: &[(AuthCertMeta, &str)]) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        for (meta, content) in certs {
            inner.authcerts.insert(
                *meta.key_ids(),
                (meta.clone(), (*content).to_string()),
            );
        }

        Ok(())
    }

    fn microdescs(&self, digests: &[MdDigest]) -> Result<HashMap<MdDigest, String>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let mut result = HashMap::new();
        for digest in digests {
            if let Some((content, _)) = inner.microdescs.get(digest) {
                result.insert(*digest, content.clone());
            }
        }

        Ok(result)
    }

    fn store_microdescs(&mut self, digests: &[(&str, &MdDigest)], when: SystemTime) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        for (content, digest) in digests {
            inner.microdescs.insert(**digest, ((*content).to_string(), when));
        }

        Ok(())
    }

    fn update_microdescs_listed(&mut self, digests: &[MdDigest], when: SystemTime) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        for digest in digests {
            if let Some((_, listed)) = inner.microdescs.get_mut(digest) {
                if *listed < when {
                    *listed = when;
                }
            }
        }

        Ok(())
    }

    #[cfg(feature = "routerdesc")]
    fn routerdescs(&self, digests: &[RdDigest]) -> Result<HashMap<RdDigest, String>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let mut result = HashMap::new();
        for digest in digests {
            if let Some((content, _)) = inner.routerdescs.get(digest) {
                result.insert(*digest, content.clone());
            }
        }

        Ok(result)
    }

    #[cfg(feature = "routerdesc")]
    fn store_routerdescs(&mut self, digests: &[(&str, SystemTime, &RdDigest)]) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        for (content, when, digest) in digests {
            inner.routerdescs.insert(**digest, ((*content).to_string(), *when));
        }

        Ok(())
    }

    #[cfg(feature = "bridge-client")]
    fn lookup_bridgedesc(&self, bridge: &BridgeConfig) -> Result<Option<CachedBridgeDescriptor>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let key = bridge.to_string();
        Ok(inner.bridgedescs.get(&key).map(|(desc, _)| desc.clone()))
    }

    #[cfg(feature = "bridge-client")]
    fn store_bridgedesc(
        &mut self,
        bridge: &BridgeConfig,
        entry: CachedBridgeDescriptor,
        until: SystemTime,
    ) -> Result<()> {
        if self.readonly {
            warn!("Skipping store_bridgedesc on readonly store");
            return Ok(());
        }

        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let key = bridge.to_string();
        inner.bridgedescs.insert(key, (entry, until));

        Ok(())
    }

    #[cfg(feature = "bridge-client")]
    fn delete_bridgedesc(&mut self, bridge: &BridgeConfig) -> Result<()> {
        if self.readonly {
            warn!("Skipping delete_bridgedesc on readonly store");
            return Ok(());
        }

        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        let key = bridge.to_string();
        inner.bridgedescs.remove(&key);

        Ok(())
    }

    fn update_protocol_recommendations(
        &mut self,
        valid_after: SystemTime,
        protocols: &ProtoStatuses,
    ) -> Result<()> {
        if self.readonly {
            return Err(internal!("attempted write to readonly store").into());
        }
        let mut inner = self.inner.write().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        // Only update if this is newer than what we have
        match &inner.protocol_recs {
            Some((existing_time, _)) if *existing_time >= valid_after => {
                // Our existing recommendation is newer or same; don't update
            }
            _ => {
                inner.protocol_recs = Some((valid_after, protocols.clone()));
            }
        }

        Ok(())
    }

    fn cached_protocol_recommendations(&self) -> Result<Option<(SystemTime, ProtoStatuses)>> {
        let inner = self.inner.read().map_err(|_| {
            Error::CacheCorruption("InMemoryStore lock poisoned")
        })?;

        Ok(inner.protocol_recs.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tor_netdoc::doc::netstatus::Lifetime;

    fn make_test_cmeta(valid_after_secs: u64) -> ConsensusMeta {
        let valid_after = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(valid_after_secs);
        let fresh_until = valid_after + std::time::Duration::from_secs(3600);
        let valid_until = fresh_until + std::time::Duration::from_secs(3600);
        let lifetime = Lifetime::new(valid_after, fresh_until, valid_until).unwrap();
        ConsensusMeta::new(lifetime, [0u8; 32], [1u8; 32])
    }

    #[test]
    fn test_store_and_retrieve_consensus() {
        let mut store = InMemoryStore::new(false);
        let cmeta = make_test_cmeta(1000);
        let content = "test consensus content";

        store
            .store_consensus(&cmeta, ConsensusFlavor::Microdesc, true, content)
            .unwrap();

        // Should find it when looking for pending
        let found = store
            .latest_consensus(ConsensusFlavor::Microdesc, Some(true))
            .unwrap();
        assert!(found.is_some());

        // Should not find it when looking for non-pending
        let not_found = store
            .latest_consensus(ConsensusFlavor::Microdesc, Some(false))
            .unwrap();
        assert!(not_found.is_none());

        // Mark it usable
        store.mark_consensus_usable(&cmeta).unwrap();

        // Now should find it when looking for non-pending
        let found = store
            .latest_consensus(ConsensusFlavor::Microdesc, Some(false))
            .unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn test_microdescs() {
        let mut store = InMemoryStore::new(false);
        let digest: MdDigest = [42u8; 32];
        let content = "test microdesc";
        let when = SystemTime::now();

        store.store_microdescs(&[(content, &digest)], when).unwrap();

        let found = store.microdescs(&[digest]).unwrap();
        assert_eq!(found.get(&digest), Some(&content.to_string()));
    }
}