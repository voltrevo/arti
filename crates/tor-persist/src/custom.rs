//! Object-safe custom storage trait for WASM environments.
//!
//! This module provides an object-safe trait [`CustomStateMgr`] that can be
//! implemented by external crates (like tor-js) to provide custom storage
//! backends.
//!
//! The [`BoxedStateMgr`] wrapper implements the full [`StateMgr`] trait while
//! delegating to a boxed [`CustomStateMgr`], handling JSON serialization
//! internally.

use crate::err::{Action, Resource};
use crate::{Error, ErrorSource, LockStatus, Result, StateMgr};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

/// An object-safe trait for custom storage backends.
///
/// This trait provides a simplified interface for external storage implementations
/// that work with JSON strings instead of generic types. This allows the trait
/// to be object-safe and used with `Box<dyn CustomStateMgr>`.
///
/// # Example
///
/// ```ignore
/// use tor_persist::{CustomStateMgr, LockStatus};
///
/// struct MyStorage {
///     // ... storage implementation
/// }
///
/// impl CustomStateMgr for MyStorage {
///     fn load_json(&self, key: &str) -> tor_persist::Result<Option<String>> {
///         // Load JSON string from your storage
///     }
///
///     fn store_json(&self, key: &str, value: &str) -> tor_persist::Result<()> {
///         // Store JSON string to your storage
///     }
///
///     // ... implement other methods
/// }
/// ```
///
/// # Thread Safety
///
/// On native platforms, this trait requires `Send + Sync` for multi-threaded use.
/// On WASM, these bounds are relaxed since WASM is single-threaded.
#[cfg(not(target_arch = "wasm32"))]
pub trait CustomStateMgr: Send + Sync {
    /// Load a value as a JSON string from storage.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    fn load_json(&self, key: &str) -> Result<Option<String>>;

    /// Store a JSON string value to storage.
    fn store_json(&self, key: &str, value: &str) -> Result<()>;

    /// Return true if this storage is writable (lock is held).
    fn can_store(&self) -> bool;

    /// Try to acquire the lock for exclusive write access.
    fn try_lock(&self) -> Result<LockStatus>;

    /// Release the lock.
    fn unlock(&self) -> Result<()>;
}

/// An object-safe trait for custom storage backends (WASM version).
///
/// On WASM, types that implement this trait should also implement `Send + Sync`
/// (even though WASM is single-threaded) because other parts of arti require
/// these bounds.
#[cfg(target_arch = "wasm32")]
pub trait CustomStateMgr: Send + Sync {
    /// Load a value as a JSON string from storage.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    fn load_json(&self, key: &str) -> Result<Option<String>>;

    /// Store a JSON string value to storage.
    fn store_json(&self, key: &str, value: &str) -> Result<()>;

    /// Return true if this storage is writable (lock is held).
    fn can_store(&self) -> bool;

    /// Try to acquire the lock for exclusive write access.
    fn try_lock(&self) -> Result<LockStatus>;

    /// Release the lock.
    fn unlock(&self) -> Result<()>;
}

/// A wrapper that implements [`StateMgr`] for any [`CustomStateMgr`].
///
/// This allows custom storage implementations to be used anywhere a `StateMgr`
/// is expected. JSON serialization/deserialization is handled automatically.
#[derive(Clone)]
#[cfg(not(target_arch = "wasm32"))]
pub struct BoxedStateMgr {
    inner: Arc<dyn CustomStateMgr + Send + Sync>,
}

/// A wrapper that implements [`StateMgr`] for any [`CustomStateMgr`] (WASM version).
#[derive(Clone)]
#[cfg(target_arch = "wasm32")]
pub struct BoxedStateMgr {
    inner: Arc<dyn CustomStateMgr + Send + Sync>,
}

#[cfg(not(target_arch = "wasm32"))]
impl BoxedStateMgr {
    /// Create a new `BoxedStateMgr` from a custom storage implementation.
    pub fn new<S: CustomStateMgr + Send + Sync + 'static>(storage: S) -> Self {
        Self {
            inner: Arc::new(storage),
        }
    }

    /// Create a new `BoxedStateMgr` from a boxed custom storage.
    pub fn from_box(storage: Box<dyn CustomStateMgr + Send + Sync>) -> Self {
        Self {
            inner: Arc::from(storage),
        }
    }

    /// Create a new `BoxedStateMgr` from an Arc'd custom storage.
    pub fn from_arc(storage: Arc<dyn CustomStateMgr + Send + Sync>) -> Self {
        Self { inner: storage }
    }

    /// Helper to create an error for a given key and action.
    fn make_error(&self, source: ErrorSource, action: Action, key: &str) -> Error {
        Error::new(
            source,
            action,
            Resource::Memory {
                key: key.to_string(),
            },
        )
    }
}

#[cfg(target_arch = "wasm32")]
impl BoxedStateMgr {
    /// Create a new `BoxedStateMgr` from a custom storage implementation.
    pub fn new<S: CustomStateMgr + Send + Sync + 'static>(storage: S) -> Self {
        Self {
            inner: Arc::new(storage),
        }
    }

    /// Create a new `BoxedStateMgr` from a boxed custom storage.
    pub fn from_box(storage: Box<dyn CustomStateMgr + Send + Sync>) -> Self {
        Self {
            inner: Arc::from(storage),
        }
    }

    /// Create a new `BoxedStateMgr` from an Arc'd custom storage.
    pub fn from_arc(storage: Arc<dyn CustomStateMgr + Send + Sync>) -> Self {
        Self { inner: storage }
    }

    /// Helper to create an error for a given key and action.
    fn make_error(&self, source: ErrorSource, action: Action, key: &str) -> Error {
        Error::new(
            source,
            action,
            Resource::Memory {
                key: key.to_string(),
            },
        )
    }
}

impl StateMgr for BoxedStateMgr {
    fn load<D>(&self, key: &str) -> Result<Option<D>>
    where
        D: DeserializeOwned,
    {
        match self.inner.load_json(key)? {
            Some(json_str) => {
                let value: D = serde_json::from_str(&json_str).map_err(|e| {
                    self.make_error(Arc::new(e).into(), Action::Loading, key)
                })?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn store<S>(&self, key: &str, val: &S) -> Result<()>
    where
        S: Serialize,
    {
        if !self.can_store() {
            return Err(self.make_error(ErrorSource::NoLock, Action::Storing, key));
        }

        let json_str = serde_json::to_string(val)
            .map_err(|e| self.make_error(Arc::new(e).into(), Action::Storing, key))?;

        self.inner.store_json(key, &json_str)
    }

    fn can_store(&self) -> bool {
        self.inner.can_store()
    }

    fn try_lock(&self) -> Result<LockStatus> {
        self.inner.try_lock()
    }

    fn unlock(&self) -> Result<()> {
        self.inner.unlock()
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

    impl CustomStateMgr for TestStorage {
        fn load_json(&self, key: &str) -> Result<Option<String>> {
            let data = self.data.read().unwrap();
            Ok(data.get(key).cloned())
        }

        fn store_json(&self, key: &str, value: &str) -> Result<()> {
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
    fn test_boxed_state_mgr() {
        let storage = TestStorage::new();
        let mgr = BoxedStateMgr::new(storage);

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
