//! Unified key-value storage trait for custom backends.
//!
//! This module provides [`KeyValueStore`], a single trait that users implement
//! to provide custom storage for both state persistence and directory cache.
//! The [`split_storage`] function creates two adapters from a single store,
//! using key prefixes to separate state and directory data.
//!
//! # Key Conventions
//!
//! - **State keys** are prefixed with `"state:"` by the state adapter.
//!   For example, the key `"guards"` becomes `"state:guards"` in the store.
//! - **Directory keys** already include a `"dir:"` prefix (e.g.,
//!   `"dir:consensus:microdesc:abc123"`). The directory adapter passes these
//!   through unchanged.
//!
//! # Example
//!
//! ```ignore
//! use arti_client::{TorClient, KeyValueStore};
//!
//! struct MyStore { /* ... */ }
//! impl KeyValueStore for MyStore { /* ... */ }
//!
//! let client = TorClient::builder()
//!     .storage(MyStore::new())
//!     .create_bootstrapped()
//!     .await?;
//! ```

use std::sync::Arc;
use tor_dirmgr::CustomDirStore;
use tor_persist::{LockStatus, StringStore};

/// Error type for [`KeyValueStore`] operations.
pub type StorageError = Box<dyn std::error::Error + Send + Sync>;

/// A simple key-value storage backend.
///
/// Implement this trait once to provide both state persistence and directory
/// cache storage. Use [`TorClientBuilder::storage()`](crate::TorClientBuilder::storage)
/// to wire it in, or call [`split_storage`] directly.
///
/// Locking is shared between state and directory storage — when the store
/// is locked, both sides can write.
pub trait KeyValueStore: Send + Sync {
    /// Load a value by key. Returns `Ok(None)` if the key does not exist.
    fn get(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Store a value by key, replacing any previous value.
    fn set(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Delete a key. Not an error if the key does not exist.
    fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// List all keys whose names begin with `prefix`.
    fn keys(&self, prefix: &str) -> Result<Vec<String>, StorageError>;

    /// Try to acquire exclusive write access.
    ///
    /// Returns `Ok(true)` if the lock was newly acquired, `Ok(false)` if
    /// already held. Implementations may use file locks, Web Locks API,
    /// or any other advisory locking mechanism.
    fn try_lock(&self) -> Result<bool, StorageError>;

    /// Return true if this store currently holds the write lock.
    fn is_locked(&self) -> Result<bool, StorageError>;

    /// Release the write lock.
    fn unlock(&self) -> Result<(), StorageError>;
}

/// Split a single [`KeyValueStore`] into both a state manager and a directory store.
///
/// This creates two adapters that share the same underlying store:
/// - A state manager that prefixes all keys with `"state:"`
/// - A directory store that passes keys through as-is (they already have `"dir:"` prefix)
pub fn split_storage<S: KeyValueStore + 'static>(
    store: S,
) -> (tor_persist::AnyStateMgr, tor_dirmgr::BoxedDirStore) {
    let shared: Arc<dyn KeyValueStore> = Arc::new(store);

    let state_adapter = KvStateAdapter {
        store: Arc::clone(&shared),
    };
    let dir_adapter = KvDirAdapter {
        store: shared,
    };

    let statemgr = tor_persist::AnyStateMgr::from_custom(state_adapter);
    let dirstore = tor_dirmgr::BoxedDirStore::new(dir_adapter);

    (statemgr, dirstore)
}

// ============================================================================
// KvStateAdapter — implements StringStore
// ============================================================================

/// Adapter that implements [`StringStore`] on top of a [`KeyValueStore`].
///
/// Adds a `"state:"` prefix to all keys.
struct KvStateAdapter {
    store: Arc<dyn KeyValueStore>,
}

impl KvStateAdapter {
    fn prefixed(key: &str) -> String {
        format!("state:{}", key)
    }
}

impl StringStore for KvStateAdapter {
    fn load_str(&self, key: &str) -> tor_persist::Result<Option<String>> {
        self.store
            .get(&Self::prefixed(key))
            .map_err(|e| tor_persist::Error::load_error(key, std::io::Error::other(e)))
    }

    fn store_str(&self, key: &str, value: &str) -> tor_persist::Result<()> {
        self.store
            .set(&Self::prefixed(key), value)
            .map_err(|e| tor_persist::Error::store_error(key, std::io::Error::other(e)))
    }

    fn is_locked(&self) -> tor_persist::Result<bool> {
        self.store
            .is_locked()
            .map_err(|e| tor_persist::Error::lock_error(std::io::Error::other(e)))
    }

    fn try_lock(&self) -> tor_persist::Result<LockStatus> {
        match self.store.try_lock() {
            Ok(true) => Ok(LockStatus::NewlyAcquired),
            Ok(false) => Ok(LockStatus::AlreadyHeld),
            Err(e) => Err(tor_persist::Error::lock_error(std::io::Error::other(e))),
        }
    }

    fn unlock(&self) -> tor_persist::Result<()> {
        self.store
            .unlock()
            .map_err(|e| tor_persist::Error::unlock_error(std::io::Error::other(e)))
    }
}

// ============================================================================
// KvDirAdapter — implements CustomDirStore
// ============================================================================

/// Adapter that implements [`CustomDirStore`] on top of a [`KeyValueStore`].
///
/// Directory keys already include the `"dir:"` prefix, so no prefix is added.
struct KvDirAdapter {
    store: Arc<dyn KeyValueStore>,
}

impl CustomDirStore for KvDirAdapter {
    fn load(&self, key: &str) -> tor_dirmgr::Result<Option<String>> {
        self.store
            .get(key)
            .map_err(|e| {
                tracing::warn!("custom dir store load error: {}", e);
                tor_dirmgr::Error::CacheCorruption("custom storage read failed")
            })
    }

    fn store(&self, key: &str, value: &str) -> tor_dirmgr::Result<()> {
        if !self.store.is_locked().unwrap_or(false) {
            return Err(tor_dirmgr::Error::CacheCorruption("store is read-only"));
        }
        self.store.set(key, value).map_err(|e| {
            tracing::warn!("custom dir store write error: {}", e);
            tor_dirmgr::Error::CacheCorruption("custom storage write failed")
        })
    }

    fn delete(&self, key: &str) -> tor_dirmgr::Result<()> {
        if !self.store.is_locked().unwrap_or(false) {
            return Err(tor_dirmgr::Error::CacheCorruption("store is read-only"));
        }
        self.store.delete(key).map_err(|e| {
            tracing::warn!("custom dir store delete error: {}", e);
            tor_dirmgr::Error::CacheCorruption("custom storage delete failed")
        })
    }

    fn keys(&self, prefix: &str) -> tor_dirmgr::Result<Vec<String>> {
        self.store.keys(prefix).map_err(|e| {
            tracing::warn!("custom dir store keys error: {}", e);
            tor_dirmgr::Error::CacheCorruption("custom storage keys failed")
        })
    }

    fn is_readonly(&self) -> bool {
        !self.store.is_locked().unwrap_or(false)
    }

    fn upgrade_to_readwrite(&mut self) -> tor_dirmgr::Result<bool> {
        self.store.try_lock().map_err(|e| {
            tracing::warn!("custom dir store lock error: {}", e);
            tor_dirmgr::Error::CacheCorruption("custom storage lock failed")
        }).map(|newly| {
            // try_lock returns true if newly acquired, false if already held.
            // upgrade_to_readwrite returns true if we now have write access.
            // Either way, we have write access.
            let _ = newly;
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use tor_persist::StateMgr;

    /// Simple in-memory KeyValueStore for testing.
    struct MemStore {
        data: RwLock<HashMap<String, String>>,
        locked: RwLock<bool>,
    }

    impl MemStore {
        fn new() -> Self {
            Self {
                data: RwLock::new(HashMap::new()),
                locked: RwLock::new(false),
            }
        }
    }

    impl KeyValueStore for MemStore {
        fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.data.read().unwrap().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.data
                .write()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), StorageError> {
            self.data.write().unwrap().remove(key);
            Ok(())
        }

        fn keys(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
            Ok(self
                .data
                .read()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }

        fn try_lock(&self) -> Result<bool, StorageError> {
            let mut locked = self.locked.write().unwrap();
            if *locked {
                Ok(false)
            } else {
                *locked = true;
                Ok(true)
            }
        }

        fn is_locked(&self) -> Result<bool, StorageError> {
            Ok(*self.locked.read().map_err(|e| e.to_string())?)
        }

        fn unlock(&self) -> Result<(), StorageError> {
            *self.locked.write().unwrap() = false;
            Ok(())
        }
    }

    #[test]
    fn state_adapter_prefixes_keys() {
        let (statemgr, _dirstore) = split_storage(MemStore::new());

        // Lock so we can store
        assert_eq!(statemgr.try_lock().unwrap(), LockStatus::NewlyAcquired);
        assert!(statemgr.can_store());

        // Store via state manager (StateMgr::store serializes to JSON)
        statemgr.store("guards", &42i32).unwrap();

        // Load back
        let loaded: Option<i32> = statemgr.load("guards").unwrap();
        assert_eq!(loaded, Some(42));

        // Missing key
        let missing: Option<String> = statemgr.load("missing").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn dir_adapter_passes_keys_through() {
        // Test the KvDirAdapter directly
        let store: Arc<dyn KeyValueStore> = Arc::new(MemStore::new());
        let mut adapter = KvDirAdapter {
            store: Arc::clone(&store),
        };

        // Initially readonly
        assert!(adapter.is_readonly());

        // Upgrade to readwrite (acquires lock on underlying store)
        assert_eq!(adapter.upgrade_to_readwrite().unwrap(), true);
        assert!(!adapter.is_readonly());

        // Store a dir key (keys already include "dir:" prefix from BoxedDirStore)
        adapter.store("dir:consensus:test", "consensus data").unwrap();

        // Load it back
        let loaded = adapter.load("dir:consensus:test").unwrap();
        assert_eq!(loaded.as_deref(), Some("consensus data"));

        // Keys
        let keys = adapter.keys("dir:consensus:").unwrap();
        assert_eq!(keys, vec!["dir:consensus:test"]);

        // Delete
        adapter.delete("dir:consensus:test").unwrap();
        assert!(adapter.load("dir:consensus:test").unwrap().is_none());
    }

    #[test]
    fn shared_lock_state() {
        let (statemgr, _dirstore) = split_storage(MemStore::new());

        // Initially not locked
        assert!(!statemgr.can_store());

        // Lock via state manager
        assert_eq!(statemgr.try_lock().unwrap(), LockStatus::NewlyAcquired);
        assert!(statemgr.can_store());

        // Lock again — already held
        assert_eq!(statemgr.try_lock().unwrap(), LockStatus::AlreadyHeld);

        // Unlock
        statemgr.unlock().unwrap();
        assert!(!statemgr.can_store());
    }
}
