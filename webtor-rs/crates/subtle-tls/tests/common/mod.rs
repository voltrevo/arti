//! Shared test utilities for integration tests
//!
//! This module provides portable test attributes that work on both WASM and native targets,
//! as well as WASM-only test attributes for tests that require SubtleCrypto APIs.

// Re-export the appropriate test attribute based on target
#[allow(unused_imports)]
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::wasm_bindgen_test as portable_test;

#[allow(unused_imports)]
#[cfg(not(target_arch = "wasm32"))]
pub use core::prelude::rust_2021::test as portable_test;

// For async tests: wasm_bindgen_test on WASM, tokio::test on native
#[allow(unused_imports)]
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen_test::wasm_bindgen_test as portable_test_async;

#[allow(unused_imports)]
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::test as portable_test_async;
