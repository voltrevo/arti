//! Webtor - A Tor client library for web browsers
//!
//! This library provides a Rust implementation of a Tor client that can be
//! compiled to WebAssembly and embedded in web pages. It supports anonymous
//! HTTP/HTTPS requests through the Tor network using Snowflake bridges.

#[cfg(test)]
pub mod test_util;

pub mod circuit;
pub mod client;
pub mod config;
pub mod directory;
pub mod error;
pub mod http;
pub mod isolation;
pub mod kcp_stream;
pub mod relay;
pub mod retry;
pub mod smux;
pub mod snowflake;
pub mod snowflake_broker;
pub mod snowflake_ws;
pub mod time;
pub mod tls;
pub mod turbo;
pub mod wasm_runtime;
pub mod websocket;

#[cfg(not(target_arch = "wasm32"))]
pub mod webtunnel;

#[cfg(target_arch = "wasm32")]
pub mod webrtc_stream;

// Arti-client integration (WASM only)
#[cfg(all(target_arch = "wasm32", feature = "arti-integration"))]
pub mod arti_transport;

pub use client::TorClient;
pub use config::TorClientOptions;
pub use error::{Result, TorError, TorErrorKind};
pub use isolation::{IsolationKey, StreamIsolationPolicy};
pub use retry::{
    retry_with_backoff, with_cancellation, with_timeout, with_timeout_and_cancellation,
    CancellationToken, RetryPolicy,
};

// Re-export commonly used types
pub use http::HttpResponse;
pub use url::Url;

// Re-export Tor stream types for advanced usage
pub use tor_proto::client::stream::DataStream;

// Re-export arti-client integration types
#[cfg(all(target_arch = "wasm32", feature = "arti-integration"))]
pub use arti_transport::{SnowflakeChannelFactory, SnowflakeMode, SnowflakePtMgr};
