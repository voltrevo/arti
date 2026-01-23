//! Test utilities for cross-platform testing (WASM and native)
//!
//! This module provides portable test attributes that work on both WASM and native targets.
//!
//! # Usage
//! ```ignore
//! use crate::test_util::{portable_test, portable_test_async};
//!
//! #[portable_test]
//! fn my_sync_test() {
//!     assert!(true);
//! }
//!
//! #[portable_test_async]
//! async fn my_async_test() {
//!     assert!(true);
//! }
//! ```

// Re-export the appropriate test attribute based on target
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::wasm_bindgen_test as portable_test;

#[cfg(not(target_arch = "wasm32"))]
pub use core::prelude::rust_2021::test as portable_test;

// For async tests: wasm_bindgen_test on WASM, tokio::test on native
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::wasm_bindgen_test as portable_test_async;

#[cfg(not(target_arch = "wasm32"))]
pub use tokio::test as portable_test_async;