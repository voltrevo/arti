// @@ begin example lint list maintained by maint/add_warning @@
#![allow(unknown_lints)] // @@REMOVE_WHEN(ci_arti_nightly)
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::dbg_macro)]
#![allow(clippy::mixed_attributes_style)]
#![allow(clippy::print_stderr)]
#![allow(clippy::print_stdout)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::unchecked_time_subtraction)]
#![allow(clippy::useless_vec)]
#![allow(clippy::needless_pass_by_value)]
//! <!-- @@ end example lint list maintained by maint/add_warning @@ -->

//! Example: using a custom storage backend with arti-client.
//!
//! This example demonstrates how to implement `KeyValueStore` to provide
//! a single custom storage backend for both state and directory cache.
//! Data is stored as individual files under a local `custom-storage/` directory.

use anyhow::Result;
use arti_client::storage::StorageError;
use arti_client::{KeyValueStore, TorClient, TorClientConfig};
use tokio_crate as tokio;

use futures::io::{AsyncReadExt, AsyncWriteExt};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// A simple file-backed implementation of `KeyValueStore`.
///
/// Each key is stored as a file under `{dir}/{sanitized_key}`.
/// Locking is tracked in-memory (a real implementation might use file locks).
struct FileStore {
    dir: PathBuf,
    locked: RwLock<bool>,
}

impl FileStore {
    fn new(dir: &Path) -> Self {
        fs::create_dir_all(dir).expect("failed to create storage dir");
        // Ignore storage files in git
        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, "*\n").expect("failed to create .gitignore");
        }
        Self {
            dir: dir.to_owned(),
            locked: RwLock::new(false),
        }
    }

    /// Hex-escape non-alphanumeric bytes as `_XX` to produce a safe filename.
    /// e.g. `"state:guards"` -> `"state_3aguards"`, `"a_b"` -> `"a_5fb"`
    fn escape(key: &str) -> String {
        let mut safe = String::with_capacity(key.len());
        for b in key.bytes() {
            if b.is_ascii_alphanumeric() {
                safe.push(b as char);
            } else {
                safe.push_str(&format!("_{:02x}", b));
            }
        }
        safe
    }

    /// Reverse the hex-escaping to recover the original key.
    fn unescape(safe: &str) -> Option<String> {
        let mut key = Vec::new();
        let bytes = safe.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'_' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                key.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            } else {
                key.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(key).ok()
    }

    fn key_path(&self, key: &str) -> PathBuf {
        self.dir.join(Self::escape(key))
    }
}

impl KeyValueStore for FileStore {
    fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let path = self.key_path(key);
        match fs::read_to_string(&path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let path = self.key_path(key);
        fs::write(&path, value)?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.key_path(key);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn keys(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let safe_prefix = Self::escape(prefix);
        let mut result = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&safe_prefix) {
                        if let Some(key) = Self::unescape(name) {
                            result.push(key);
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    fn try_lock(&self) -> Result<bool, StorageError> {
        let mut locked = self.locked.write().map_err(|e| e.to_string())?;
        if *locked {
            Ok(false) // already held
        } else {
            *locked = true;
            Ok(true) // newly acquired
        }
    }

    fn is_locked(&self) -> Result<bool, StorageError> {
        Ok(*self.locked.read().map_err(|e| e.to_string())?)
    }

    fn unlock(&self) -> Result<(), StorageError> {
        *self.locked.write().map_err(|e| e.to_string())? = false;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = TorClientConfig::default();

    let storage_dir = PathBuf::from("custom_storage");
    let store = FileStore::new(&storage_dir);

    eprintln!("using custom storage in {}/", storage_dir.display());
    eprintln!("connecting to Tor...");

    let tor_client = TorClient::builder()
        .config(config)
        .storage(store)
        .create_bootstrapped()
        .await?;

    eprintln!("connecting to example.com...");

    let mut stream = tor_client.connect(("example.com", 80)).await?;

    eprintln!("sending request...");

    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
        .await?;

    stream.flush().await?;

    eprintln!("reading response...");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    println!("{}", String::from_utf8_lossy(&buf));

    Ok(())
}
