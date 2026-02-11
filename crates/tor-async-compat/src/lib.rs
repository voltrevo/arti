//! WASM compatibility macros for async traits.
//!
//! This crate provides `#[async_trait]` which expands to:
//! - `#[async_trait::async_trait]` on native (requires Send futures)
//! - `#[async_trait::async_trait(?Send)]` on WASM (allows non-Send futures)
//!
//! Use `use tor_async_compat::async_trait;` instead of `use async_trait::async_trait;`

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Item};

/// Attribute macro that applies `#[async_trait]` with conditional Send bounds.
///
/// On native targets, this expands to `#[async_trait::async_trait]` which requires futures to be Send.
/// On WASM targets, this expands to `#[async_trait::async_trait(?Send)]` which allows non-Send futures.
///
/// # Example
/// ```ignore
/// use tor_async_compat::async_trait;
///
/// #[async_trait]
/// pub trait MyTrait {
///     async fn do_something(&self);
/// }
/// ```
#[proc_macro_attribute]
pub fn async_trait(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as Item);

    let output = quote! {
        #[cfg_attr(not(target_arch = "wasm32"), ::async_trait::async_trait)]
        #[cfg_attr(target_arch = "wasm32", ::async_trait::async_trait(?Send))]
        #item
    };

    output.into()
}