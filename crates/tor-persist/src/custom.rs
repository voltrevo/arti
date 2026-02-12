//! Object-safe custom storage trait and unified state manager enum.
//!
//! This module provides [`StringStore`], an object-safe trait for custom storage
//! backends that work with JSON strings, and [`AnyStateMgr`], an enum that
//! dispatches between the native [`FsStateMgr`] and a custom [`StringStore`].

use crate::err::{Action, Resource};
use crate::{Error, ErrorSource, LockStatus, Result, StateMgr};
use futures::future::Either;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::FsStateMgr;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

/// An object-safe trait for custom storage backends.
///
/// This trait provides a simplified interface for external storage implementations
/// that work with JSON strings instead of generic types. This allows the trait
/// to be object-safe and used with `Arc<dyn StringStore>`.
///
/// # Example
///
/// ```ignore
/// use tor_persist::{StringStore, LockStatus};
///
/// struct MyStorage {
///     // ... storage implementation
/// }
///
/// impl StringStore for MyStorage {
///     fn load_str(&self, key: &str) -> tor_persist::Result<Option<String>> {
///         // Load JSON string from your storage
///         # Ok(None)
///     }
///
///     fn store_str(&self, key: &str, value: &str) -> tor_persist::Result<()> {
///         // Store JSON string to your storage
///         # Ok(())
///     }
///
///     // ... implement other methods
///     # fn can_store(&self) -> bool { true }
///     # fn try_lock(&self) -> tor_persist::Result<LockStatus> { Ok(LockStatus::AlreadyHeld) }
///     # fn unlock(&self) -> tor_persist::Result<()> { Ok(()) }
/// }
/// ```
pub trait StringStore: Send + Sync {
    /// Load a value as a JSON string from storage.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    fn load_str(&self, key: &str) -> Result<Option<String>>;

    /// Store a JSON string value to storage.
    fn store_str(&self, key: &str, value: &str) -> Result<()>;

    /// Return true if this storage is writable (lock is held).
    fn can_store(&self) -> bool;

    /// Try to acquire the lock for exclusive write access.
    fn try_lock(&self) -> Result<LockStatus>;

    /// Release the lock.
    fn unlock(&self) -> Result<()>;
}

/// A state manager that dispatches between the native filesystem backend
/// and a custom [`StringStore`] backend.
///
/// On native platforms, the default is [`FsStateMgr`] (zero overhead).
/// Custom storage can be provided via [`AnyStateMgr::from_custom`].
///
/// On WASM, custom storage must always be provided.
#[derive(Clone)]
pub enum AnyStateMgr {
    /// Filesystem-based storage (native only).
    #[cfg(not(target_arch = "wasm32"))]
    Fs(FsStateMgr),
    /// Custom string-based storage backend.
    Custom(Arc<dyn StringStore>),
}

impl AnyStateMgr {
    /// Create an `AnyStateMgr` from a custom [`StringStore`] implementation.
    pub fn from_custom<S: StringStore + 'static>(storage: S) -> Self {
        Self::Custom(Arc::new(storage))
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
    /// For custom backends, this resolves immediately.
    pub fn wait_for_unlock(
        &self,
    ) -> impl futures::Future<Output = ()> + Send + Sync + 'static {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => Either::Left(fs.wait_for_unlock()),
            Self::Custom(_) => Either::Right(futures::future::ready(())),
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

impl StateMgr for AnyStateMgr {
    fn load<D>(&self, key: &str) -> Result<Option<D>>
    where
        D: DeserializeOwned,
    {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.load(key),
            Self::Custom(s) => match s.load_str(key)? {
                Some(json_str) => {
                    let value: D = serde_json::from_str(&json_str).map_err(|e| {
                        Self::make_error(Arc::new(e).into(), Action::Loading, key)
                    })?;
                    Ok(Some(value))
                }
                None => Ok(None),
            },
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
                if !s.can_store() {
                    return Err(Self::make_error(ErrorSource::NoLock, Action::Storing, key));
                }

                let json_str = serde_json::to_string_pretty(val).map_err(|e| {
                    Self::make_error(Arc::new(e).into(), Action::Storing, key)
                })?;

                s.store_str(key, &json_str)
            }
        }
    }

    fn can_store(&self) -> bool {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.can_store(),
            Self::Custom(s) => s.can_store(),
        }
    }

    fn try_lock(&self) -> Result<LockStatus> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.try_lock(),
            Self::Custom(s) => s.try_lock(),
        }
    }

    fn unlock(&self) -> Result<()> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Fs(fs) => fs.unlock(),
            Self::Custom(s) => s.unlock(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::RwLock;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    /// A simple in-memory implementation for testing.
    struct TestStorage {
        data: RwLock<HashMap<String, String>>,
        locked: RwLock<bool>,
    }

    impl TestStorage {
        fn new() -> Self {
            Self {
                data: RwLock::new(HashMap::new()),
                locked: RwLock::new(false),
            }
        }
    }

    impl StringStore for TestStorage {
        fn load_str(&self, key: &str) -> Result<Option<String>> {
            let data = self.data.read().unwrap();
            Ok(data.get(key).cloned())
        }

        fn store_str(&self, key: &str, value: &str) -> Result<()> {
            let mut data = self.data.write().unwrap();
            data.insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn can_store(&self) -> bool {
            *self.locked.read().unwrap()
        }

        fn try_lock(&self) -> Result<LockStatus> {
            let mut locked = self.locked.write().unwrap();
            if *locked {
                Ok(LockStatus::AlreadyHeld)
            } else {
                *locked = true;
                Ok(LockStatus::NewlyAcquired)
            }
        }

        fn unlock(&self) -> Result<()> {
            *self.locked.write().unwrap() = false;
            Ok(())
        }
    }

    #[test]
    fn test_any_state_mgr() {
        let storage = TestStorage::new();
        let mgr = AnyStateMgr::from_custom(storage);

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
