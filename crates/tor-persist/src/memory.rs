//! In-memory state manager for WASM and testing.
//!
//! This module provides a state manager that stores all state in memory.
//! State is lost when the page is reloaded (for WASM) or the process exits.

use crate::err::{Action, ErrorSource, Resource};
use crate::{Error, LockStatus, Result, StateMgr};
use futures::FutureExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// An in-memory state manager.
///
/// This stores all state in a `HashMap` in memory. It's useful for:
/// - WASM environments where filesystem access isn't available
/// - Testing where you don't want to touch the filesystem
/// - Ephemeral sessions where persistence isn't needed
///
/// # Limitations
///
/// - All state is lost when the process exits or page reloads
/// - No persistence across sessions
/// - The "lock" is always held (single-threaded WASM assumption)
#[derive(Clone, Debug)]
pub struct MemoryStateMgr {
    /// The internal state storage.
    inner: Arc<RwLock<MemoryStateMgrInner>>,
}

/// Internal state for MemoryStateMgr.
#[derive(Debug, Default)]
struct MemoryStateMgrInner {
    /// The stored data, keyed by string.
    data: HashMap<String, String>,
    /// Whether we "hold the lock" (always true for in-memory).
    locked: bool,
}

impl Default for MemoryStateMgr {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStateMgr {
    /// Create a new in-memory state manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MemoryStateMgrInner {
                data: HashMap::new(),
                locked: false,
            })),
        }
    }

    /// Create a new in-memory state manager that starts locked.
    ///
    /// This is useful when you know you'll want read-write access immediately.
    pub fn new_locked() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MemoryStateMgrInner {
                data: HashMap::new(),
                locked: true,
            })),
        }
    }

    /// Return the "path" for this state manager.
    ///
    /// For an in-memory state manager, this returns an empty path since
    /// there is no filesystem backing. This method exists for API compatibility
    /// with `FsStateMgr`.
    pub fn path(&self) -> &Path {
        // Return a static empty path - there's no real filesystem location
        static EMPTY_PATH: &str = "";
        Path::new(EMPTY_PATH)
    }

    /// Return a future that resolves when this state manager is unlocked.
    ///
    /// For an in-memory state manager, this returns a future that resolves
    /// immediately since there's no real file locking mechanism that other
    /// processes could be waiting on.
    pub fn wait_for_unlock(&self) -> impl futures::Future<Output = ()> + Send + Sync + 'static + use<> {
        // Return a future that resolves immediately
        futures::future::ready(())
    }

    /// Helper to create an error for this state manager.
    fn make_error(&self, source: ErrorSource, action: Action, key: &str) -> Error {
        Error::new(
            source,
            action,
            Resource::Memory {
                key: key.to_string(),
            },
        )
    }

    /// Helper to create an IO error.
    fn io_error(&self, msg: &str, action: Action, key: &str) -> Error {
        self.make_error(
            ErrorSource::IoError(Arc::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                msg,
            ))),
            action,
            key,
        )
    }
}

impl StateMgr for MemoryStateMgr {
    fn load<D>(&self, key: &str) -> Result<Option<D>>
    where
        D: DeserializeOwned,
    {
        let inner = self
            .inner
            .read()
            .map_err(|_| self.io_error("lock poisoned", Action::Loading, key))?;

        match inner.data.get(key) {
            Some(json_str) => {
                let value: D = serde_json::from_str(json_str)
                    .map_err(|e| self.make_error(Arc::new(e).into(), Action::Loading, key))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    fn store<S>(&self, key: &str, val: &S) -> Result<()>
    where
        S: Serialize,
    {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| self.io_error("lock poisoned", Action::Storing, key))?;

        if !inner.locked {
            return Err(self.make_error(ErrorSource::NoLock, Action::Storing, key));
        }

        let json_str = serde_json::to_string(val)
            .map_err(|e| self.make_error(Arc::new(e).into(), Action::Storing, key))?;

        inner.data.insert(key.to_string(), json_str);
        Ok(())
    }

    fn can_store(&self) -> bool {
        self.inner
            .read()
            .map(|inner| inner.locked)
            .unwrap_or(false)
    }

    fn try_lock(&self) -> Result<LockStatus> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| self.io_error("lock poisoned", Action::Locking, "<manager>"))?;

        if inner.locked {
            Ok(LockStatus::AlreadyHeld)
        } else {
            inner.locked = true;
            Ok(LockStatus::NewlyAcquired)
        }
    }

    fn unlock(&self) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| self.io_error("lock poisoned", Action::Unlocking, "<manager>"))?;

        inner.locked = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_memory_state_mgr_basic() {
        let mgr = MemoryStateMgr::new();

        // Initially not locked, so can_store is false
        assert!(!mgr.can_store());

        // Load returns None for non-existent key
        let result: Option<TestData> = mgr.load("test_key").expect("load should succeed");
        assert!(result.is_none());
    }

    #[test]
    fn test_memory_state_mgr_store_load() {
        let mgr = MemoryStateMgr::new();

        // Lock the manager
        let status = mgr.try_lock().expect("try_lock should succeed");
        assert_eq!(status, LockStatus::NewlyAcquired);
        assert!(mgr.can_store());

        // Store some data
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        mgr.store("test_key", &data).expect("store should succeed");

        // Load it back
        let loaded: Option<TestData> = mgr.load("test_key").expect("load should succeed");
        assert_eq!(loaded, Some(data));
    }

    #[test]
    fn test_memory_state_mgr_lock_status() {
        let mgr = MemoryStateMgr::new();

        // First lock
        let status = mgr.try_lock().expect("try_lock should succeed");
        assert_eq!(status, LockStatus::NewlyAcquired);

        // Second lock (already held)
        let status = mgr.try_lock().expect("try_lock should succeed");
        assert_eq!(status, LockStatus::AlreadyHeld);

        // Unlock
        mgr.unlock().expect("unlock should succeed");
        assert!(!mgr.can_store());

        // Lock again
        let status = mgr.try_lock().expect("try_lock should succeed");
        assert_eq!(status, LockStatus::NewlyAcquired);
    }

    #[test]
    fn test_memory_state_mgr_store_without_lock() {
        let mgr = MemoryStateMgr::new();

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // Should fail because we haven't locked
        let result = mgr.store("test_key", &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_state_mgr_new_locked() {
        let mgr = MemoryStateMgr::new_locked();
        assert!(mgr.can_store());

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        mgr.store("test_key", &data).expect("store should succeed");
    }
}
