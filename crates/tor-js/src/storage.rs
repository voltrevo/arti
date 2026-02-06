//! JavaScript storage adapter for tor-js.
//!
//! This module provides adapters that allow JavaScript code to implement
//! custom storage backends (IndexedDB, filesystem, etc.) for the Tor client.
//!
//! # Architecture
//!
//! The storage system bridges async JavaScript storage APIs with Rust's sync
//! storage traits using a pre-load + cache + async write-back pattern:
//!
//! 1. During client creation (async), all data is loaded from JS storage
//! 2. Sync reads hit the in-memory cache
//! 3. Writes update the cache and schedule async persistence via spawn_local()

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
}

// ============================================================================
// JsStateMgr - StateMgr implementation backed by JS storage
// ============================================================================

use tor_persist::{CustomStateMgr, ErrorSource, LockStatus};

/// State manager backed by JavaScript storage.
///
/// This implements the `CustomStateMgr` trait using a JS storage backend.
/// It uses a pre-load + cache pattern to handle the async-to-sync bridge.
#[derive(Clone)]
pub struct JsStateMgr {
    /// The underlying JS storage.
    js_storage: JsStorage,
    /// In-memory cache for sync reads.
    cache: Arc<RwLock<HashMap<String, String>>>,
    /// Whether we hold the "lock" (always granted in WASM).
    locked: Arc<RwLock<bool>>,
    /// Key prefix for state data.
    key_prefix: String,
}

// SAFETY: WASM is single-threaded, so it's safe to send JsStateMgr between "threads"
// (there's only one thread). The JsStorage inside contains JsValue which is not Send/Sync,
// but since WASM has no threads, this is safe.
unsafe impl Send for JsStateMgr {}
unsafe impl Sync for JsStateMgr {}

impl JsStateMgr {
    /// Create a new JsStateMgr and pre-load all state data.
    pub async fn new(js_storage: JsStorage) -> Result<Self, JsValue> {
        let mgr = Self {
            js_storage,
            cache: Arc::new(RwLock::new(HashMap::new())),
            locked: Arc::new(RwLock::new(false)),
            key_prefix: "state:".to_string(),
        };

        // Pre-load all state keys from JS storage
        mgr.preload_all().await?;

        Ok(mgr)
    }

    /// Pre-load all state data from JS storage into the cache.
    async fn preload_all(&self) -> Result<(), JsValue> {
        let keys = self.js_storage.keys(&self.key_prefix).await?;
        let mut cache = self
            .cache
            .write()
            .map_err(|_| JsValue::from_str("cache lock poisoned"))?;

        for key in keys {
            if let Some(value) = self.js_storage.get(&key).await? {
                // Store with the full key (including prefix)
                cache.insert(key, value);
            }
        }

        tracing::debug!("JsStateMgr: preloaded {} state entries", cache.len());
        Ok(())
    }

    /// Get the full storage key for a given state key.
    fn full_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }

    /// Schedule an async write to JS storage.
    fn schedule_persist(&self, key: String, value: String) {
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.set(&key, &value).await {
                tracing::warn!("JsStateMgr: failed to persist key {}: {:?}", key, e);
            }
        });
    }

    /// Schedule an async delete from JS storage.
    fn schedule_delete(&self, key: String) {
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.delete(&key).await {
                tracing::warn!("JsStateMgr: failed to delete key {}: {:?}", key, e);
            }
        });
    }

}

impl CustomStateMgr for JsStateMgr {
    fn load_json(&self, key: &str) -> tor_persist::Result<Option<String>> {
        let full_key = self.full_key(key);
        let cache = self
            .cache
            .read()
            .map_err(|_| tor_persist::Error::load_error(key, ErrorSource::NoLock))?;

        Ok(cache.get(&full_key).cloned())
    }

    fn store_json(&self, key: &str, value: &str) -> tor_persist::Result<()> {
        if !self.can_store() {
            return Err(tor_persist::Error::store_error(key, ErrorSource::NoLock));
        }

        let full_key = self.full_key(key);

        // Update cache
        {
            let mut cache = self.cache.write().map_err(|_| {
                tor_persist::Error::store_error(key, ErrorSource::NoLock)
            })?;
            cache.insert(full_key.clone(), value.to_string());
        }

        // Schedule async write to JS storage
        self.schedule_persist(full_key, value.to_string());

        Ok(())
    }

    fn can_store(&self) -> bool {
        self.locked.read().map(|l| *l).unwrap_or(false)
    }

    fn try_lock(&self) -> tor_persist::Result<LockStatus> {
        let mut locked = self
            .locked
            .write()
            .map_err(|_| tor_persist::Error::lock_error(ErrorSource::NoLock))?;

        if *locked {
            Ok(LockStatus::AlreadyHeld)
        } else {
            *locked = true;
            Ok(LockStatus::NewlyAcquired)
        }
    }

    fn unlock(&self) -> tor_persist::Result<()> {
        let mut locked = self
            .locked
            .write()
            .map_err(|_| tor_persist::Error::unlock_error(ErrorSource::NoLock))?;

        *locked = false;
        Ok(())
    }
}

// ============================================================================
// JsDirStore - CustomDirStore implementation backed by JS storage
// ============================================================================

use tor_dirmgr::CustomDirStore;

/// Directory store backed by JavaScript storage.
///
/// This implements the `CustomDirStore` trait using a JS storage backend.
/// It uses a pre-load + cache pattern to handle the async-to-sync bridge.
///
/// Key prefixes:
/// - `dir:consensus:{flavor}:{sha3_hex}` - Consensus documents
/// - `dir:authcert:{id_hex}:{sk_hex}` - Authority certificates
/// - `dir:microdesc:{digest_hex}` - Microdescriptors
/// - `dir:bridge:{hash}` - Bridge descriptors
/// - `dir:protocols` - Protocol recommendations
#[derive(Clone)]
pub struct JsDirStore {
    /// The underlying JS storage.
    js_storage: JsStorage,
    /// In-memory cache for sync reads.
    cache: Arc<RwLock<HashMap<String, String>>>,
    /// Whether the store is read-only.
    readonly: bool,
    /// Key prefix for directory data.
    key_prefix: String,
}

// SAFETY: WASM is single-threaded, so it's safe to send JsDirStore between "threads"
// (there's only one thread). The JsStorage inside contains JsValue which is not Send/Sync,
// but since WASM has no threads, this is safe.
unsafe impl Send for JsDirStore {}
unsafe impl Sync for JsDirStore {}

impl JsDirStore {
    /// Create a new JsDirStore and pre-load all directory data.
    pub async fn new(js_storage: JsStorage, readonly: bool) -> Result<Self, JsValue> {
        let store = Self {
            js_storage,
            cache: Arc::new(RwLock::new(HashMap::new())),
            readonly,
            key_prefix: "dir:".to_string(),
        };

        // Pre-load all directory keys from JS storage
        store.preload_all().await?;

        Ok(store)
    }

    /// Pre-load all directory data from JS storage into the cache.
    async fn preload_all(&self) -> Result<(), JsValue> {
        let keys = self.js_storage.keys(&self.key_prefix).await?;
        let mut cache = self
            .cache
            .write()
            .map_err(|_| JsValue::from_str("cache lock poisoned"))?;

        for key in keys {
            if let Some(value) = self.js_storage.get(&key).await? {
                cache.insert(key, value);
            }
        }

        tracing::debug!("JsDirStore: preloaded {} directory entries", cache.len());
        Ok(())
    }

    /// Schedule an async write to JS storage.
    fn schedule_persist(&self, key: String, value: String) {
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.set(&key, &value).await {
                tracing::warn!("JsDirStore: failed to persist key {}: {:?}", key, e);
            }
        });
    }

    /// Schedule an async delete from JS storage.
    fn schedule_delete(&self, key: String) {
        let js_storage = self.js_storage.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = js_storage.delete(&key).await {
                tracing::warn!("JsDirStore: failed to delete key {}: {:?}", key, e);
            }
        });
    }
}

impl CustomDirStore for JsDirStore {
    fn load(&self, key: &str) -> tor_dirmgr::Result<Option<String>> {
        let cache = self.cache.read().map_err(|_| {
            tor_dirmgr::Error::CacheCorruption("cache lock poisoned")
        })?;
        Ok(cache.get(key).cloned())
    }

    fn store(&self, key: &str, value: &str) -> tor_dirmgr::Result<()> {
        if self.readonly {
            return Err(tor_dirmgr::Error::CacheCorruption("store is read-only"));
        }

        // Update cache
        {
            let mut cache = self.cache.write().map_err(|_| {
                tor_dirmgr::Error::CacheCorruption("cache lock poisoned")
            })?;
            cache.insert(key.to_string(), value.to_string());
        }

        // Schedule async write to JS storage
        self.schedule_persist(key.to_string(), value.to_string());

        Ok(())
    }

    fn delete(&self, key: &str) -> tor_dirmgr::Result<()> {
        if self.readonly {
            return Err(tor_dirmgr::Error::CacheCorruption("store is read-only"));
        }

        // Update cache
        {
            let mut cache = self.cache.write().map_err(|_| {
                tor_dirmgr::Error::CacheCorruption("cache lock poisoned")
            })?;
            cache.remove(key);
        }

        // Schedule async delete from JS storage
        self.schedule_delete(key.to_string());

        Ok(())
    }

    fn keys(&self, prefix: &str) -> tor_dirmgr::Result<Vec<String>> {
        let cache = self.cache.read().map_err(|_| {
            tor_dirmgr::Error::CacheCorruption("cache lock poisoned")
        })?;

        let matching: Vec<String> = cache
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();

        Ok(matching)
    }

    fn is_readonly(&self) -> bool {
        self.readonly
    }

    fn upgrade_to_readwrite(&mut self) -> tor_dirmgr::Result<bool> {
        // FIXME: This always grants the lock, but multiple browser tabs or Node.js
        // processes could share the same IndexedDB/filesystem storage. We should add
        // locking methods to TorStorage (tryLock/unlock) and implement proper advisory
        // locking - e.g., Web Locks API for browser, lock files for Node.js.
        // For now, concurrent instances may corrupt each other's data.
        self.readonly = false;
        Ok(true)
    }
}
