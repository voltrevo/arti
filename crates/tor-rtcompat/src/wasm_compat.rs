//! WASM compatibility traits that shadow std::marker::{Send, Sync}.
//!
//! On native targets, these re-export the real `Send` and `Sync` from std.
//! On WASM targets, these are auto-implemented empty traits (since WASM is single-threaded).
//!
//! # Usage
//! ```ignore
//! use tor_rtcompat::wasm_compat::{Send, Sync};
//!
//! // Now you can use Send/Sync in bounds and they'll be no-ops on WASM
//! pub trait MyTrait: Send + Sync {
//!     // ...
//! }
//! ```

// On native: re-export the real Send and Sync
#[cfg(not(target_arch = "wasm32"))]
pub use std::marker::{Send, Sync};

// On WASM: provide empty traits that everything implements
/// Marker trait for types safe to transfer across threads.
/// On WASM, this is auto-implemented for all types since WASM is single-threaded.
#[cfg(target_arch = "wasm32")]
pub trait Send {}

#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> Send for T {}

/// Marker trait for types safe to share between threads.
/// On WASM, this is auto-implemented for all types since WASM is single-threaded.
#[cfg(target_arch = "wasm32")]
pub trait Sync {}

#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> Sync for T {}