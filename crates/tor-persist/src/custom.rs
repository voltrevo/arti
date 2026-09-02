//! Key-value storage trait and unified state manager enum.
//!
//! This module provides [`KeyValueStore`], a simple key-value trait that users
//! implement to provide custom storage for both state persistence and directory
//! cache. [`AnyStateMgr`] dispatches between the native [`FsStateMgr`] and a
//! custom [`KeyValueStore`] backend.

use crate::err::{Action, Resource};
use crate::{Error, ErrorSource, LockStatus, Result, StateMgr};
#[cfg(not(target_arch = "wasm32"))]
use futures::future::Either;
use serde::{de::DeserializeOwned, Serialize};
use std::pin::Pin;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::FsStateMgr;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

/// Error type for [`KeyValueStore`] operations.
pub type StorageError = Box<dyn std::error::Error + Send + Sync>;

/// A simple key-value storage backend.
///
/// Implement this trait once to provide both state persistence and directory
/// cache storage. Use [`TorClientBuilder::storage()`](arti_client::TorClientBuilder::storage)
/// to wire it in, or call [`split_storage()`](arti_client::storage::split_storage) directly.
///
/// Locking is shared between state and directory storage -- when the store
/// is locked, both sides can write.
pub trait KeyValueStore: Send + Sync {
    /// Load a value by key. Returns `Ok(None)` if the key does not exist.
    fn get(&self, key: &str) -> std::result::Result<Option<String>, StorageError>;

    /// Store a value by key, replacing any previous value.
    fn set(&self, key: &str, value: &str) -> std::result::Result<(), StorageError>;

    /// Delete a key. Not an error if the key does not exist.
    fn delete(&self, key: &str) -> std::result::Result<(), StorageError>;

    /// List all keys whose names begin with `prefix`.
    fn keys(&self, prefix: &str) -> std::result::Result<Vec<String>, StorageError>;

    /// Try to acquire exclusive write access.
    ///
    /// Returns `Ok(true)` if the lock was newly acquired, `Ok(false)` if
    /// already held. Implementations may use file locks, Web Locks API,
    /// or any other advisory locking mechanism.
    fn try_lock(&self) -> std::result::Result<bool, StorageError>;

    /// Return true if this store currently holds the write lock.
    ///
    /// Returns `false` if the lock is not held by this instance, regardless
    /// of whether another instance holds it.
    fn can_store(&self) -> std::result::Result<bool, StorageError>;

    /// Release the write lock.
    fn unlock(&self) -> std::result::Result<(), StorageError>;

    /// Return a future that resolves when this store is dropped/unlocked.
    ///
    /// Callers use this to wait until exclusive access becomes available.
    fn wait_for_unlock(
        &self,
    ) -> Pin<Box<dyn futures::Future<Output = ()> + Send + Sync + 'static>>;
}

/// A state manager that dispatches between the native filesystem backend
/// and a custom [`KeyValueStore`] backend.
///
/// On native platforms, the default is [`FsStateMgr`] (zero overhead).
/// Custom storage can be provided via [`AnyStateMgr::from_custom`].
///
/// On WASM, custom storage must always be provided.
#[derive(Clone)]
#[non_exhaustive]
pub enum AnyStateMgr {
    /// Filesystem-based storage (native only).
    #[cfg(not(target_arch = "wasm32"))]
    Fs(FsStateMgr),
    /// Custom key-value storage backend.
    Custom(Arc<dyn KeyValueStore>),
}

impl AnyStateMgr {
    /// Create an `AnyStateMgr` from a custom [`KeyValueStore`] implementation.
    pub fn from_custom(storage: Arc<dyn KeyValueStore>) -> Self {
        Self::Custom(storage)
    }

    /// Construct from a filesystem path (native only).
    ///
    /// This creates an [`FsStateMgr`] and wraps it in the `Fs` variant.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_path_and_mistrust<P: AsRef<Path>>(
        path: P,
        mistrust: &fs_mistrust::Mistrust,
    ) -> Result<Self> {
        Ok(Self::Fs(FsStateMgr::from_path_and_mistrust(
            path, mistrust,
        )?))
    }

    /// Return the storage path, if this is a filesystem-backed manager.
    ///
    /// Returns `None` for custom storage backends.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Fs(fs) => Some(fs.path()),
            Self::Custom(_) => None,
        }
    }

    /// Return a future that resolves when this manager is dropped/unlocked.
    ///
    /// For filesystem-backed managers, this waits for the lock file to be released.
    /// For custom backends, this defers to [`KeyValueStore::wait_for_unlock`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait_for_unlock(
        &self,
    ) -> impl futures::Future<Output = ()> + Send + Sync + 'static {
        match self {
            Self::Fs(fs) => Either::Left(fs.wait_for_unlock()),
            Self::Custom(s) => Either::Right(s.wait_for_unlock()),
        }
    }

    /// Return a future that resolves when this manager is dropped/unlocked.
    ///
    /// Defers to [`KeyValueStore::wait_for_unlock`].
    #[cfg(target_arch = "wasm32")]
    pub fn wait_for_unlock(
        &self,
    ) -> impl futures::Future<Output = ()> + Send + Sync + 'static {
        match self {
            Self::Custom(s) => s.wait_for_unlock(),
        }
    }

    /// Helper to create an error for a given key and action.
    fn make_error(source: ErrorSource, action: Action, key: &str) -> Error {
        Error::new(
            source,
            action,
            Resource::Memory {
                key: key.to_string(),
            },
        )
    }
}

impl AnyStateMgr {
    /// Add the `"state:"` prefix to a key for custom storage.
    fn prefixed(key: &str) -> String {
        format!("state:{}", key)
    }
}

impl StateMgr for AnyStateMgr {
    fn load<D>(&self, key: &str) -> Result<Option<D>>
    where
        D: DeserializeOwned,
    {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.load(key),
            Self::Custom(s) => {
                let prefixed = Self::prefixed(key);
                match s.get(&prefixed).map_err(|e| {
                    Error::load_error(key, std::io::Error::other(e))
                })? {
                    Some(json_str) => {
                        let value: D = serde_json::from_str(&json_str).map_err(|e| {
                            Self::make_error(Arc::new(e).into(), Action::Loading, key)
                        })?;
                        Ok(Some(value))
                    }
                    None => Ok(None),
                }
            }
        }
    }

    fn store<S>(&self, key: &str, val: &S) -> Result<()>
    where
        S: Serialize,
    {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.store(key, val),
            Self::Custom(s) => {
                if !s.can_store().unwrap_or(false) {
                    return Err(Self::make_error(ErrorSource::NoLock, Action::Storing, key));
                }

                let json_str = serde_json::to_string_pretty(val).map_err(|e| {
                    Self::make_error(Arc::new(e).into(), Action::Storing, key)
                })?;

                let prefixed = Self::prefixed(key);
                s.set(&prefixed, &json_str).map_err(|e| {
                    Error::store_error(key, std::io::Error::other(e))
                })
            }
        }
    }

    fn can_store(&self) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.can_store(),
            Self::Custom(s) => s.can_store().unwrap_or(false),
        }
    }

    fn try_lock(&self) -> Result<LockStatus> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.try_lock(),
            Self::Custom(s) => match s.try_lock() {
                Ok(true) => Ok(LockStatus::NewlyAcquired),
                Ok(false) => Ok(LockStatus::AlreadyHeld),
                Err(e) => Err(Error::lock_error(std::io::Error::other(e))),
            },
        }
    }

    fn unlock(&self) -> Result<()> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.unlock(),
            Self::Custom(s) => s.unlock().map_err(|e| {
                Error::unlock_error(std::io::Error::other(e))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::RwLock;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestData {
        /// Name field.
        name: String,
        /// Value field.
        value: i32,
    }

    /// A simple in-memory implementation for testing.
    struct TestStorage {
        /// Data store.
        data: RwLock<HashMap<String, String>>,
        /// Lock state.
        locked: RwLock<bool>,
    }

    impl TestStorage {
        /// Create a new empty test storage.
        fn new() -> Self {
            Self {
                data: RwLock::new(HashMap::new()),
                locked: RwLock::new(false),
            }
        }
    }

    impl KeyValueStore for TestStorage {
        fn get(&self, key: &str) -> std::result::Result<Option<String>, StorageError> {
            let data = self.data.read().unwrap();
            Ok(data.get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> std::result::Result<(), StorageError> {
            let mut data = self.data.write().unwrap();
            data.insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, key: &str) -> std::result::Result<(), StorageError> {
            self.data.write().unwrap().remove(key);
            Ok(())
        }

        fn keys(&self, prefix: &str) -> std::result::Result<Vec<String>, StorageError> {
            Ok(self
                .data
                .read()
                .unwrap()
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect())
        }

        fn can_store(&self) -> std::result::Result<bool, StorageError> {
            Ok(*self.locked.read().unwrap())
        }

        fn try_lock(&self) -> std::result::Result<bool, StorageError> {
            let mut locked = self.locked.write().unwrap();
            if *locked {
                Ok(false)
            } else {
                *locked = true;
                Ok(true)
            }
        }

        fn unlock(&self) -> std::result::Result<(), StorageError> {
            *self.locked.write().unwrap() = false;
            Ok(())
        }

        fn wait_for_unlock(
            &self,
        ) -> Pin<Box<dyn futures::Future<Output = ()> + Send + Sync + 'static>> {
            Box::pin(futures::future::ready(()))
        }
    }

    #[test]
    fn test_any_state_mgr() {
        let storage = TestStorage::new();
        let mgr = AnyStateMgr::from_custom(Arc::new(storage));

        // Lock the manager
        let status = mgr.try_lock().unwrap();
        assert_eq!(status, LockStatus::NewlyAcquired);
        assert!(mgr.can_store());

        // Store some data
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        mgr.store("test_key", &data).unwrap();

        // Load it back
        let loaded: Option<TestData> = mgr.load("test_key").unwrap();
        assert_eq!(loaded, Some(data));

        // Non-existent key
        let missing: Option<TestData> = mgr.load("missing").unwrap();
        assert!(missing.is_none());
    }
}
