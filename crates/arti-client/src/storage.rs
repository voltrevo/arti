//! Unified key-value storage trait for custom backends.
//!
//! This module re-exports [`KeyValueStore`] and [`StorageError`] from
//! [`tor_persist`], and provides [`split_storage`] to create both a state
//! manager and a directory store from a single [`KeyValueStore`].
//!
//! # Key Conventions
//!
//! - **State keys** are prefixed with `"state:"` by the state manager.
//!   For example, the key `"guards"` becomes `"state:guards"` in the store.
//! - **Directory keys** already include a `"dir:"` prefix (e.g.,
//!   `"dir:consensus:microdesc:abc123"`). The directory store passes these
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

pub use tor_persist::{KeyValueStore, StorageError};

/// Split a single [`KeyValueStore`] into both a state manager and a directory store.
///
/// This creates two views of the same underlying store:
/// - A state manager that prefixes all keys with `"state:"`
/// - A directory store that passes keys through as-is (they already have `"dir:"` prefix)
pub fn split_storage<S: KeyValueStore + 'static>(
    store: S,
) -> (tor_persist::AnyStateMgr, tor_dirmgr::BoxedDirStore) {
    let shared: Arc<dyn KeyValueStore> = Arc::new(store);

    let statemgr = tor_persist::AnyStateMgr::from_custom(Arc::clone(&shared));
    let dirstore = tor_dirmgr::BoxedDirStore::new(shared);

    (statemgr, dirstore)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use tor_persist::{LockStatus, StateMgr};

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

        fn can_store(&self) -> Result<bool, StorageError> {
            Ok(*self.locked.read().map_err(|e| e.to_string())?)
        }

        fn unlock(&self) -> Result<(), StorageError> {
            *self.locked.write().unwrap() = false;
            Ok(())
        }

        fn wait_for_unlock(
            &self,
        ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + Send + Sync + 'static>> {
            Box::pin(futures::future::ready(()))
        }
    }

    #[test]
    fn state_adapter_prefixes_keys() {
        let (statemgr, _dirstore) = split_storage(MemStore::new());

        // Lock so we can store
        assert_eq!(statemgr.try_lock().unwrap(), LockStatus::NewlyAcquired);
        assert!(statemgr.can_store());

        // Store via state manager (StateMgr::store serializes to JSON)
        statemgr.store("guards", &42_i32).unwrap();

        // Load back
        let loaded: Option<i32> = statemgr.load("guards").unwrap();
        assert_eq!(loaded, Some(42));

        // Missing key
        let missing: Option<String> = statemgr.load("missing").unwrap();
        assert!(missing.is_none());
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
