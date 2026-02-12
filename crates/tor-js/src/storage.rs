//! JavaScript storage adapter for tor-js.
//!
//! This module provides [`CachedJsStorage`], a [`KeyValueStore`] implementation
//! that bridges async JavaScript storage APIs with Rust's sync storage traits
//! using a pre-load + cache + async write-back pattern:
//!
//! 1. During client creation (async), all data is loaded from JS storage
//! 2. Sync reads hit the in-memory cache
//! 3. Writes update the cache and schedule async persistence via spawn_local()
//!
//! [`KeyValueStore`]: arti_client::storage::KeyValueStore

use arti_client::storage::{KeyValueStore, StorageError};
use js_sys::Promise;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

// ============================================================================
// JS Storage Interface
// ============================================================================

/// JavaScript storage interface.
///
/// This is the interface that JavaScript code must implement to provide
/// custom storage. All methods return Promises.
/// FIXME: Why use Result on these methods? Distinguishing sync/async failure
/// is usually an anti-pattern in js. (Just put the failure in the promise
/// always.)
#[wasm_bindgen]
extern "C" {
    /// The JavaScript storage interface type.
    #[wasm_bindgen(typescript_type = "TorStorage")]
    pub type JsStorageInterface;

    /// Get a value by key. Returns null if not found.
    #[wasm_bindgen(method, catch)]
    fn get(this: &JsStorageInterface, key: &str) -> Result<Promise, JsValue>;

    /// Set a value by key.
    #[wasm_bindgen(method, catch)]
    fn set(this: &JsStorageInterface, key: &str, value: &str) -> Result<Promise, JsValue>;

    /// Delete a value by key.
    #[wasm_bindgen(method, catch)]
    fn delete(this: &JsStorageInterface, key: &str) -> Result<Promise, JsValue>;

    /// List all keys with a given prefix.
    #[wasm_bindgen(method, catch)]
    fn keys(this: &JsStorageInterface, prefix: &str) -> Result<Promise, JsValue>;

    /// Try to acquire an exclusive write lock.
    /// Returns a boolean: true if newly acquired, false if already held.
    #[wasm_bindgen(method, catch, js_name = "tryLock")]
    fn try_lock(this: &JsStorageInterface) -> Result<Promise, JsValue>;

    /// Release the write lock.
    #[wasm_bindgen(method, catch)]
    fn unlock(this: &JsStorageInterface) -> Result<Promise, JsValue>;
}

// ============================================================================
// JsStorage Wrapper
// ============================================================================

/// Rust wrapper around the JavaScript storage interface.
///
/// Provides async methods that convert JS Promises to Rust Futures.
pub struct JsStorage {
    inner: JsStorageInterface,
}

// JsStorageInterface is a JsValue wrapper, we can clone it via JsValue::clone()
impl Clone for JsStorage {
    fn clone(&self) -> Self {
        // Clone the underlying JsValue
        let inner_clone: JsStorageInterface = self.inner.clone().unchecked_into();
        Self { inner: inner_clone }
    }
}

// SAFETY: WASM is single-threaded, so it's safe to send JsValue between "threads"
// (there's only one thread). These impls are required because other parts of arti
// have Send bounds even on WASM.
unsafe impl Send for JsStorage {}
unsafe impl Sync for JsStorage {}

impl JsStorage {
    /// Create a new JsStorage from a JavaScript storage interface.
    pub fn new(interface: JsStorageInterface) -> Self {
        Self { inner: interface }
    }

    /// Get a value by key.
    pub async fn get(&self, key: &str) -> Result<Option<String>, JsValue> {
        let promise = self.inner.get(key)?;
        let result = JsFuture::from(promise).await?;
        if result.is_null() || result.is_undefined() {
            Ok(None)
        } else {
            Ok(result.as_string())
        }
    }

    /// Set a value by key.
    pub async fn set(&self, key: &str, value: &str) -> Result<(), JsValue> {
        let promise = self.inner.set(key, value)?;
        JsFuture::from(promise).await?;
        Ok(())
    }

    /// Delete a value by key.
    pub async fn delete(&self, key: &str) -> Result<(), JsValue> {
        let promise = self.inner.delete(key)?;
        JsFuture::from(promise).await?;
        Ok(())
    }

    /// List all keys with a given prefix.
    pub async fn keys(&self, prefix: &str) -> Result<Vec<String>, JsValue> {
        let promise = self.inner.keys(prefix)?;
        let result = JsFuture::from(promise).await?;

        // Convert JS array to Vec<String>
        let array = js_sys::Array::from(&result);
        let mut keys = Vec::with_capacity(array.length() as usize);
        for i in 0..array.length() {
            if let Some(key) = array.get(i).as_string() {
                keys.push(key);
            }
        }
        Ok(keys)
    }

    /// Try to acquire an exclusive write lock.
    pub async fn try_lock(&self) -> Result<bool, JsValue> {
        let promise = self.inner.try_lock()?;
        let result = JsFuture::from(promise).await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    /// Release the write lock.
    pub async fn unlock(&self) -> Result<(), JsValue> {
        let promise = self.inner.unlock()?;
        JsFuture::from(promise).await?;
        Ok(())
    }
}

// ============================================================================
// CachedJsStorage - KeyValueStore backed by JS storage with cache
// ============================================================================

/// Cached JavaScript storage implementing [`KeyValueStore`].
///
/// Bridges async JavaScript storage APIs with the sync [`KeyValueStore`] trait
/// using a pre-load + cache + async write-back pattern. All keys from JS
/// storage are loaded into an in-memory cache during construction, then:
///
/// - **Reads** hit the cache directly (sync)
/// - **Writes** update the cache and schedule async persistence via `spawn_local()`
/// - **Lock/unlock** delegates to JavaScript's `tryLock()`/`unlock()` methods
pub struct CachedJsStorage {
    /// The underlying JS storage (for async write-back).
    js_storage: JsStorage,
    /// In-memory cache for sync reads.
    cache: Arc<RwLock<HashMap<String, String>>>,
    /// Whether we currently hold the write lock.
    locked: Arc<RwLock<bool>>,
}

// SAFETY: WASM is single-threaded, so it's safe to send CachedJsStorage between "threads"
// (there's only one thread). The JsStorage inside contains JsValue which is not Send/Sync,
// but since WASM has no threads, this is safe.
unsafe impl Send for CachedJsStorage {}
unsafe impl Sync for CachedJsStorage {}

impl CachedJsStorage {
    /// Create a new CachedJsStorage, preloading all data from JS storage.
    ///
    /// This loads all `"state:"` and `"dir:"` prefixed keys from JS storage
    /// into the in-memory cache. This is necessary because the [`KeyValueStore`]
    /// trait is synchronous, but JS storage APIs are async.
    pub async fn new(js_storage: JsStorage) -> Result<Self, JsValue> {
        let storage = Self {
            js_storage,
            cache: Arc::new(RwLock::new(HashMap::new())),
            locked: Arc::new(RwLock::new(false)),
        };

        storage.preload_all().await?;

        Ok(storage)
    }

    /// Pre-load all data from JS storage into the cache.
    async fn preload_all(&self) -> Result<(), JsValue> {
        let mut cache = self
            .cache
            .write()
            .map_err(|_| JsValue::from_str("cache lock poisoned"))?;

        // Load state keys
        let state_keys = self.js_storage.keys("state:").await?;
        for key in state_keys {
            if let Some(value) = self.js_storage.get(&key).await? {
                cache.insert(key, value);
            }
        }

        // Load dir keys
        let dir_keys = self.js_storage.keys("dir:").await?;
        for key in dir_keys {
            if let Some(value) = self.js_storage.get(&key).await? {
                cache.insert(key, value);
            }
        }

        tracing::debug!("CachedJsStorage: preloaded {} entries", cache.len());
        Ok(())
    }

    /// Schedule an async write to JS storage.
    fn schedule_persist(&self, key: String, value: String) {
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.set(&key, &value).await {
                tracing::warn!("CachedJsStorage: failed to persist key {}: {:?}", key, e);
            }
        });
    }

    /// Schedule an async delete from JS storage.
    fn schedule_delete(&self, key: String) {
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.delete(&key).await {
                tracing::warn!("CachedJsStorage: failed to delete key {}: {:?}", key, e);
            }
        });
    }
}

impl KeyValueStore for CachedJsStorage {
    fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let cache = self
            .cache
            .read()
            .map_err(|_| -> StorageError { "cache lock poisoned".into() })?;
        Ok(cache.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        // Update cache
        {
            let mut cache = self
                .cache
                .write()
                .map_err(|_| -> StorageError { "cache lock poisoned".into() })?;
            cache.insert(key.to_string(), value.to_string());
        }

        // Schedule async write to JS storage
        self.schedule_persist(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        // Update cache
        {
            let mut cache = self
                .cache
                .write()
                .map_err(|_| -> StorageError { "cache lock poisoned".into() })?;
            cache.remove(key);
        }

        // Schedule async delete from JS storage
        self.schedule_delete(key.to_string());
        Ok(())
    }

    fn keys(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let cache = self
            .cache
            .read()
            .map_err(|_| -> StorageError { "cache lock poisoned".into() })?;
        Ok(cache
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    fn try_lock(&self) -> Result<bool, StorageError> {
        // On WASM, we can't synchronously call the async JS tryLock().
        // Use the local lock state and schedule the JS lock call asynchronously.
        let mut locked = self
            .locked
            .write()
            .map_err(|_| -> StorageError { "lock state poisoned".into() })?;
        if *locked {
            return Ok(false);
        }
        *locked = true;

        // Schedule the async JS lock call
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.try_lock().await {
                tracing::warn!("CachedJsStorage: JS tryLock() failed: {:?}", e);
            }
        });

        Ok(true)
    }

    fn is_locked(&self) -> Result<bool, StorageError> {
        Ok(*self.locked.read().map_err(|e| e.to_string())?)
    }

    fn unlock(&self) -> Result<(), StorageError> {
        let mut locked = self
            .locked
            .write()
            .map_err(|_| -> StorageError { "lock state poisoned".into() })?;
        *locked = false;

        // Schedule the async JS unlock call
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.unlock().await {
                tracing::warn!("CachedJsStorage: JS unlock() failed: {:?}", e);
            }
        });

        Ok(())
    }
}
